# ADR 0002 — Bloques mediante marcas OSC 133 con `aid`

- Estado: aceptada (2026-11-22)

## Decisión

Los bloques se delimitan con OSC 133 (A/B/C/D), emitidas por scripts que
MKShell inyecta al lanzar el shell (`--rcfile`, `ZDOTDIR` temporal,
`--init-command`, `-Command`, variable `PROMPT`), sin tocar los dotfiles.
Cada marca lleva `aid=<pid>.<aleatorio>` para soportar shells anidados.

## Detalles

- Bash: el inicio de salida se marca con `PS0`; «hubo comando» se detecta con el
  contador de prompt `\#` (sin trampas DEBUG, que chocan con bash-preexec).
- Zsh: `precmd`/`preexec`; se desactiva `PROMPT_EOL_MARK` solo si no está personalizado.
- PowerShell: marca C envolviendo `PSConsoleHostReadLine`; sin PSReadLine, `mkc=0`
  y el núcleo infiere el inicio de la salida.
- CMD: sin código de salida ni marca C (limitación de CMD).
- Un PTY mezcla stdout y stderr: no se separan en esta fase.
