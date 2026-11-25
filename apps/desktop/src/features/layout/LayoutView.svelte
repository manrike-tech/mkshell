<script lang="ts">
  // Árbol de paneles de una sesión. Las divisiones se redimensionan arrastrando
  // el separador; la proporción final se guarda en el núcleo.
  import Self from './LayoutView.svelte';
  import PaneView from '../terminal/PaneView.svelte';
  import { api } from '../../lib/ipc/api';
  import { app } from '../../lib/state/app.svelte';
  import type { LayoutNode } from '../../lib/ipc/types';
  import { withRatio } from '../../lib/util/layout';

  let { node, sessionId }: { node: LayoutNode; sessionId: number } = $props();

  let container: HTMLDivElement | undefined = $state();
  let dragRatio = $state<number | null>(null);

  const ratio = $derived(node.type === 'split' ? (dragRatio ?? node.ratio) : 1);

  function startDrag(e: PointerEvent) {
    if (node.type !== 'split' || !container) return;
    const split = node;
    const rect = container.getBoundingClientRect();
    const target = e.currentTarget as HTMLElement;
    target.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => {
      const r =
        split.direction === 'vertical'
          ? (ev.clientX - rect.left) / rect.width
          : (ev.clientY - rect.top) / rect.height;
      dragRatio = Math.min(0.95, Math.max(0.05, r));
    };
    const up = () => {
      target.removeEventListener('pointermove', move);
      target.removeEventListener('pointerup', up);
      if (dragRatio !== null) {
        const final = dragRatio;
        app.sessions = app.sessions.map((s) =>
          s.id === sessionId ? { ...s, layout: withRatio(s.layout, split.id, final) } : s,
        );
        api.setSplitRatio(sessionId, split.id, final).catch(() => {});
      }
      dragRatio = null;
    };
    target.addEventListener('pointermove', move);
    target.addEventListener('pointerup', up);
  }

  function onKey(e: KeyboardEvent) {
    if (node.type !== 'split') return;
    const step = e.shiftKey ? 0.1 : 0.02;
    const dec = node.direction === 'vertical' ? 'ArrowLeft' : 'ArrowUp';
    const inc = node.direction === 'vertical' ? 'ArrowRight' : 'ArrowDown';
    if (e.key !== dec && e.key !== inc) return;
    e.preventDefault();
    const next = Math.min(0.95, Math.max(0.05, node.ratio + (e.key === inc ? step : -step)));
    const id = node.id;
    app.sessions = app.sessions.map((s) => (s.id === sessionId ? { ...s, layout: withRatio(s.layout, id, next) } : s));
    api.setSplitRatio(sessionId, id, next).catch(() => {});
  }
</script>

{#if node.type === 'pane'}
  <PaneView paneId={node.id} />
{:else}
  <div class="split {node.direction}" bind:this={container}>
    <div class="child" style:flex-basis="{ratio * 100}%">
      <Self node={node.first} {sessionId} />
    </div>
    <!-- Separador enfocable (patrón ARIA «window splitter»): se mueve con flechas. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div
      class="divider"
      class:dragging={dragRatio !== null}
      role="separator"
      tabindex="0"
      aria-orientation={node.direction === 'vertical' ? 'vertical' : 'horizontal'}
      aria-valuenow={Math.round(ratio * 100)}
      aria-valuemin={5}
      aria-valuemax={95}
      onpointerdown={startDrag}
      onkeydown={onKey}
    ></div>
    <div class="child" style:flex-basis="{(1 - ratio) * 100}%">
      <Self node={node.second} {sessionId} />
    </div>
  </div>
{/if}

<style>
  .split {
    display: flex;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
  }
  .split.vertical {
    flex-direction: row;
  }
  .split.horizontal {
    flex-direction: column;
  }
  .child {
    flex-grow: 0;
    flex-shrink: 1;
    min-width: 0;
    min-height: 0;
    display: flex;
  }
  .divider {
    flex: 0 0 6px;
    background: transparent;
    position: relative;
  }
  .vertical > .divider {
    cursor: col-resize;
  }
  .horizontal > .divider {
    cursor: row-resize;
  }
  .divider:hover,
  .divider.dragging,
  .divider:focus-visible {
    background: var(--accent-soft);
    outline: none;
  }
</style>
