// Geometría del árbol de paneles (espejo de mksh-core::layout para la navegación).

import type { LayoutNode } from '../ipc/types';

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export type Dir = 'left' | 'right' | 'up' | 'down';

const EPS = 1e-4;

export function paneIds(node: LayoutNode): number[] {
  return node.type === 'pane' ? [node.id] : [...paneIds(node.first), ...paneIds(node.second)];
}

export function rects(node: LayoutNode, area: Rect = { x: 0, y: 0, w: 1, h: 1 }): Map<number, Rect> {
  const out = new Map<number, Rect>();
  const walk = (n: LayoutNode, a: Rect) => {
    if (n.type === 'pane') {
      out.set(n.id, a);
      return;
    }
    if (n.direction === 'vertical') {
      const w1 = a.w * n.ratio;
      walk(n.first, { ...a, w: w1 });
      walk(n.second, { ...a, x: a.x + w1, w: a.w - w1 });
    } else {
      const h1 = a.h * n.ratio;
      walk(n.first, { ...a, h: h1 });
      walk(n.second, { ...a, y: a.y + h1, h: a.h - h1 });
    }
  };
  walk(node, area);
  return out;
}

const overlap = (a: number, al: number, b: number, bl: number) => Math.min(a + al, b + bl) - Math.max(a, b);

/** Panel vecino en una dirección (mismo criterio que LayoutTree::neighbor). */
export function neighbor(node: LayoutNode, pane: number, dir: Dir): number | null {
  const all = rects(node);
  const from = all.get(pane);
  if (!from) return null;
  let best: { d: number; o: number; id: number } | null = null;
  for (const [id, r] of all) {
    if (id === pane) continue;
    let d: number;
    let o: number;
    if (dir === 'left' && r.x + r.w <= from.x + EPS) [d, o] = [from.x - (r.x + r.w), overlap(from.y, from.h, r.y, r.h)];
    else if (dir === 'right' && r.x >= from.x + from.w - EPS)
      [d, o] = [r.x - (from.x + from.w), overlap(from.y, from.h, r.y, r.h)];
    else if (dir === 'up' && r.y + r.h <= from.y + EPS) [d, o] = [from.y - (r.y + r.h), overlap(from.x, from.w, r.x, r.w)];
    else if (dir === 'down' && r.y >= from.y + from.h - EPS)
      [d, o] = [r.y - (from.y + from.h), overlap(from.x, from.w, r.x, r.w)];
    else continue;
    if (o <= EPS) continue;
    if (!best || d < best.d - EPS || (Math.abs(d - best.d) <= EPS && o > best.o)) best = { d, o, id };
  }
  return best?.id ?? null;
}

/** Copia del árbol con la proporción de una división cambiada. */
export function withRatio(node: LayoutNode, splitId: number, ratio: number): LayoutNode {
  if (node.type === 'pane') return node;
  if (node.id === splitId) return { ...node, ratio };
  return { ...node, first: withRatio(node.first, splitId, ratio), second: withRatio(node.second, splitId, ratio) };
}
