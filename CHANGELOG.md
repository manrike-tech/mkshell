# Cambios

## 0.1.0 — sin publicar (fase 2, primer incremento)

- Núcleo: PTY multiplataforma, segmentación en bloques con OSC 133 y shells
  anidados, control de flujo, sesiones y layout de paneles.
- Integración para bash, zsh, fish, PowerShell y CMD; bloques en SSH.
- App Tauri 2 + Svelte 5: Command Input, bloques, TTY automático, splits,
  pestañas de sesión, búsqueda, exportación, atajos configurables.
- Historial en SQLite; `config.toml` y `state.toml`.
- `mksh-probe` para validar la integración sin UI (fase 1 en Windows).
