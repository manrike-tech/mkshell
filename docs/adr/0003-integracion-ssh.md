# ADR 0003 — Integración en sesiones SSH

- Estado: aceptada (2026-11-22)

## Decisión

Para tener bloques en un host remoto, MKShell envía al shell remoto una única
línea que decodifica (base64) y evalúa en memoria el script de bash o zsh.

- Solo con consentimiento explícito; «Recordar host» lo guarda en `state.toml`
  y activa la inyección automática cuando el prompt remoto queda en reposo.
- No instala nada ni escribe en disco; requiere `base64` en el host.
- La línea empieza por espacio para no entrar en el historial cuando
  `HISTCONTROL`/`HIST_IGNORE_SPACE` lo permiten.
- El eco de la línea inyectada se excluye del bloque `ssh` al congelarlo.

## Pendiente

Fish y PowerShell remotos (fase 3). Dentro de `tmux`/`screen` remotos las marcas
pueden no llegar: se usa TTY clásico.
