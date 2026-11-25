//! Genera los permisos `allow-<comando>` de cada comando IPC de la app. Solo
//! los que aparezcan en `capabilities/` podrán invocarse desde la WebView.

const COMMANDS: &[&str] = &[
    "get_bootstrap",
    "create_session",
    "close_session",
    "rename_session",
    "split_pane",
    "close_pane",
    "set_split_ratio",
    "attach_pane",
    "run_command",
    "write_input",
    "resize_pane",
    "ack_output",
    "history",
    "complete_path",
    "enable_blocks",
    "save_text_file",
];

fn main() {
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    tauri_build::try_build(attributes).expect("tauri-build");
}
