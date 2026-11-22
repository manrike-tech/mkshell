# MKShell — integración para PowerShell 7 y Windows PowerShell 5.1.
#
# Emite OSC 133 (A/B/C/D con `aid`) y OSC 9;9 (directorio actual). La marca C
# se emite envolviendo PSConsoleHostReadLine (PSReadLine). Sin PSReadLine, la
# marca A incluye `mkc=0` y MKShell infiere el inicio de la salida.
#
# MKShell la carga con `-NoExit -Command ". <ruta>"` tras el perfil del usuario.
# Compatible con PowerShell 5.1: no usa el escape `e.
#
# Estado: no verificada todavía en Windows (pendiente de la CI de Windows).

if (-not $global:__MKSH_LOADED) {
    $global:__MKSH_LOADED = $true
    $global:__mksh_aid = "$PID." + (Get-Random -Maximum 32767)
    $global:__mksh_esc = [char]27
    $global:__mksh_bel = [char]7
    $global:__mksh_running = $false
    $global:__mksh_has_readline = $false
    $global:__mksh_user_prompt = $function:prompt

    if (Get-Command PSConsoleHostReadLine -ErrorAction SilentlyContinue) {
        $global:__mksh_has_readline = $true
        $global:__mksh_user_readline = $function:PSConsoleHostReadLine
        function global:PSConsoleHostReadLine {
            $line = & $global:__mksh_user_readline
            if ($null -ne $line -and $line.Trim().Length -gt 0) {
                $global:__mksh_running = $true
                [Console]::Write("$($global:__mksh_esc)]133;C;aid=$($global:__mksh_aid)$($global:__mksh_bel)")
            }
            return $line
        }
    }

    function global:prompt {
        # Debe ser la primera instrucción: estado del último comando.
        $ok = $global:?
        $e = $global:__mksh_esc
        $b = $global:__mksh_bel
        $aid = $global:__mksh_aid
        $out = ""
        # Sin PSReadLine no se sabe si hubo comando: se emite D siempre y
        # MKShell lo ignora si no había un bloque abierto.
        if ($global:__mksh_running -or -not $global:__mksh_has_readline) {
            $code = 0
            if (-not $ok) {
                $code = 1
                if ($null -ne $global:LASTEXITCODE -and $global:LASTEXITCODE -ne 0) {
                    $code = $global:LASTEXITCODE
                }
            }
            $out += "$e]133;D;$code;aid=$aid$b"
            $global:__mksh_running = $false
        }
        $mkc = ""
        if (-not $global:__mksh_has_readline) { $mkc = ";mkc=0" }
        $out += "$e]133;A;aid=$aid$mkc$b"
        $loc = $executionContext.SessionState.Path.CurrentLocation
        if ($loc.Provider.Name -eq "FileSystem") {
            $out += "$e]9;9;`"$($loc.ProviderPath)`"$b"
        }
        $out += & $global:__mksh_user_prompt
        $out += "$e]133;B$b"
        return $out
    }
}
