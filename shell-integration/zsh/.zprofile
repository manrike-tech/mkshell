# MKShell: carga el .zprofile del usuario (shells de login).
__mksh_zdotdir="$ZDOTDIR"
ZDOTDIR="$MKSH_USER_ZDOTDIR"
if [[ -f "$ZDOTDIR/.zprofile" ]]; then
    . "$ZDOTDIR/.zprofile"
fi
MKSH_USER_ZDOTDIR="$ZDOTDIR"
ZDOTDIR="$__mksh_zdotdir"
unset __mksh_zdotdir
