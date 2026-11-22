// Temas: variables CSS de la interfaz + paleta de xterm.js.

import type { ITheme } from '@xterm/xterm';

export interface MkTheme {
  name: string;
  css: Record<string, string>;
  terminal: ITheme;
}

const dark: MkTheme = {
  name: 'mk-dark',
  css: {
    '--bg': '#0f1419',
    '--bg-elev': '#151b22',
    '--bg-block': '#121820',
    '--bg-input': '#0b1015',
    '--border': '#243040',
    '--border-strong': '#33455a',
    '--fg': '#d7dde5',
    '--fg-muted': '#8a97a8',
    '--accent': '#4fc1ff',
    '--accent-soft': 'rgba(79, 193, 255, 0.14)',
    '--ok': '#7fd88f',
    '--err': '#ff7b72',
    '--warn': '#e3b341',
    '--ai': '#c792ea',
    '--selection': 'rgba(79, 193, 255, 0.30)',
  },
  terminal: {
    background: '#121820',
    foreground: '#d7dde5',
    cursor: '#4fc1ff',
    cursorAccent: '#121820',
    selectionBackground: 'rgba(79, 193, 255, 0.30)',
    black: '#1c2430',
    red: '#ff7b72',
    green: '#7fd88f',
    yellow: '#e3b341',
    blue: '#59a6ff',
    magenta: '#c792ea',
    cyan: '#4fc1ff',
    white: '#d7dde5',
    brightBlack: '#5c6b7e',
    brightRed: '#ffa198',
    brightGreen: '#a5e8b0',
    brightYellow: '#f2cc60',
    brightBlue: '#8cc4ff',
    brightMagenta: '#ddb6f2',
    brightCyan: '#8ad9ff',
    brightWhite: '#ffffff',
  },
};

const light: MkTheme = {
  name: 'mk-light',
  css: {
    '--bg': '#f6f8fa',
    '--bg-elev': '#ffffff',
    '--bg-block': '#ffffff',
    '--bg-input': '#ffffff',
    '--border': '#d6dde5',
    '--border-strong': '#b8c3cf',
    '--fg': '#1f2328',
    '--fg-muted': '#59636e',
    '--accent': '#0969da',
    '--accent-soft': 'rgba(9, 105, 218, 0.10)',
    '--ok': '#1a7f37',
    '--err': '#cf222e',
    '--warn': '#9a6700',
    '--ai': '#8250df',
    '--selection': 'rgba(9, 105, 218, 0.20)',
  },
  terminal: {
    background: '#ffffff',
    foreground: '#1f2328',
    cursor: '#0969da',
    cursorAccent: '#ffffff',
    selectionBackground: 'rgba(9, 105, 218, 0.20)',
    black: '#24292f',
    red: '#cf222e',
    green: '#1a7f37',
    yellow: '#9a6700',
    blue: '#0969da',
    magenta: '#8250df',
    cyan: '#1b7c83',
    white: '#6e7781',
    brightBlack: '#57606a',
    brightRed: '#a40e26',
    brightGreen: '#2da44e',
    brightYellow: '#bf8700',
    brightBlue: '#218bff',
    brightMagenta: '#a475f9',
    brightCyan: '#3192aa',
    brightWhite: '#8c959f',
  },
};

export const THEMES: Record<string, MkTheme> = { 'mk-dark': dark, 'mk-light': light };

export function resolveTheme(name: string): MkTheme {
  return THEMES[name] ?? dark;
}

export function applyTheme(theme: MkTheme, font: { family: string; size: number; lineHeight: number }) {
  const root = document.documentElement;
  for (const [k, v] of Object.entries(theme.css)) root.style.setProperty(k, v);
  root.style.setProperty('--font-mono', font.family);
  root.style.setProperty('--font-size', `${font.size}px`);
  root.style.setProperty('--line-height', String(font.lineHeight));
  root.dataset.theme = theme.name;
}
