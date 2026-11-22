<script lang="ts">
  // Terminal completo del panel: modo TTY clásico o aplicación en pantalla
  // alternativa (vim, htop, less…). Usa el renderer WebGL si está disponible.
  import type { PaneController } from './pane.svelte';

  let { pane, overlay = false }: { pane: PaneController; overlay?: boolean } = $props();
  let el: HTMLDivElement | undefined = $state();

  $effect(() => {
    if (el) pane.mountTty(el);
  });
</script>

<div class="tty" class:overlay bind:this={el}></div>

<style>
  .tty {
    position: absolute;
    inset: 0;
    padding: 2px 4px;
    background: var(--bg-block);
  }
  .tty.overlay {
    z-index: 5;
    box-shadow: 0 0 0 1px var(--accent) inset;
  }
</style>
