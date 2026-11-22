# MKShell — integración para Fish (>= 3.1).
#
# Emite OSC 133 (A/B/C/D con `aid`) y OSC 7. MKShell la carga con
# `--init-command` después de la configuración del usuario. Es idempotente.

if status is-interactive; and not set -q __MKSH_LOADED
    set -g __MKSH_LOADED 1
    set -g __mksh_aid "$fish_pid."(random)
    set -g __mksh_running 0

    function __mksh_preexec --on-event fish_preexec
        set -g __mksh_running 1
        printf '\e]133;C;aid=%s\a' $__mksh_aid
    end

    function __mksh_postexec --on-event fish_postexec
        set -l st $status
        if test "$__mksh_running" = 1
            printf '\e]133;D;%s;aid=%s\a' $st $__mksh_aid
            set -g __mksh_running 0
        end
    end

    function __mksh_urlencode
        string escape --style=url -- $argv[1] | string replace -a '%2F' '/'
    end

    if functions -q fish_prompt
        functions -c fish_prompt __mksh_user_prompt
    else
        function __mksh_user_prompt
            printf '%s> ' (prompt_pwd)
        end
    end

    function fish_prompt
        printf '\e]7;file://%s%s\a' (hostname) (__mksh_urlencode $PWD)
        printf '\e]133;A;aid=%s\a' $__mksh_aid
        __mksh_user_prompt
        printf '\e]133;B\a'
    end
end
