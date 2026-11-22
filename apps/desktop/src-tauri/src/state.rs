//! Estado de la aplicación: configuración, gestor de sesiones e historial.

use mksh_config::{AppState, Config, Paths};
use mksh_session::{BlockRecord, ManagerOptions, SessionInfo, SessionManager};
use mksh_shell::{detect_profiles, slug, IntegrationAssets, Profile, ShellKind};
use mksh_store::{History, NewEntry};
use parking_lot::Mutex;
use std::sync::Arc;

pub struct AppContext {
    pub manager: SessionManager,
    pub config: Config,
    pub config_error: Option<String>,
    pub paths: Paths,
    pub history: Option<Arc<History>>,
    pub state: Mutex<AppState>,
}

impl AppContext {
    pub fn init(
        on_change: impl Fn(Vec<SessionInfo>) + Send + Sync + 'static,
    ) -> anyhow::Result<Self> {
        let paths = Paths::detect();
        let (config, config_error) = Config::load_or_init(&paths.config_file());
        let config_error = config_error.map(|e| e.to_string());
        if let Some(e) = &config_error {
            log::warn!("{e}");
        }

        let assets = match IntegrationAssets::install(&paths.integration_dir()) {
            Ok(a) => Some(a),
            Err(e) => {
                // Sin scripts, los paneles arrancan en modo TTY clásico.
                log::error!("no se pudieron instalar los scripts de integración: {e}");
                None
            }
        };

        let history = match History::open(&paths.history_db()) {
            Ok(h) => Some(Arc::new(h)),
            Err(e) => {
                log::error!("historial no disponible: {e}");
                None
            }
        };

        let profiles = merge_profiles(detect_profiles(), &config);
        let default_profile = config.default_profile.as_ref().map(|d| {
            profiles
                .iter()
                .find(|p| p.id == *d || p.name == *d)
                .map(|p| p.id.clone())
                .unwrap_or(d.clone())
        });

        let hist = history.clone();
        let on_block = move |r: BlockRecord| {
            if let Some(h) = &hist {
                let origin = if r.origin.is_user() { "user" } else { "agent" };
                let _ = h.add(&NewEntry {
                    profile: &r.profile_id,
                    command: &r.command,
                    cwd: r.cwd.as_deref(),
                    host: r.host.as_deref(),
                    exit_code: r.exit_code,
                    duration_ms: r.duration_ms,
                    origin,
                });
            }
        };

        let manager = SessionManager::new(ManagerOptions {
            assets,
            profiles,
            default_profile,
            on_change: Arc::new(on_change),
            on_block: Some(Arc::new(on_block)),
        });

        let state = Mutex::new(AppState::load(&paths.state_file()));
        Ok(Self {
            manager,
            config,
            config_error,
            paths,
            history,
            state,
        })
    }
}

/// Perfiles detectados + los del usuario (los del usuario van primero y
/// sustituyen a los detectados con el mismo id).
fn merge_profiles(detected: Vec<Profile>, config: &Config) -> Vec<Profile> {
    let mut out: Vec<Profile> = Vec::new();
    for pc in config.profiles.iter().filter(|p| p.applies_here()) {
        let mut p = Profile::new(pc.name.clone(), pc.command.clone(), pc.args.clone());
        p.cwd = pc.cwd.clone();
        p.env = pc.env.clone();
        p.integration = pc.integration && p.kind != ShellKind::Wsl;
        p.id = slug(&pc.name);
        out.push(p);
    }
    for d in detected {
        if !out.iter().any(|p| p.id == d.id) {
            out.push(d);
        }
    }
    out
}
