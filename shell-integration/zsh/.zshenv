# MKShell: ZDOTDIR temporal. Carga el .zshenv del usuario y restaura su ZDOTDIR.
__mksh_zdotdir="$ZDOTDIR"
ZDOTDIR="${MKSH_USER_ZDOTDIR:-$HOME}"
if [[ -f "$ZDOTDIR/.zshenv" ]]; then
    . "$ZDOTDIR/.zshenv"
fi
MKSH_USER_ZDOTDIR="$ZDOTDIR"
ZDOTDIR="$__mksh_zdotdir"
unset __mksh_zdotdir
