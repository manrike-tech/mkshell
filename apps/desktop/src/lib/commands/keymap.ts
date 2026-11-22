// Atajos de teclado: normalización igual a mksh-config::normalize_chord.

export interface KeyLike {
  key: string;
  code?: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

/** Tecla base: con Shift pulsado `key` cambia (D→"D", 1→"!"), así que se usa `code` para letras y dígitos. */
function baseKey(e: KeyLike): string {
  const code = e.code ?? '';
  if (/^Key[A-Z]$/.test(code)) return code.slice(3).toLowerCase();
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (code === 'Equal') return '=';
  if (code === 'Minus') return '-';
  if (code === 'Comma') return ',';
  if (code === 'Period') return '.';
  if (code === 'Slash') return '/';
  if (code === 'BracketLeft') return '[';
  if (code === 'BracketRight') return ']';
  const k = e.key.toLowerCase();
  return k === ' ' ? 'space' : k;
}

export function chordOf(e: KeyLike): string {
  const parts: string[] = [];
  if (e.ctrlKey) parts.push('ctrl');
  if (e.altKey) parts.push('alt');
  if (e.shiftKey) parts.push('shift');
  if (e.metaKey) parts.push('meta');
  const key = baseKey(e);
  if (['control', 'alt', 'shift', 'meta'].includes(key)) return '';
  parts.push(key);
  return parts.join('+');
}

/** Atajo → comando. Devuelve el comando si `e` coincide con algún atajo. */
export function matchCommand(bindings: Record<string, string>, e: KeyLike): string | null {
  const chord = chordOf(e);
  return chord ? (bindings[chord] ?? null) : null;
}

/** Texto legible para mostrar un atajo («Ctrl+Shift+D»). */
export function prettyChord(chord: string): string {
  return chord
    .split('+')
    .map((p) =>
      p === 'ctrl'
        ? 'Ctrl'
        : p === 'shift'
          ? 'Shift'
          : p === 'alt'
            ? 'Alt'
            : p === 'meta'
              ? 'Meta'
              : p.startsWith('arrow')
                ? { arrowleft: '←', arrowright: '→', arrowup: '↑', arrowdown: '↓' }[p] ?? p
                : p.length === 1
                  ? p.toUpperCase()
                  : p[0].toUpperCase() + p.slice(1),
    )
    .join('+');
}

/** Primer atajo asignado a un comando (para tooltips). */
export function chordFor(bindings: Record<string, string>, command: string): string | null {
  const entry = Object.entries(bindings).find(([, c]) => c === command);
  return entry ? prettyChord(entry[0]) : null;
}
