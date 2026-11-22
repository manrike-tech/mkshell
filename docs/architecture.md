# Arquitectura

La propuesta completa está en la [propuesta técnica](https://claude.ai/artifact/1naum7ZSngyCHvjWkJuvBw).
Este documento describe lo implementado en el código.

## Capas

```text
WebView (Svelte 5)  ──invoke(Command)──▶  src-tauri (comandos IPC con ACL)  ──▶  crates mksh-*
        ▲                                            │
        └──── Channel binario por panel ◀────────────┘  (bytes del PTY + eventos JSON)
```

- La UI no accede a disco, red ni procesos. Solo invoca los comandos listados en
  `apps/desktop/src-tauri/build.rs`, autorizados uno a uno en
  `capabilities/default.json`.
- `src-tauri` es una capa fina; toda la lógica está en `crates/`, que no
  dependen de Tauri. El futuro daemon `mkshd` (fase 3) reutilizará el mismo núcleo.

## Flujo de un comando

1. `CommandInput` envía el texto con `run_command`.
2. `PaneRuntime::run_command` lo registra en el `Tracker` y lo escribe en el PTY
   (*bracketed paste* + Enter si el shell lo admite).
3. El shell emite `OSC 133;C` al empezar a ejecutar y `OSC 133;D;<código>` al terminar
   (scripts en `shell-integration/`).
4. El hilo lector pasa los bytes al `Tracker`, que los parte por regiones
   (prompt, eco, salida, arranque) sin modificarlos y genera eventos.
5. Cada fragmento viaja a la UI con una cabecera de 11 bytes
   (`crates/mksh-session/src/frame.rs`); los eventos van como JSON.
6. `PaneController` (UI) escribe la salida del bloque en un xterm.js vivo y, al
   terminar, la congela como HTML estático para no acumular terminales.

## Shells anidados (SSH)

Cada marca lleva `aid=<id>` del shell que la emite. El `Tracker` mantiene una pila:

- Un `A` con `aid` desconocido mientras hay un comando en ejecución apila un shell anidado.
- Un `D` con el `aid` de un shell inferior desapila los de encima (cubre conexiones cortadas).
- El `D` espurio del primer prompt de un shell recién inyectado se ignora.

La inyección (`mksh_shell::ssh::bootstrap_line`) es una línea que decodifica en
base64 el script de bash o zsh y lo evalúa en memoria. Solo se envía tras el
consentimiento explícito en la UI; los hosts recordados se guardan en `state.toml`.

## Control de flujo

El lector suma los bytes enviados y la UI los confirma (`ack_output`) cuando
xterm.js los ha procesado. Por encima de 2 MiB sin confirmar el lector deja de
leer hasta bajar de 512 KiB: el proceso se bloquea en su escritura y la UI no se
congela. Probado con 6 MB de salida (`tests/shells.rs`).

## Ficheros y rutas

| Qué | Linux | Windows |
| --- | --- | --- |
| Configuración | `~/.config/mkshell/config.toml` | `%APPDATA%\MKShell\config.toml` |
| Historial | `~/.local/share/mkshell/history.db` | `%LOCALAPPDATA%\MKShell\history.db` |
| Estado (hosts SSH) | `~/.local/share/mkshell/state.toml` | `%LOCALAPPDATA%\MKShell\state.toml` |
| Scripts de integración | `~/.local/share/mkshell/shell-integration/<versión>/` | `%LOCALAPPDATA%\MKShell\shell-integration\<versión>\` |

`MKSHELL_HOME=<dir>` sustituye todas las rutas (pruebas y desarrollo).

## Decisiones

Ver `docs/adr/`.
