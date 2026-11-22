//! Configuración de usuario (TOML) y estado persistente de la aplicación.
//!
//! * `config.toml`: lo edita la persona usuaria (apariencia, perfiles, atajos).
//!   MKShell nunca lo reescribe; si no existe, crea uno comentado.
//! * `state.toml`: lo gestiona MKShell (hosts SSH autorizados…).
//!
//! Rutas: Linux `~/.config/mkshell/` y `~/.local/share/mkshell/`;
//! Windows `%APPDATA%\MKShell\` y `%LOCALAPPDATA%\MKShell\`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("E/S en {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("configuración inválida en {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("no se pudo serializar: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// Orden de los bloques bajo el Command Input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum BlockOrder {
    /// El más reciente justo debajo del input (por defecto).
    #[default]
    NewestFirst,
    /// Cronológico: el más reciente al final.
    OldestFirst,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Appearance {
    /// `mk-dark` o `mk-light`.
    pub theme: String,
    pub font_family: String,
    pub font_size: f32,
    pub line_height: f32,
    pub block_order: BlockOrder,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: "mk-dark".into(),
            font_family:
                "'JetBrains Mono', 'Cascadia Mono', 'DejaVu Sans Mono', Consolas, monospace".into(),
            font_size: 13.0,
            line_height: 1.2,
            block_order: BlockOrder::NewestFirst,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TerminalSettings {
    /// Líneas de scrollback por bloque y por terminal clásica.
    pub scrollback: u32,
    /// Milisegundos sin marcas OSC 133 antes de pasar a modo TTY clásico.
    pub tty_fallback_ms: u32,
    /// Bloques visibles por panel antes de descartar los más antiguos.
    pub max_blocks: u32,
    /// Copiar automáticamente al seleccionar texto.
    pub copy_on_select: bool,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            scrollback: 10_000,
            tty_fallback_ms: 3_000,
            max_blocks: 500,
            copy_on_select: false,
        }
    }
}

/// Perfil declarado por la persona usuaria.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// `linux` o `windows`; si se omite, aplica a ambas.
    #[serde(default)]
    pub platform: Option<String>,
    /// Desactivar para lanzar sin integración (modo TTY clásico).
    #[serde(default = "yes")]
    pub integration: bool,
}

fn yes() -> bool {
    true
}

impl ProfileConfig {
    pub fn applies_here(&self) -> bool {
        match self.platform.as_deref() {
            None => true,
            Some(p) => p.eq_ignore_ascii_case(std::env::consts::OS),
        }
    }
}

/// Configuración completa.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub appearance: Appearance,
    pub terminal: TerminalSettings,
    /// Id (o nombre) del perfil por defecto.
    pub default_profile: Option<String>,
    pub profiles: Vec<ProfileConfig>,
    /// Atajo → comando. Sustituye o amplía los atajos por defecto; el valor
    /// `"none"` desactiva un atajo por defecto.
    pub keybindings: BTreeMap<String, String>,
}

impl Config {
    /// Atajos efectivos: por defecto + los del usuario.
    pub fn effective_keybindings(&self) -> BTreeMap<String, String> {
        let mut map: BTreeMap<String, String> = default_keybindings()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        for (k, v) in &self.keybindings {
            let key = normalize_chord(k);
            if v == "none" {
                map.remove(&key);
            } else {
                map.insert(key, v.clone());
            }
        }
        map
    }

    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.into(),
            source,
        })?;
        toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.into(),
            source,
        })
    }

    /// Carga la configuración; si no existe, escribe la plantilla comentada.
    /// Un fichero inválido no impide arrancar: se devuelven los valores por
    /// defecto junto con el error para mostrarlo en la UI.
    pub fn load_or_init(path: &Path) -> (Self, Option<ConfigError>) {
        if !path.exists() {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(path, TEMPLATE);
            return (Self::default(), None);
        }
        match Self::load(path) {
            Ok(c) => (c, None),
            Err(e) => (Self::default(), Some(e)),
        }
    }
}

/// Atajos por defecto. `Ctrl+C`/`Ctrl+V` quedan para el shell.
pub fn default_keybindings() -> Vec<(&'static str, &'static str)> {
    vec![
        ("ctrl+shift+t", "session.new"),
        ("ctrl+shift+q", "session.close"),
        ("ctrl+shift+r", "session.rename"),
        ("ctrl+pagedown", "session.next"),
        ("ctrl+pageup", "session.prev"),
        ("ctrl+shift+d", "pane.split_vertical"),
        ("ctrl+shift+e", "pane.split_horizontal"),
        ("ctrl+shift+w", "pane.close"),
        ("ctrl+shift+arrowleft", "pane.focus_left"),
        ("ctrl+shift+arrowright", "pane.focus_right"),
        ("ctrl+shift+arrowup", "pane.focus_up"),
        ("ctrl+shift+arrowdown", "pane.focus_down"),
        ("ctrl+shift+f", "pane.search"),
        ("ctrl+shift+k", "pane.clear"),
        ("ctrl+shift+y", "pane.toggle_tty"),
        ("ctrl+shift+i", "input.focus"),
        ("ctrl+shift+c", "clipboard.copy"),
        ("ctrl+shift+v", "clipboard.paste"),
        ("ctrl+shift+p", "palette.open"),
        ("ctrl+shift+a", "ai.toggle_panel"),
        ("ctrl+shift+m", "mode.toggle"),
    ]
}

/// Normaliza un atajo: minúsculas, modificadores en orden ctrl, alt, shift, meta.
pub fn normalize_chord(chord: &str) -> String {
    let parts: Vec<String> = chord
        .split('+')
        .map(|p| p.trim().to_ascii_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    let Some((key, mods)) = parts.split_last() else {
        return String::new();
    };
    fn alias(x: &str) -> &str {
        match x {
            "control" => "ctrl",
            "option" => "alt",
            "cmd" | "super" | "win" => "meta",
            other => other,
        }
    }
    let mut out: Vec<&str> = Vec::new();
    for m in ["ctrl", "alt", "shift", "meta"] {
        if mods.iter().any(|x| alias(x) == m) {
            out.push(m);
        }
    }
    let key = match key.as_str() {
        "left" => "arrowleft",
        "right" => "arrowright",
        "up" => "arrowup",
        "down" => "arrowdown",
        "esc" => "escape",
        "pgup" => "pageup",
        "pgdown" | "pgdn" => "pagedown",
        "plus" => "+",
        k => k,
    };
    out.push(key);
    out.join("+")
}

/// Estado gestionado por MKShell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppState {
    /// Hosts (`usuario@host[:puerto]`) donde se autorizó activar bloques por SSH.
    pub ssh_trusted_hosts: Vec<String>,
}

impl AppState {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let text = toml::to_string_pretty(self)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|source| ConfigError::Io {
                path: dir.into(),
                source,
            })?;
        }
        // Escritura atómica: fichero temporal + rename.
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text).map_err(|source| ConfigError::Io {
            path: tmp.clone(),
            source,
        })?;
        std::fs::rename(&tmp, path).map_err(|source| ConfigError::Io {
            path: path.into(),
            source,
        })
    }

    pub fn trust_host(&mut self, key: &str) -> bool {
        if self.ssh_trusted_hosts.iter().any(|h| h == key) {
            return false;
        }
        self.ssh_trusted_hosts.push(key.to_string());
        self.ssh_trusted_hosts.sort();
        true
    }

    pub fn untrust_host(&mut self, key: &str) -> bool {
        let before = self.ssh_trusted_hosts.len();
        self.ssh_trusted_hosts.retain(|h| h != key);
        before != self.ssh_trusted_hosts.len()
    }
}

/// Rutas de la aplicación.
#[derive(Debug, Clone)]
pub struct Paths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl Paths {
    /// Rutas estándar del sistema. `MKSHELL_HOME` las sustituye (útil en pruebas).
    pub fn detect() -> Self {
        if let Some(home) = std::env::var_os("MKSHELL_HOME") {
            let base = PathBuf::from(home);
            return Self {
                config_dir: base.join("config"),
                data_dir: base.join("data"),
            };
        }
        let name = if cfg!(windows) { "MKShell" } else { "mkshell" };
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(name);
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(name);
        Self {
            config_dir,
            data_dir,
        }
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }
    pub fn state_file(&self) -> PathBuf {
        self.data_dir.join("state.toml")
    }
    pub fn history_db(&self) -> PathBuf {
        self.data_dir.join("history.db")
    }
    pub fn integration_dir(&self) -> PathBuf {
        self.data_dir.join("shell-integration")
    }
}

/// Plantilla escrita en el primer arranque. Todo comentado: los valores por
/// defecto viven en el código para poder mejorarlos sin migraciones.
pub const TEMPLATE: &str = r#"# Configuración de MKShell. Descomenta y ajusta lo que necesites.
# Los cambios se aplican al reiniciar la aplicación.

# [appearance]
# theme = "mk-dark"            # "mk-dark" | "mk-light"
# font_family = "'JetBrains Mono', monospace"
# font_size = 13
# line_height = 1.2
# block_order = "newest-first" # "newest-first" | "oldest-first"

# [terminal]
# scrollback = 10000
# tty_fallback_ms = 3000       # sin marcas de integración → modo TTY clásico
# max_blocks = 500
# copy_on_select = false

# default_profile = "bash"

# [[profiles]]
# name = "Bash login"
# command = "/bin/bash"
# args = ["-l"]
# cwd = "~/proyectos"
# platform = "linux"
#
# [[profiles]]
# name = "PowerShell"
# command = "pwsh.exe"
# platform = "windows"

# [keybindings]
# "ctrl+shift+d" = "pane.split_vertical"
# "ctrl+shift+e" = "pane.split_horizontal"
# "ctrl+shift+a" = "ai.toggle_panel"
# "ctrl+shift+m" = "mode.toggle"
# "ctrl+shift+p" = "palette.open"
# "ctrl+shift+q" = "none"        # desactiva un atajo por defecto
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_parses_to_defaults() {
        let c: Config = toml::from_str(TEMPLATE).unwrap();
        assert_eq!(c, Config::default());
    }

    #[test]
    fn parses_full_config() {
        let text = r#"
            default_profile = "zsh"
            [appearance]
            theme = "mk-light"
            block_order = "oldest-first"
            [[profiles]]
            name = "Prod"
            command = "/bin/bash"
            args = ["-l"]
            platform = "linux"
            [keybindings]
            "Ctrl+Shift+Left" = "pane.focus_left"
            "ctrl+shift+q" = "none"
        "#;
        let c: Config = toml::from_str(text).unwrap();
        assert_eq!(c.appearance.block_order, BlockOrder::OldestFirst);
        assert!(c.profiles[0].integration);
        let kb = c.effective_keybindings();
        assert_eq!(
            kb.get("ctrl+shift+arrowleft").map(String::as_str),
            Some("pane.focus_left")
        );
        assert!(!kb.contains_key("ctrl+shift+q"));
        assert_eq!(
            kb.get("ctrl+shift+d").map(String::as_str),
            Some("pane.split_vertical")
        );
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(toml::from_str::<Config>("[appearance]\nthme = 'x'").is_err());
    }

    #[test]
    fn default_keybindings_map_to_known_commands() {
        for (chord, cmd) in default_keybindings() {
            assert_eq!(normalize_chord(chord), chord, "{chord} normalizado");
            assert!(
                mksh_core::command::find_command(cmd).is_some(),
                "{cmd} desconocido"
            );
        }
    }

    #[test]
    fn chord_normalization() {
        assert_eq!(normalize_chord("Shift+Ctrl+D"), "ctrl+shift+d");
        assert_eq!(normalize_chord("control+alt+pgdn"), "ctrl+alt+pagedown");
    }

    #[test]
    fn state_roundtrip() {
        let dir = std::env::temp_dir().join(format!("mksh-state-{}", std::process::id()));
        let path = dir.join("state.toml");
        let mut s = AppState::default();
        assert!(s.trust_host("root@pve"));
        assert!(!s.trust_host("root@pve"));
        s.save(&path).unwrap();
        assert_eq!(AppState::load(&path), s);
        std::fs::remove_dir_all(dir).ok();
    }
}
