# MKShell

Terminal multiplataforma (Linux y Windows) con sesiones y paneles al estilo tmux,
**comandos arriba y resultados abajo** organizados en bloques, e integración
futura con agentes de IA e IDE Mode.

> Estado: **fase 2 (MVP de terminal) en curso**, versión 0.1.0. Ver
> [estado del MVP](#estado-del-mvp) y la [propuesta técnica](https://claude.ai/artifact/1naum7ZSngyCHvjWkJuvBw).

## Qué hace hoy

- Sesiones con nombre (crear, cerrar, renombrar, cambiar) y paneles divididos en
  horizontal/vertical, redimensionables con el ratón o el teclado.
- Cada panel ejecuta un shell real en un PTY (openpty en Linux, ConPTY en Windows).
- **Command Input** arriba (CodeMirror): resaltado, historial persistente (↑/↓),
  autocompletado de historial y rutas (Tab), multilínea (Shift+Enter).
- **Bloques** debajo: cada comando con su salida, código de salida, duración y cwd;
  copiar, exportar, volver a ejecutar, contraer y buscar.
- Cambio automático a terminal completa con `vim`, `htop`, `less`… y modo TTY clásico.
- Entrada interactiva durante la ejecución (`read`, `sudo`, confirmaciones), Ctrl+C.
- **Bloques en SSH**: con consentimiento por host, inyecta la integración en el
  shell remoto en memoria (sin instalar nada); los comandos remotos se muestran
  como bloques propios con el host.
- Atajos configurables en `config.toml` (F1 los lista).

## Arquitectura en una línea

Tauri 2 + núcleo Rust headless (`crates/mksh-*`, sin dependencia de Tauri) +
UI Svelte 5 con xterm.js 6 y CodeMirror 6. Detalle en [docs/architecture.md](docs/architecture.md).

```text
apps/desktop/            App Tauri (src-tauri) + UI Svelte (src)
crates/mksh-core         Tipos de dominio, layout, bus de comandos, eventos
crates/mksh-pty          PTY multiplataforma + cierre del árbol de procesos
crates/mksh-shell        Detección de shells, perfiles, integración, SSH
crates/mksh-session      Segmentación en bloques (OSC 133), paneles, sesiones
crates/mksh-config       config.toml, atajos, estado persistente
crates/mksh-store        Historial SQLite
tools/mksh-probe         Prueba de concepto sin UI (valida ConPTY / OSC 133)
shell-integration/       Scripts bash, zsh, fish, PowerShell
```

## Desarrollo rápido

Requisitos: Rust estable, Node 22 + pnpm 10 y las dependencias de Tauri 2 de tu
sistema ([guía](docs/development.md)).

```bash
pnpm install
pnpm dev                       # app en modo desarrollo
cargo test --workspace         # pruebas Rust (incluye shells reales si están instalados)
pnpm check && pnpm test        # tipos y pruebas del frontend
cargo run -p mksh-probe -- --shell bash   # PoC de integración sin UI
pnpm build                     # instaladores (deb, rpm, AppImage, MSI, NSIS)
```

## Estado del MVP

| Elemento | Estado |
| --- | --- |
| PTY, sesiones, splits, Command Input, bloques, TTY automático | Hecho y probado en Linux (tests + app en Xvfb) |
| Integración bash, zsh, fish | Hecho y probado con shells reales |
| PowerShell 7 y 5.1 en Windows | Probado en CI (`windows-2025`, `mksh-probe`): bloques, códigos de salida y cwd |
| CMD en Windows | Probado en CI: bloques y salida; sin código de salida (limitación de CMD) |
| ConPTY y Job Objects | Probado en CI: E/S por ConPTY, respuesta a la consulta de cursor inicial, cierre de nietos |
| App de escritorio en Windows | Compila y pasa pruebas en CI; **interfaz sin probar visualmente** en WebView2 |
| Bloques en SSH (bash/zsh remotos) | Hecho y probado contra un `sshd` local |
| Historial SQLite, atajos configurables | Hecho |
| Persistencia de sesiones, daemon `mkshd`, temas en UI | Fase 3 |

## Licencia

[MIT](LICENSE).
