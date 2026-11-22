// Utilidades sobre xterm.js: creación, congelado a HTML y medición.

import { Terminal, type ITheme } from '@xterm/xterm';
import { SearchAddon } from '@xterm/addon-search';
import { SerializeAddon } from '@xterm/addon-serialize';
import { Unicode11Addon } from '@xterm/addon-unicode11';
import { WebglAddon } from '@xterm/addon-webgl';

export interface TermSettings {
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  scrollback: number;
  theme: ITheme;
}

export interface TermHandle {
  term: Terminal;
  serialize: SerializeAddon;
  search: SearchAddon;
  webgl: WebglAddon | null;
  opened: boolean;
}

export function createTerm(s: TermSettings, cols: number, rows: number): TermHandle {
  const term = new Terminal({
    cols,
    rows,
    fontFamily: s.fontFamily,
    fontSize: s.fontSize,
    lineHeight: s.lineHeight,
    scrollback: s.scrollback,
    theme: s.theme,
    allowProposedApi: true,
    convertEol: false,
    cursorBlink: false,
    drawBoldTextInBrightColors: false,
    macOptionIsMeta: true,
    rightClickSelectsWord: false,
  });
  const serialize = new SerializeAddon();
  const search = new SearchAddon();
  term.loadAddon(serialize);
  term.loadAddon(search);
  const unicode = new Unicode11Addon();
  term.loadAddon(unicode);
  term.unicode.activeVersion = '11';
  return { term, serialize, search, webgl: null, opened: false };
}

/** Abre el terminal en un elemento. `webgl` intenta el renderer GPU con caída al DOM. */
export function openTerm(h: TermHandle, el: HTMLElement, webgl: boolean) {
  if (h.opened) {
    // La vista se recreó (p. ej. al dividir el panel): se mueve el terminal.
    const node = h.term.element;
    if (node && node.parentElement !== el) {
      el.appendChild(node);
      h.term.refresh(0, h.term.rows - 1);
    }
    return;
  }
  h.term.open(el);
  h.opened = true;
  if (webgl) {
    try {
      const addon = new WebglAddon();
      addon.onContextLoss(() => {
        addon.dispose();
        h.webgl = null;
      });
      h.term.loadAddon(addon);
      h.webgl = addon;
    } catch (e) {
      // WebKitGTK sin WebGL2: se queda con el renderer DOM.
      console.warn('WebGL no disponible, se usa el renderer DOM', e);
      h.webgl = null;
    }
  }
}

export function disposeTerm(h: TermHandle | null) {
  if (!h) return;
  try {
    h.webgl?.dispose();
    h.term.dispose();
  } catch {
    /* ya liberado */
  }
}

/** Filas con contenido (para ajustar la altura de un bloque vivo). */
export function usedRows(term: Terminal): number {
  const b = term.buffer.active;
  let used = b.baseY + b.cursorY + 1;
  if (b.cursorX === 0) {
    const line = b.getLine(b.baseY + b.cursorY);
    if (!line || line.translateToString(true).length === 0) used -= 1;
  }
  return Math.max(0, used);
}

/** Texto plano del buffer, sin líneas vacías al final. */
export function plainText(term: Terminal, fromLine = 0, toLine?: number): string {
  const b = term.buffer.active;
  const end = Math.min(toLine ?? b.length, b.length);
  const lines: string[] = [];
  for (let i = fromLine; i < end; i++) {
    const line = b.getLine(i);
    if (!line) continue;
    const text = line.translateToString(true);
    if (line.isWrapped && lines.length > 0) lines[lines.length - 1] += text;
    else lines.push(text);
  }
  while (lines.length && lines[lines.length - 1].trim() === '') lines.pop();
  return lines.join('\n');
}

/**
 * Convierte el contenido del terminal en HTML estático (colores incluidos) y
 * elimina las filas vacías del final. `toLine` (exclusivo) limita el rango.
 */
export function freezeHtml(h: TermHandle, toLine?: number): string {
  const b = h.term.buffer.active;
  const last = Math.min(toLine ?? b.length, b.length) - 1;
  if (last < 0) return '';
  const raw = h.serialize.serializeAsHTML({
    includeGlobalBackground: false,
    range: { startLine: 0, endLine: last, startCol: 0 },
  });
  const doc = new DOMParser().parseFromString(raw, 'text/html');
  const container = doc.querySelector('pre > div');
  if (!container) return '';
  container.removeAttribute('style');
  const rows = Array.from(container.children);
  for (let i = rows.length - 1; i >= 0; i--) {
    if ((rows[i].textContent ?? '').trim() !== '') break;
    rows[i].remove();
  }
  return container.innerHTML;
}

/** Tamaño de celda aproximado para calcular columnas/filas antes de crear terminales. */
export function measureCell(fontFamily: string, fontSize: number, lineHeight: number): { w: number; h: number } {
  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d');
  let w = fontSize * 0.6;
  if (ctx) {
    ctx.font = `${fontSize}px ${fontFamily}`;
    w = ctx.measureText('W'.repeat(50)).width / 50;
  }
  // xterm.js redondea la altura de fila con el devicePixelRatio.
  const dpr = window.devicePixelRatio || 1;
  const h = Math.ceil(fontSize * 1.17 * lineHeight * dpr) / dpr;
  return { w, h };
}
