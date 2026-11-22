# Guía de desarrollo

## Requisitos

| | Linux (Ubuntu/Debian) | Windows 10 1809+ / 11 |
| --- | --- | --- |
| Rust | `rustup` estable | `rustup` estable (MSVC) + Visual Studio Build Tools |
| Node | 22 LTS + `corepack enable` (pnpm 10) | igual |
| Sistema | `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev libayatana-appindicator3-dev build-essential` | WebView2 (incluido en Windows 11) |
| Para pruebas | `zsh fish` (opcionales: se omiten si faltan) | PowerShell 7 recomendado |

## Comandos

```bash
pnpm install                         # dependencias del frontend
pnpm dev                             # tauri dev (Vite en :1420)
cargo test --workspace               # pruebas Rust
cargo clippy --workspace --all-targets -- -D warnings
pnpm check                           # svelte-check
pnpm test                            # vitest
pnpm build                           # tauri build (instaladores)
```

Para aislar la configuración durante el desarrollo:

```bash
MKSHELL_HOME=/tmp/mkshell-dev pnpm dev
```

## Validar ConPTY y la integración (PoC de la fase 1)

`mksh-probe` lanza un shell en el PTY real, ejecuta tres comandos y comprueba
bloques, códigos de salida y cwd, sin interfaz:

```powershell
cargo run -p mksh-probe -- --list
cargo run -p mksh-probe -- --shell pwsh                 # PowerShell 7
cargo run -p mksh-probe -- --shell windows-powershell   # PowerShell 5.1
cargo run -p mksh-probe -- --shell cmd
cargo run -p mksh-probe -- --shell pwsh --dump   # muestra cada fragmento y evento
```

Si el prompt no se detecta en Windows, ConPTY puede estar filtrando las
secuencias OSC: copia `conpty.dll` y `OpenConsole.exe` de una versión reciente de
Windows Terminal junto al ejecutable (portable-pty los carga primero) y repite.

## Linux: problemas gráficos de WebKitGTK

En algunas GPU (sobre todo NVIDIA) WebKitGTK falla o va lento. Pruebas, de menos
a más invasiva (documentación de Tauri):

```bash
__NV_DISABLE_EXPLICIT_SYNC=1 pnpm dev
WEBKIT_DISABLE_DMABUF_RENDERER=1 pnpm dev
```

Los terminales a pantalla completa intentan WebGL y caen al renderer DOM si falla.

## Probar la app sin escritorio (CI, contenedores)

```bash
Xvfb :99 -screen 0 1400x900x24 &
DISPLAY=:99 MKSHELL_HOME=/tmp/mk WEBKIT_DISABLE_DMABUF_RENDERER=1 \
  dbus-run-session -- target/debug/mkshell
```

Para que una build de depuración sirva `dist/` en lugar de Vite:
`cargo build -p mkshell-desktop --features custom-protocol` (tras `pnpm --filter mkshell-desktop build`).

## Añadir un comando IPC

1. Función `#[tauri::command]` en `apps/desktop/src-tauri/src/ipc/mod.rs`.
2. Registrarla en `generate_handler!` (`lib.rs`) y en `COMMANDS` (`build.rs`).
3. Autorizar `allow-<nombre-con-guiones>` en `capabilities/default.json`.
4. Envoltorio tipado en `apps/desktop/src/lib/ipc/api.ts`.

## Añadir un atajo o acción

1. Identificador en `mksh_core::command::COMMANDS`.
2. Atajo por defecto (opcional) en `mksh_config::default_keybindings`.
3. Implementación en `AppStore.run` (`apps/desktop/src/lib/state/app.svelte.ts`).
