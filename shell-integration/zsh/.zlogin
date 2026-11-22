# MKShell: carga el .zlogin del usuario y restaura su ZDOTDIR definitivamente.
ZDOTDIR="$MKSH_USER_ZDOTDIR"
if [[ -f "$ZDOTDIR/.zlogin" ]]; then
    . "$ZDOTDIR/.zlogin"
fi
unset MKSH_USER_ZDOTDIR
