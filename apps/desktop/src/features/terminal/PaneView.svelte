<script lang="ts">
  // Panel: cabecera, Command Input arriba y bloques debajo. En modo TTY o con
  // una aplicación a pantalla completa se superpone un terminal clásico.
  import { onDestroy, onMount } from 'svelte';
  import { api } from '../../lib/ipc/api';
  import { app } from '../../lib/state/app.svelte';
  import { chordFor } from '../../lib/commands/keymap';
  import CommandInput from '../command-input/CommandInput.svelte';
  import BlockView from './BlockView.svelte';
  import TtyView from './TtyView.svelte';
  import { matches } from './highlight';

  let { paneId }: { paneId: number } = $props();

  const pane = $derived(app.controllers.get(paneId));
  const focused = $derived(app.focusedPane === paneId);
  const order = $derived(app.boot?.config.appearance.block_order ?? 'newest-first');
  const bindings = $derived(app.boot?.config.keybindings ?? {});

  const visible = $derived.by(() => {
    if (!pane) return [];
    const list = pane.query ? pane.blocks.filter((b) => matches(pane.query, b.command, b.fullText())) : pane.blocks;
    return order === 'newest-first' ? [...list].reverse() : list;
  });

  let body: HTMLDivElement | undefined = $state();
  let blocksEl: HTMLDivElement | undefined = $state();
  let observer: ResizeObserver | null = null;

  onMount(() => {
    observer = new ResizeObserver(() => measure());
    if (body) observer.observe(body);
  });

  onDestroy(() => observer?.disconnect());

  function measure() {
    if (!body || !pane) return;
    const r = body.getBoundingClientRect();
    // Paneles ocultos (otra sesión activa) miden 0: no se redimensiona el PTY.
    if (r.width < 60 || r.height < 40) return;
    // Ancho útil dentro de un bloque: márgenes, borde y relleno (~44 px).
    pane.resize(r.width - 44, r.height - 8, app.cell);
  }

  // Orden cronológico: mantener la vista abajo mientras llega salida.
  $effect(() => {
    if (order !== 'oldest-first' || !blocksEl || !pane) return;
    void pane.blocks.length;
    for (const b of pane.blocks) void b.liveRows;
    blocksEl.scrollTop = blocksEl.scrollHeight;
  });

  function onFocusIn() {
    if (!focused) app.setFocus(paneId);
  }

  function shortCwd(cwd: string | null): string {
    if (!cwd) return '';
    return cwd.length > 48 ? `…${cwd.slice(-46)}` : cwd;
  }

  async function exportPane() {
    if (!pane) return;
    const path = await api.saveTextFile(`mkshell-panel-${paneId}.txt`, pane.exportText()).catch(() => null);
    if (path) app.notify(`Exportado en ${path}`);
  }
</script>

{#if pane}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <section
    class="pane"
    class:focused
    onfocusin={onFocusIn}
    onmousedown={onFocusIn}
    aria-label="Panel {pane.info.profileName}"
  >
    <header>
      <span class="profile">{pane.info.profileName}</span>
      {#if pane.host}<span class="host">{pane.host}</span>{/if}
      <span class="cwd" title={pane.cwd ?? ''}>{shortCwd(pane.cwd)}</span>
      {#if pane.mode === 'tty'}<span class="tag">TTY</span>{/if}
      {#if pane.mode === 'pending'}<span class="tag">iniciando…</span>{/if}
      {#if pane.alt}<span class="tag accent">pantalla completa</span>{/if}
      {#if pane.exited}<span class="tag err">terminado</span>{/if}
      <span class="spacer"></span>
      {#if pane.mode === 'tty' && !pane.exited}
        <button class="primary" onclick={() => app.requestBlocks(paneId, null)} title="Inyecta la integración en el shell actual (WSL, sh…)">
          Activar bloques
        </button>
      {/if}
      <button onclick={() => (pane.searchOpen = !pane.searchOpen)} title="Buscar ({chordFor(bindings, 'pane.search') ?? ''})">⌕</button>
      <button onclick={() => pane.toggleTty()} title="Alternar TTY clásico ({chordFor(bindings, 'pane.toggle_tty') ?? ''})">⌨</button>
      <button onclick={exportPane} title="Exportar salida del panel">⤓</button>
      <button onclick={() => pane.clear()} title="Limpiar ({chordFor(bindings, 'pane.clear') ?? ''})">⌫</button>
      <button
        onclick={() => api.splitPane(paneId, 'vertical').then(() => {})}
        title="Dividir lado a lado ({chordFor(bindings, 'pane.split_vertical') ?? ''})">◫</button
      >
      <button
        onclick={() => api.splitPane(paneId, 'horizontal').then(() => {})}
        title="Dividir arriba/abajo ({chordFor(bindings, 'pane.split_horizontal') ?? ''})">⊟</button
      >
      <button class="close" onclick={() => api.closePane(paneId)} title="Cerrar panel ({chordFor(bindings, 'pane.close') ?? ''})">✕</button>
    </header>

    {#if pane.searchOpen}
      <div class="search">
        <!-- svelte-ignore a11y_autofocus -->
        <input
          autofocus
          placeholder="Buscar en comandos y salida…"
          bind:value={pane.query}
          onkeydown={(e) => {
            if (e.key === 'Escape') {
              pane.query = '';
              pane.searchOpen = false;
              pane.focus();
            }
          }}
        />
        <span class="count">{pane.query ? `${visible.length} bloque(s)` : ''}</span>
      </div>
    {/if}

    <div class="body" bind:this={body}>
      {#if pane.mode !== 'tty'}
        <CommandInput {pane} />
        <div class="blocks" bind:this={blocksEl}>
          {#each visible as block (block.id)}
            <BlockView {block} {pane} />
          {/each}
          {#if pane.blocks.length === 0 && pane.mode === 'blocks'}
            <p class="empty">Listo. La salida de cada comando aparecerá aquí como un bloque.</p>
          {/if}
        </div>
      {/if}
      {#if pane.mode === 'tty' || pane.alt}
        {#key pane.ttyKey}
          <TtyView {pane} overlay={pane.mode !== 'tty'} />
        {/key}
      {/if}
    </div>
  </section>
{/if}

<style>
  .pane {
    display: flex;
    flex-direction: column;
    height: 100%;
    width: 100%;
    min-width: 0;
    min-height: 0;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
  }
  .pane.focused {
    border-color: var(--accent);
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 8px;
    background: var(--bg-elev);
    border-bottom: 1px solid var(--border);
    font-size: 12px;
    color: var(--fg-muted);
    min-height: 28px;
  }
  .profile {
    color: var(--fg);
    font-weight: 600;
  }
  .host {
    color: var(--accent);
    font-family: var(--font-mono);
  }
  .cwd {
    font-family: var(--font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .tag {
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    padding: 0 5px;
    font-size: 11px;
  }
  .tag.accent {
    color: var(--accent);
    border-color: var(--accent);
  }
  .tag.err {
    color: var(--err);
    border-color: var(--err);
  }
  .spacer {
    flex: 1;
  }
  button {
    background: transparent;
    border: 1px solid transparent;
    color: var(--fg-muted);
    border-radius: 4px;
    padding: 1px 6px;
    cursor: pointer;
    font-size: 13px;
  }
  button:hover {
    color: var(--fg);
    border-color: var(--border-strong);
  }
  button.primary {
    color: var(--accent);
    border-color: var(--accent);
    font-size: 12px;
  }
  button.close:hover {
    color: var(--err);
  }
  .search {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 4px 8px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-elev);
  }
  .search input {
    flex: 1;
    background: var(--bg-input);
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    color: var(--fg);
    padding: 3px 6px;
    font-family: var(--font-mono);
  }
  .count {
    font-size: 12px;
    color: var(--fg-muted);
  }
  .body {
    position: relative;
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .blocks {
    flex: 1;
    overflow-y: auto;
    padding-bottom: 8px;
  }
  .empty {
    color: var(--fg-muted);
    font-size: 12px;
    padding: 12px 14px;
  }
</style>
