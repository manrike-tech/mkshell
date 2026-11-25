import { describe, expect, it } from 'vitest';
import { decodeFrame, Region } from './frames';

describe('decodeFrame', () => {
  it('decodifica datos con la cabecera de 11 bytes (igual que el test de Rust)', () => {
    const f = decodeFrame(new Uint8Array([0, 3, 1, 2, 1, 0, 0, 0, 0, 0, 0, 104, 105]));
    expect(f.kind).toBe('data');
    if (f.kind !== 'data') return;
    expect(f.region).toBe(Region.Output);
    expect(f.alt).toBe(true);
    expect(f.block).toBe(258);
    expect(new TextDecoder().decode(f.bytes)).toBe('hi');
  });

  it('bloque 0 significa sin bloque', () => {
    const f = decodeFrame(new Uint8Array([0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
    expect(f.kind === 'data' && f.block).toBe(null);
  });

  it('decodifica eventos JSON', () => {
    const json = new TextEncoder().encode('{"type":"altScreen","on":true}');
    const f = decodeFrame(Uint8Array.from([1, ...json]));
    expect(f).toEqual({ kind: 'event', event: { type: 'altScreen', on: true } });
  });

  it('acepta ArrayBuffer y arrays de números', () => {
    const arr = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 65];
    expect(decodeFrame(arr).kind).toBe('data');
    expect(decodeFrame(Uint8Array.from(arr).buffer).kind).toBe('data');
  });
});
