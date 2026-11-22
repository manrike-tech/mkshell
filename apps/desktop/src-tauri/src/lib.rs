//! Aplicación de escritorio MKShell (Tauri 2).
//!
//! Capa fina: arranca el núcleo (`mksh-*`), lo expone como comandos IPC con
//! ACL (`capabilities/default.json`) y reenvía la salida de cada panel por un
//! `Channel` binario. Ninguna lógica de terminal vive aquí.

mod ipc;
mod state;

use state::AppContext;
use tauri::{Emitter, Manager, RunEvent};

/// Evento global con la lista de sesiones tras cada cambio estructural.
pub const SESSIONS_EVENT: &str = "mksh://sessions";

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let ctx = AppContext::init(move |sessions| {
                let _ = handle.emit(SESSIONS_EVENT, sessions);
            })?;
            app.manage(ctx);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc::get_bootstrap,
            ipc::create_session,
            ipc::close_session,
            ipc::rename_session,
            ipc::split_pane,
            ipc::close_pane,
            ipc::set_split_ratio,
            ipc::attach_pane,
            ipc::run_command,
            ipc::write_input,
            ipc::resize_pane,
            ipc::ack_output,
            ipc::history,
            ipc::complete_path,
            ipc::enable_blocks,
            ipc::save_text_file,
        ])
        .build(tauri::generate_context!())
        .expect("error al construir la aplicación");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            if let Some(ctx) = handle.try_state::<AppContext>() {
                ctx.manager.shutdown();
            }
        }
    });
}
