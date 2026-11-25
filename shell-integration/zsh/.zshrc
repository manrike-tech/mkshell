# MKShell: carga el .zshrc del usuario y después la integración.
__mksh_zdotdir="$ZDOTDIR"
ZDOTDIR="$MKSH_USER_ZDOTDIR"
if [[ -f "$ZDOTDIR/.zshrc" ]]; then
    . "$ZDOTDIR/.zshrc"
fi
MKSH_USER_ZDOTDIR="$ZDOTDIR"
. "$__mksh_zdotdir/../mkshell.zsh"
# Si no es shell de login, .zlogin no se lee: se restaura ZDOTDIR aquí para
# que los zsh anidados usen la configuración del usuario.
if [[ ! -o login ]]; then
    ZDOTDIR="$MKSH_USER_ZDOTDIR"
    unset MKSH_USER_ZDOTDIR
else
    ZDOTDIR="$__mksh_zdotdir"
fi
unset __mksh_zdotdir
