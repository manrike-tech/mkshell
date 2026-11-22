//! Gestor de sesiones y paneles.
//!
//! Mantiene las sesiones con su árbol de layout y los paneles vivos. Notifica
//! cada cambio estructural (sesión creada, panel cerrado porque su shell
//! terminó…) a un oyente, que en la app de escritorio es la UI.

use crate::runtime::{BlockHook, PaneInfo, PaneOptions, PaneRuntime};
use anyhow::{Result, anyhow};
use mksh_core::{CoreError, LayoutNode, LayoutTree, PaneId, SessionId, SplitDirection, SplitId};
use mksh_shell::{IntegrationAssets, Profile};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Weak};

/// Estado público de una sesión.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: SessionId,
    pub name: String,
    pub layout: LayoutNode,
    pub panes: Vec<PaneInfo>,
}

pub type ChangeListener = Arc<dyn Fn(Vec<SessionInfo>) + Send + Sync>;

struct SessionEntry {
    id: SessionId,
    name: String,
    layout: LayoutTree,
}

#[derive(Default)]
struct State {
    sessions: Vec<SessionEntry>,
    panes: HashMap<PaneId, Arc<PaneRuntime>>,
    counter: usize,
}

struct Inner {
    state: Mutex<State>,
    assets: Option<IntegrationAssets>,
    profiles: RwLock<Vec<Profile>>,
    default_profile: RwLock<Option<String>>,
    on_change: ChangeListener,
    on_block: Option<BlockHook>,
}

/// Configuración del gestor.
pub struct ManagerOptions {
    pub assets: Option<IntegrationAssets>,
    pub profiles: Vec<Profile>,
    pub default_profile: Option<String>,
    pub on_change: ChangeListener,
    pub on_block: Option<BlockHook>,
}

#[derive(Clone)]
pub struct SessionManager {
    inner: Arc<Inner>,
}

impl SessionManager {
    pub fn new(opts: ManagerOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(State::default()),
                assets: opts.assets,
                profiles: RwLock::new(opts.profiles),
                default_profile: RwLock::new(opts.default_profile),
                on_change: opts.on_change,
                on_block: opts.on_block,
            }),
        }
    }

    pub fn profiles(&self) -> Vec<Profile> {
        self.inner.profiles.read().clone()
    }

    fn profile(&self, id: Option<&str>) -> Result<Profile> {
        let profiles = self.inner.profiles.read();
        let wanted = id
            .map(String::from)
            .or_else(|| self.inner.default_profile.read().clone());
        if let Some(w) = wanted {
            if let Some(p) = profiles.iter().find(|p| p.id == w) {
                return Ok(p.clone());
            }
            if id.is_some() {
                return Err(anyhow!("perfil desconocido: {w}"));
            }
        }
        profiles
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("no hay shells disponibles"))
    }

    pub fn sessions(&self) -> Vec<SessionInfo> {
        let st = self.inner.state.lock();
        snapshot(&st)
    }

    pub fn pane(&self, id: PaneId) -> Result<Arc<PaneRuntime>> {
        self.inner
            .state
            .lock()
            .panes
            .get(&id)
            .cloned()
            .ok_or_else(|| CoreError::PaneNotFound(id).into())
    }

    fn spawn_pane(&self, profile: Profile, cwd: Option<PathBuf>) -> Result<Arc<PaneRuntime>> {
        let id = PaneId::new();
        let weak: Weak<Inner> = Arc::downgrade(&self.inner);
        let opts = PaneOptions {
            cwd,
            on_exit: Some(Box::new(move |pane, _code| {
                if let Some(inner) = weak.upgrade() {
                    SessionManager { inner }.remove_pane(pane);
                }
            })),
            on_block: self.inner.on_block.clone(),
            ..Default::default()
        };
        PaneRuntime::spawn(id, profile, self.inner.assets.as_ref(), opts)
    }

    /// Crea una sesión con un panel.
    pub fn create_session(
        &self,
        profile: Option<&str>,
        name: Option<String>,
        cwd: Option<PathBuf>,
    ) -> Result<SessionInfo> {
        let profile = self.profile(profile)?;
        let pane = self.spawn_pane(profile, cwd)?;
        let info = {
            let mut st = self.inner.state.lock();
            st.counter += 1;
            let id = SessionId::new();
            let name = name
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| format!("Sesión {}", st.counter));
            st.sessions.push(SessionEntry {
                id,
                name,
                layout: LayoutTree::new(pane.id),
            });
            st.panes.insert(pane.id, pane);
            session_info(&st, id).expect("recién creada")
        };
        self.notify();
        Ok(info)
    }

    pub fn rename_session(&self, id: SessionId, name: String) -> Result<()> {
        {
            let mut st = self.inner.state.lock();
            let s = st
                .sessions
                .iter_mut()
                .find(|s| s.id == id)
                .ok_or(CoreError::SessionNotFound(id))?;
            let name = name.trim();
            if !name.is_empty() {
                s.name = name.chars().take(64).collect();
            }
        }
        self.notify();
        Ok(())
    }

    pub fn close_session(&self, id: SessionId) -> Result<()> {
        let panes: Vec<Arc<PaneRuntime>> = {
            let mut st = self.inner.state.lock();
            let idx = st
                .sessions
                .iter()
                .position(|s| s.id == id)
                .ok_or(CoreError::SessionNotFound(id))?;
            let entry = st.sessions.remove(idx);
            entry
                .layout
                .panes()
                .iter()
                .filter_map(|p| st.panes.remove(p))
                .collect()
        };
        for p in panes {
            p.kill();
        }
        self.notify();
        Ok(())
    }

    /// Divide un panel. El nuevo hereda el directorio del original.
    pub fn split_pane(
        &self,
        pane: PaneId,
        direction: SplitDirection,
        profile: Option<&str>,
    ) -> Result<SessionInfo> {
        let source = self.pane(pane)?;
        let profile = match profile {
            Some(p) => self.profile(Some(p))?,
            None => source.profile.clone(),
        };
        let new_pane = self.spawn_pane(profile, source.local_cwd())?;
        let info = {
            let mut st = self.inner.state.lock();
            let Some(session) = st.sessions.iter_mut().find(|s| s.layout.contains(pane)) else {
                drop(st);
                new_pane.kill();
                return Err(CoreError::PaneNotFound(pane).into());
            };
            session.layout.split(pane, direction, new_pane.id)?;
            let sid = session.id;
            st.panes.insert(new_pane.id, new_pane);
            session_info(&st, sid).expect("existe")
        };
        self.notify();
        Ok(info)
    }

    /// Cierra un panel (y su sesión si era el último).
    pub fn close_pane(&self, pane: PaneId) -> Result<()> {
        let rt = self
            .detach_pane(pane)
            .ok_or(CoreError::PaneNotFound(pane))?;
        rt.kill();
        self.notify();
        Ok(())
    }

    pub fn set_ratio(&self, session: SessionId, split: SplitId, ratio: f32) -> Result<()> {
        let mut st = self.inner.state.lock();
        let s = st
            .sessions
            .iter_mut()
            .find(|s| s.id == session)
            .ok_or(CoreError::SessionNotFound(session))?;
        s.layout.set_ratio(split, ratio)?;
        Ok(())
    }

    /// Quita el panel del layout y del mapa, sin matarlo.
    fn detach_pane(&self, pane: PaneId) -> Option<Arc<PaneRuntime>> {
        let mut st = self.inner.state.lock();
        let rt = st.panes.remove(&pane)?;
        if let Some(idx) = st.sessions.iter().position(|s| s.layout.contains(pane)) {
            match st.sessions[idx].layout.remove(pane) {
                Ok(()) => {}
                Err(_) => {
                    st.sessions.remove(idx);
                }
            }
        }
        Some(rt)
    }

    /// Llamado cuando el shell de un panel termina por sí mismo.
    fn remove_pane(&self, pane: PaneId) {
        if self.detach_pane(pane).is_some() {
            self.notify();
        }
    }

    /// Termina todos los paneles (cierre de la aplicación).
    pub fn shutdown(&self) {
        let panes: Vec<Arc<PaneRuntime>> = {
            let mut st = self.inner.state.lock();
            st.sessions.clear();
            st.panes.drain().map(|(_, p)| p).collect()
        };
        for p in panes {
            p.kill();
        }
    }

    fn notify(&self) {
        let snapshot = self.sessions();
        (self.inner.on_change)(snapshot);
    }
}

fn session_info(st: &State, id: SessionId) -> Option<SessionInfo> {
    let s = st.sessions.iter().find(|s| s.id == id)?;
    Some(SessionInfo {
        id: s.id,
        name: s.name.clone(),
        layout: s.layout.root.clone(),
        panes: s
            .layout
            .panes()
            .iter()
            .filter_map(|p| st.panes.get(p))
            .map(|p| p.info())
            .collect(),
    })
}

fn snapshot(st: &State) -> Vec<SessionInfo> {
    st.sessions
        .iter()
        .filter_map(|s| session_info(st, s.id))
        .collect()
}
