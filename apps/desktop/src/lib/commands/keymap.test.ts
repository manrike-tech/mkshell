import { describe, expect, it } from 'vitest';
import { chordFor, chordOf, matchCommand, prettyChord } from './keymap';

const ev = (key: string, code: string, mods: Partial<Record<'ctrl' | 'alt' | 'shift' | 'meta', boolean>> = {}) => ({
  key,
  code,
  ctrlKey: !!mods.ctrl,
  altKey: !!mods.alt,
  shiftKey: !!mods.shift,
  metaKey: !!mods.meta,
});

describe('chordOf', () => {
  it('usa el código físico para letras con Shift', () => {
    expect(chordOf(ev('D', 'KeyD', { ctrl: true, shift: true }))).toBe('ctrl+shift+d');
  });
  it('flechas y teclas especiales', () => {
    expect(chordOf(ev('ArrowLeft', 'ArrowLeft', { ctrl: true, shift: true }))).toBe('ctrl+shift+arrowleft');
    expect(chordOf(ev('PageDown', 'PageDown', { ctrl: true }))).toBe('ctrl+pagedown');
  });
  it('ignora pulsaciones de solo modificadores', () => {
    expect(chordOf(ev('Shift', 'ShiftLeft', { shift: true }))).toBe('');
  });
});

describe('matchCommand', () => {
  const bindings = { 'ctrl+shift+d': 'pane.split_vertical' };
  it('encuentra el comando', () => {
    expect(matchCommand(bindings, ev('D', 'KeyD', { ctrl: true, shift: true }))).toBe('pane.split_vertical');
    expect(matchCommand(bindings, ev('d', 'KeyD', { ctrl: true }))).toBe(null);
  });
  it('formatea atajos', () => {
    expect(prettyChord('ctrl+shift+arrowleft')).toBe('Ctrl+Shift+←');
    expect(chordFor(bindings, 'pane.split_vertical')).toBe('Ctrl+Shift+D');
  });
});
