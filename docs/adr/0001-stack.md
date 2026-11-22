# ADR 0001 — Stack de la aplicación

- Estado: aceptada (2026-11-22)
- Contexto: propuesta técnica v0.1, validada para empezar la fase 2.

## Decisión

Tauri 2 (núcleo Rust, WebView del sistema) + Svelte 5 + TypeScript, xterm.js 6
para emulación, CodeMirror 6 para la entrada de comandos, portable-pty 0.9 para
PTY/ConPTY, SQLite (rusqlite) para historial y TOML para configuración.

## Consecuencias

- La lógica vive en crates Rust sin dependencia de Tauri: si el rendimiento de
  WebKitGTK no fuera aceptable, el núcleo se puede servir desde un daemon y
  cambiar de contenedor sin reescribirlo.
- `app.security.freezePrototype` **no** está activado: con las dependencias
  actuales del frontend la página no llega a ejecutarse (comprobado en
  WebKitGTK 2.52). Reevaluar al actualizar dependencias.
- `tauri-specta` sigue en RC; de momento los tipos TS se mantienen a mano en
  `apps/desktop/src/lib/ipc/types.ts`.
