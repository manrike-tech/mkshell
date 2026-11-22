//! Persistencia local: historial de comandos en SQLite.
//!
//! La auditoría de acciones de agentes (fase 4) vivirá aquí también, como
//! JSONL append-only con hash encadenado.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("base de datos: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("E/S: {0}")]
    Io(#[from] std::io::Error),
}

/// Entrada del historial.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: i64,
    /// Milisegundos desde la época Unix.
    pub ts: i64,
    pub profile: String,
    pub command: String,
    pub cwd: Option<String>,
    pub host: Option<String>,
    pub exit_code: Option<i32>,
    pub duration_ms: i64,
    /// `user` o `agent`.
    pub origin: String,
}

/// Datos para insertar una entrada.
#[derive(Debug, Clone)]
pub struct NewEntry<'a> {
    pub profile: &'a str,
    pub command: &'a str,
    pub cwd: Option<&'a str>,
    pub host: Option<&'a str>,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub origin: &'a str,
}

const SCHEMA_VERSION: i64 = 1;

pub struct History {
    conn: Mutex<Connection>,
}

impl History {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Self, StoreError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, StoreError> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version < 1 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS history (
                    id          INTEGER PRIMARY KEY,
                    ts          INTEGER NOT NULL,
                    profile     TEXT NOT NULL,
                    command     TEXT NOT NULL,
                    cwd         TEXT,
                    host        TEXT,
                    exit_code   INTEGER,
                    duration_ms INTEGER NOT NULL DEFAULT 0,
                    origin      TEXT NOT NULL DEFAULT 'user'
                );
                CREATE INDEX IF NOT EXISTS history_ts ON history(ts DESC);
                CREATE INDEX IF NOT EXISTS history_cmd ON history(command);",
            )?;
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Añade un comando. Las repeticiones consecutivas del mismo comando en el
    /// mismo perfil actualizan la entrada existente en lugar de duplicarla.
    pub fn add(&self, e: &NewEntry<'_>) -> Result<i64, StoreError> {
        let command = e.command.trim();
        if command.is_empty() {
            return Ok(0);
        }
        let conn = self.conn.lock().expect("history");
        let ts = now_ms();
        let last: Option<(i64, String, String)> = conn
            .query_row(
                "SELECT id, profile, command FROM history ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((id, _, _)) =
            last.filter(|(_, profile, cmd)| profile == e.profile && cmd == command)
        {
            conn.execute(
                "UPDATE history SET ts=?1, cwd=?2, host=?3, exit_code=?4, duration_ms=?5, origin=?6 WHERE id=?7",
                params![ts, e.cwd, e.host, e.exit_code, e.duration_ms as i64, e.origin, id],
            )?;
            return Ok(id);
        }
        conn.execute(
            "INSERT INTO history (ts, profile, command, cwd, host, exit_code, duration_ms, origin)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                ts,
                e.profile,
                command,
                e.cwd,
                e.host,
                e.exit_code,
                e.duration_ms as i64,
                e.origin
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Comandos únicos más recientes (el más reciente primero), opcionalmente
    /// filtrados por perfil y por prefijo.
    pub fn recent_unique(
        &self,
        profile: Option<&str>,
        prefix: Option<&str>,
        limit: usize,
    ) -> Result<Vec<HistoryEntry>, StoreError> {
        let conn = self.conn.lock().expect("history");
        let like = prefix.map(|p| format!("{}%", escape_like(p)));
        let mut stmt = conn.prepare(
            "SELECT h.id, h.ts, h.profile, h.command, h.cwd, h.host, h.exit_code, h.duration_ms, h.origin
             FROM history h
             JOIN (SELECT command, MAX(id) AS id FROM history
                   WHERE (?1 IS NULL OR profile = ?1)
                     AND (?2 IS NULL OR command LIKE ?2 ESCAPE '\\')
                   GROUP BY command) last ON last.id = h.id
             ORDER BY h.id DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![profile, like, limit as i64], |r| {
            Ok(HistoryEntry {
                id: r.get(0)?,
                ts: r.get(1)?,
                profile: r.get(2)?,
                command: r.get(3)?,
                cwd: r.get(4)?,
                host: r.get(5)?,
                exit_code: r.get(6)?,
                duration_ms: r.get(7)?,
                origin: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn count(&self) -> Result<i64, StoreError> {
        let conn = self.conn.lock().expect("history");
        Ok(conn.query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0))?)
    }
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add(h: &History, profile: &str, cmd: &str) {
        h.add(&NewEntry {
            profile,
            command: cmd,
            cwd: Some("/tmp"),
            host: None,
            exit_code: Some(0),
            duration_ms: 5,
            origin: "user",
        })
        .unwrap();
    }

    #[test]
    fn dedupes_consecutive_and_lists_unique() {
        let h = History::in_memory().unwrap();
        add(&h, "bash", "ls");
        add(&h, "bash", "ls");
        add(&h, "bash", "git status");
        add(&h, "bash", "ls");
        add(&h, "zsh", "echo z");
        assert_eq!(h.count().unwrap(), 4);
        let cmds: Vec<String> = h
            .recent_unique(Some("bash"), None, 10)
            .unwrap()
            .into_iter()
            .map(|e| e.command)
            .collect();
        assert_eq!(cmds, vec!["ls", "git status"]);
        let all: Vec<String> = h
            .recent_unique(None, None, 10)
            .unwrap()
            .into_iter()
            .map(|e| e.command)
            .collect();
        assert_eq!(all, vec!["echo z", "ls", "git status"]);
    }

    #[test]
    fn prefix_search_escapes_wildcards() {
        let h = History::in_memory().unwrap();
        add(&h, "bash", "git status");
        add(&h, "bash", "git_x");
        add(&h, "bash", "gitx");
        let r: Vec<String> = h
            .recent_unique(None, Some("git_"), 10)
            .unwrap()
            .into_iter()
            .map(|e| e.command)
            .collect();
        assert_eq!(r, vec!["git_x"]);
    }

    #[test]
    fn persists_on_disk() {
        let dir = std::env::temp_dir().join(format!("mksh-hist-{}", std::process::id()));
        let path = dir.join("history.db");
        {
            let h = History::open(&path).unwrap();
            add(&h, "bash", "uptime");
        }
        let h = History::open(&path).unwrap();
        assert_eq!(h.recent_unique(None, None, 5).unwrap()[0].command, "uptime");
        std::fs::remove_dir_all(dir).ok();
    }
}
