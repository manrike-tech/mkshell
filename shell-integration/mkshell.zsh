# MKShell — integración para Zsh (>= 5.1).
#
# Emite OSC 133 (A/B/C/D con `aid`) y OSC 7. MKShell la carga desde un
# ZDOTDIR temporal que primero lee los ficheros del usuario, o la inyecta en
# memoria en sesiones SSH. Es idempotente.

if [[ -n "${ZSH_VERSION-}" && -z "${__MKSH_LOADED-}" && -o interactive ]]; then
typeset -g __MKSH_LOADED=1
typeset -g __mksh_aid="$$.${RANDOM}"
typeset -g __mksh_running=0
typeset -g __mksh_status=0
autoload -Uz add-zsh-hook

__mksh_urlencode() {
    emulate -L zsh
    local LC_ALL=C s="$1" out="" c
    local -i i
    for (( i = 1; i <= ${#s}; i++ )); do
        c="${s[i]}"
        case "$c" in
            ([a-zA-Z0-9/._~-]) out+="$c" ;;
            (*) out+="$(printf '%%%02X' "'$c")" ;;
        esac
    done
    print -rn -- "$out"
}

# Primer hook de precmd: captura el código antes de que otros hooks lo cambien.
__mksh_precmd_capture() {
    __mksh_status=$?
    if (( __mksh_running )); then
        printf '\e]133;D;%s;aid=%s\a' "$__mksh_status" "$__mksh_aid"
        __mksh_running=0
    fi
    printf '\e]7;file://%s%s\a' "${HOST:-localhost}" "$(__mksh_urlencode "$PWD")"
    return $__mksh_status
}

# Último hook de precmd: envuelve el prompt final (temas que regeneran PS1).
__mksh_precmd_wrap() {
    if [[ "$PS1" != *'133;B'* ]]; then
        PS1=$'%{\e]133;A;aid='"${__mksh_aid}"$'\a%}'"${PS1}"$'%{\e]133;B\a%}'
    fi
    return $__mksh_status
}

__mksh_preexec() {
    __mksh_running=1
    printf '\e]133;C;aid=%s\a' "$__mksh_aid"
}

# La marca de «falta salto de línea» (PROMPT_SP) ensucia la salida de cada
# bloque; se desactiva solo si el usuario no la ha personalizado.
if [[ -z "${PROMPT_EOL_MARK+x}" || "$PROMPT_EOL_MARK" == '%B%S%#%s%b' ]]; then
    PROMPT_EOL_MARK=''
fi

precmd_functions=(__mksh_precmd_capture ${precmd_functions[@]} __mksh_precmd_wrap)
add-zsh-hook preexec __mksh_preexec
fi
