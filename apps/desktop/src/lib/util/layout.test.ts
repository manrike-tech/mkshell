import { describe, expect, it } from 'vitest';
import type { LayoutNode } from '../ipc/types';
import { neighbor, paneIds, rects, withRatio } from './layout';

// Mismo árbol que el test de Rust: a | (b / c)
const tree: LayoutNode = {
  type: 'split',
  id: 10,
  direction: 'vertical',
  ratio: 0.5,
  first: { type: 'pane', id: 1 },
  second: {
    type: 'split',
    id: 11,
    direction: 'horizontal',
    ratio: 0.5,
    first: { type: 'pane', id: 2 },
    second: { type: 'pane', id: 3 },
  },
};

describe('layout', () => {
  it('lista paneles en orden', () => expect(paneIds(tree)).toEqual([1, 2, 3]));

  it('calcula rectángulos', () => {
    expect(rects(tree).get(3)).toEqual({ x: 0.5, y: 0.5, w: 0.5, h: 0.5 });
  });

  it('navega como el núcleo', () => {
    expect(neighbor(tree, 1, 'right')).toBe(2);
    expect(neighbor(tree, 3, 'left')).toBe(1);
    expect(neighbor(tree, 2, 'down')).toBe(3);
    expect(neighbor(tree, 1, 'left')).toBe(null);
  });

  it('cambia proporciones sin mutar', () => {
    const t2 = withRatio(tree, 11, 0.3);
    expect(rects(t2).get(2)?.h).toBeCloseTo(0.3);
    expect(rects(tree).get(2)?.h).toBeCloseTo(0.5);
  });
});
