# MKShell — integración para Bash (>= 4.4).
#
# Emite marcas OSC 133 (A: inicio de prompt, B: fin de prompt, C: inicio de
# salida, D: fin de comando con código) y OSC 7 (directorio actual). Cada marca
# lleva `aid=<id>` para distinguir shells anidados (ssh, subshells).
#
# No modifica ficheros del usuario: MKShell lo carga con `--rcfile` tras
# `~/.bashrc`, o lo inyecta en memoria en sesiones SSH. Es idempotente.

if [ -n "${BASH_VERSION-}" ] && [ -z "${__MKSH_LOADED-}" ] && [[ $- == *i* ]]; then
__MKSH_LOADED=1
__mksh_aid="$$.${RANDOM}"
__mksh_status=0
# Número de comando del prompt (\#): solo crece cuando se ejecuta un comando,
# así se distingue «comando ejecutado» de «Enter en vacío» sin trampas DEBUG.
__mksh_cnum_fmt='\#'
__mksh_last_cnum="${__mksh_cnum_fmt@P}"

__mksh_urlencode() {
    local LC_ALL=C s="$1" out="" c i
    for (( i = 0; i < ${#s}; i++ )); do
        c="${s:i:1}"
        case "$c" in
            [a-zA-Z0-9/._~-]) out+="$c" ;;
            *) printf -v c '%%%02X' "'$c"; out+="$c" ;;
        esac
    done
    printf '%s' "$out"
}

__mksh_prompt_start() {
    __mksh_status=$?
    local cnum="${__mksh_cnum_fmt@P}"
    if [ "$cnum" != "$__mksh_last_cnum" ]; then
        __mksh_last_cnum="$cnum"
        printf '\033]133;D;%s;aid=%s\007' "$__mksh_status" "$__mksh_aid"
    fi
    printf '\033]7;file://%s%s\007' "${HOSTNAME:-localhost}" "$(__mksh_urlencode "$PWD")"
    return "$__mksh_status"
}

__mksh_prompt_end() {
    # Se vuelve a envolver PS1 si otro hook lo regeneró (starship, oh-my-bash…).
    if [[ "$PS1" != *'133;B'* ]]; then
        PS1="\[\033]133;A;aid=${__mksh_aid}\007\]${PS1}\[\033]133;B\007\]"
    fi
    return "$__mksh_status"
}

# PS0 se expande tras leer un comando y antes de ejecutarlo: marca C.
PS0="\033]133;C;aid=${__mksh_aid}\007${PS0-}"

if [[ "$(declare -p PROMPT_COMMAND 2>/dev/null)" == "declare -a"* ]]; then
    PROMPT_COMMAND=(__mksh_prompt_start "${PROMPT_COMMAND[@]}" __mksh_prompt_end)
else
    PROMPT_COMMAND="__mksh_prompt_start${PROMPT_COMMAND:+; ${PROMPT_COMMAND}}; __mksh_prompt_end"
fi
fi
