// Decodificación de los mensajes binarios de un panel (ver crates/mksh-session/src/frame.rs).
//
//   Datos:  [0][región][flags][bloque u64 LE][bytes…]  (cabecera de 11 bytes)
//   Evento: [1][JSON]

import type { PaneEvent } from './types';

export const Region = {
  Startup: 0,
  Prompt: 1,
  Echo: 2,
  Output: 3,
} as const;
export type Region = (typeof Region)[keyof typeof Region];

export type Frame =
  | { kind: 'data'; region: Region; alt: boolean; block: number | null; bytes: Uint8Array }
  | { kind: 'event'; event: PaneEvent };

const HEADER = 11;
const decoder = new TextDecoder();

export function toBytes(msg: ArrayBuffer | Uint8Array | number[]): Uint8Array {
  if (msg instanceof Uint8Array) return msg;
  if (msg instanceof ArrayBuffer) return new Uint8Array(msg);
  return Uint8Array.from(msg);
}

export function decodeFrame(msg: ArrayBuffer | Uint8Array | number[]): Frame {
  const buf = toBytes(msg);
  if (buf.length === 0) throw new Error('mensaje vacío');
  if (buf[0] === 1) {
    return { kind: 'event', event: JSON.parse(decoder.decode(buf.subarray(1))) as PaneEvent };
  }
  if (buf[0] !== 0 || buf.length < HEADER) throw new Error(`mensaje desconocido (${buf[0]})`);
  const view = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  const lo = view.getUint32(3, true);
  const hi = view.getUint32(7, true);
  const block = hi * 2 ** 32 + lo;
  return {
    kind: 'data',
    region: buf[1] as Region,
    alt: (buf[2] & 1) === 1,
    block: block === 0 ? null : block,
    bytes: buf.subarray(HEADER),
  };
}
