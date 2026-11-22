//! Comandos IPC. Cada uno tiene su permiso `allow-<comando>` (ver `build.rs`).
//!
//! Todos los comandos que tocan procesos originan acciones de la persona
//! usuaria (`Origin::User`): la UI es el único cliente en esta fase. Los
//! agentes (fase 4) entrarán por otra vía, sujeta al motor de políticas.

use crate::state::AppContext;
use mksh_config::{Appearance, TerminalSettings};
use mksh_core::command::{CommandId, COMMANDS};
use mksh_core::{Origin, PaneId, SessionId, SplitDirection, SplitId};
use mksh_session::{PaneFrame, SessionInfo};
use mksh_shell::Profile;
use mksh_store::HistoryEntry;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::State;
use tauri_plugin_dialog::DialogExt;

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigView {
    appearance: Appearance,
    terminal: TerminalSettings,
    keybindings: BTreeMap<String, String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    version: &'static str,
    platform: &'static str,
    config: ConfigView,
    config_path: String,
    config_error: Option<String>,
    profiles: Vec<Profile>,
    sessions: Vec<SessionInfo>,
    commands: &'static [CommandId],
    ssh_trusted_hosts: Vec<String>,
    history_available: bool,
}

#[tauri::command]
pub fn get_bootstrap(ctx: State<'_, AppContext>) -> Bootstrap {
    Bootstrap {
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        config: ConfigView {
            appearance: ctx.config.appearance.clone(),
            terminal: ctx.config.terminal.clone(),
            keybindings: ctx.config.effective_keybindings(),
        },
        config_path: ctx.paths.config_file().to_string_lossy().into_owned(),
        config_error: ctx.config_error.clone(),
        profiles: ctx.manager.profiles(),
        sessions: ctx.manager.sessions(),
        commands: COMMANDS,
        ssh_trusted_hosts: ctx.state.lock().ssh_trusted_hosts.clone(),
        history_available: ctx.history.is_some(),
    }
}

#[tauri::command]
pub fn create_session(
    ctx: State<'_, AppContext>,
    profile_id: Option<String>,
    name: Option<String>,
) -> Res<SessionInfo> {
    ctx.manager
        .create_session(profile_id.as_deref(), name, None)
        .map_err(err)
}

#[tauri::command]
pub fn close_session(ctx: State<'_, AppContext>, session_id: SessionId) -> Res<()> {
    ctx.manager.close_session(session_id).map_err(err)
}

#[tauri::command]
pub fn rename_session(ctx: State<'_, AppContext>, session_id: SessionId, name: String) -> Res<()> {
    ctx.manager.rename_session(session_id, name).map_err(err)
}

#[tauri::command]
pub fn split_pane(
    ctx: State<'_, AppContext>,
    pane_id: PaneId,
    direction: SplitDirection,
    profile_id: Option<String>,
) -> Res<SessionInfo> {
    ctx.manager
        .split_pane(pane_id, direction, profile_id.as_deref())
        .map_err(err)
}

#[tauri::command]
pub fn close_pane(ctx: State<'_, AppContext>, pane_id: PaneId) -> Res<()> {
    ctx.manager.close_pane(pane_id).map_err(err)
}

#[tauri::command]
pub fn set_split_ratio(
    ctx: State<'_, AppContext>,
    session_id: SessionId,
    split_id: SplitId,
    ratio: f32,
) -> Res<()> {
    ctx.manager
        .set_ratio(session_id, split_id, ratio)
        .map_err(err)
}

/// Conecta la salida de un panel a un canal de la WebView (mensajes binarios,
/// formato en `mksh_session::frame`).
#[tauri::command]
pub fn attach_pane(
    ctx: State<'_, AppContext>,
    pane_id: PaneId,
    on_output: Channel<InvokeResponseBody>,
) -> Res<()> {
    let pane = ctx.manager.pane(pane_id).map_err(err)?;
    pane.attach(Arc::new(move |frame: &PaneFrame| {
        on_output
            .send(InvokeResponseBody::Raw(frame.encode()))
            .is_ok()
    }));
    Ok(())
}

#[tauri::command]
pub fn run_command(ctx: State<'_, AppContext>, pane_id: PaneId, text: String) -> Res<bool> {
    ctx.manager
        .pane(pane_id)
        .map_err(err)?
        .run_command(&text, Origin::User)
        .map_err(err)
}

#[tauri::command]
pub fn write_input(ctx: State<'_, AppContext>, pane_id: PaneId, data: String) -> Res<()> {
    ctx.manager
        .pane(pane_id)
        .map_err(err)?
        .write_input(data.as_bytes())
        .map_err(err)
}

#[tauri::command]
pub fn resize_pane(ctx: State<'_, AppContext>, pane_id: PaneId, cols: u16, rows: u16) -> Res<()> {
    ctx.manager
        .pane(pane_id)
        .map_err(err)?
        .resize(cols.clamp(2, 1000), rows.clamp(1, 500))
        .map_err(err)
}

#[tauri::command]
pub fn ack_output(ctx: State<'_, AppContext>, pane_id: PaneId, bytes: usize) {
    if let Ok(p) = ctx.manager.pane(pane_id) {
        p.ack(bytes);
    }
}

#[tauri::command]
pub fn history(
    ctx: State<'_, AppContext>,
    profile_id: Option<String>,
    prefix: Option<String>,
    limit: Option<usize>,
) -> Res<Vec<HistoryEntry>> {
    let Some(h) = &ctx.history else {
        return Ok(Vec::new());
    };
    h.recent_unique(
        profile_id.as_deref(),
        prefix.as_deref().filter(|p| !p.is_empty()),
        limit.unwrap_or(200).min(2000),
    )
    .map_err(err)
}

/// Completa una ruta relativa al cwd del panel. Devuelve el texto completo de
/// cada candidato (directorios con separador final).
#[tauri::command]
pub fn complete_path(
    ctx: State<'_, AppContext>,
    pane_id: PaneId,
    prefix: String,
) -> Res<Vec<String>> {
    let pane = ctx.manager.pane(pane_id).map_err(err)?;
    let base = pane
        .local_cwd()
        .or_else(home_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    Ok(complete_in(&base, &prefix, 200))
}

/// Envía el arranque de la integración al shell en primer plano (SSH, WSL…).
/// La UI pide consentimiento antes; `remember` guarda el host como autorizado.
#[tauri::command]
pub fn enable_blocks(
    ctx: State<'_, AppContext>,
    pane_id: PaneId,
    host: Option<String>,
    remember: bool,
) -> Res<Vec<String>> {
    ctx.manager
        .pane(pane_id)
        .map_err(err)?
        .enable_integration()
        .map_err(err)?;
    let mut st = ctx.state.lock();
    if let (true, Some(h)) = (remember, host.as_deref()) {
        if st.trust_host(h) {
            st.save(&ctx.paths.state_file()).map_err(err)?;
        }
    }
    log::info!(
        "integración enviada al panel {pane_id} (host: {})",
        host.as_deref().unwrap_or("-")
    );
    Ok(st.ssh_trusted_hosts.clone())
}

/// Guarda texto en un fichero elegido con el diálogo nativo.
#[tauri::command]
pub async fn save_text_file(
    app: tauri::AppHandle,
    suggested_name: String,
    content: String,
) -> Res<Option<String>> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .set_file_name(&suggested_name)
        .save_file(move |p| {
            let _ = tx.send(p);
        });
    let picked = tauri::async_runtime::spawn_blocking(move || rx.recv().ok().flatten())
        .await
        .map_err(err)?;
    let Some(path) = picked else { return Ok(None) };
    let path = path.into_path().map_err(err)?;
    std::fs::write(&path, content).map_err(err)?;
    Ok(Some(path.to_string_lossy().into_owned()))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

fn complete_in(base: &Path, prefix: &str, limit: usize) -> Vec<String> {
    let sep = std::path::MAIN_SEPARATOR;
    // Expande `~` y separa directorio y fragmento del nombre.
    let (shown_dir, fragment) = match prefix.rfind(['/', '\\']) {
        Some(i) => (&prefix[..=i], &prefix[i + 1..]),
        None => ("", prefix),
    };
    let dir = if shown_dir.is_empty() {
        base.to_path_buf()
    } else if let Some(rest) = shown_dir.strip_prefix('~') {
        match home_dir() {
            Some(h) => h.join(rest.trim_start_matches(['/', '\\'])),
            None => return Vec::new(),
        }
    } else {
        let p = PathBuf::from(shown_dir);
        if p.is_absolute() {
            p
        } else {
            base.join(p)
        }
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let show_hidden = fragment.starts_with('.');
    let mut out: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with(fragment) || (!show_hidden && name.starts_with('.')) {
                return None;
            }
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            Some(format!(
                "{shown_dir}{name}{}",
                if is_dir {
                    sep.to_string()
                } else {
                    String::new()
                }
            ))
        })
        .collect();
    out.sort();
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completes_relative_and_hidden() {
        let dir = std::env::temp_dir().join(format!("mksh-compl-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src/sub")).unwrap();
        std::fs::write(dir.join("src/main.rs"), "").unwrap();
        std::fs::write(dir.join(".hidden"), "").unwrap();
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(complete_in(&dir, "sr", 10), vec![format!("src{sep}")]);
        assert_eq!(
            complete_in(&dir, "src/", 10),
            vec!["src/main.rs".to_string(), format!("src/sub{sep}")]
        );
        assert!(complete_in(&dir, "", 10)
            .iter()
            .all(|e| !e.starts_with('.')));
        assert_eq!(complete_in(&dir, ".h", 10), vec![".hidden".to_string()]);
        std::fs::remove_dir_all(dir).ok();
    }
}
