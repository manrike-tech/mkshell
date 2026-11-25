<script lang="ts">
  // Un bloque: cabecera (comando, estado, duración, origen) y su salida, ya sea
  // congelada (HTML estático) o viva (xterm.js mientras el comando se ejecuta).
  import { writeText } from '@tauri-apps/plugin-clipboard-manager';
  import { api } from '../../lib/ipc/api';
  import { app } from '../../lib/state/app.svelte';
  import type { BlockModel, PaneController } from './pane.svelte';
  import { highlightHtml } from './highlight';

  let { block, pane }: { block: BlockModel; pane: PaneController } = $props();

  let liveEl: HTMLDivElement | undefined = $state();

  $effect(() => {
    void block.liveKey;
    if (block.live && liveEl) pane.mountBlock(block, liveEl);
  });

  const segments = $derived(pane.query ? block.segments.map((s) => highlightHtml(s, pane.query)) : block.segments);
  const failed = $derived(block.status === 'done' && block.exitCode !== null && block.exitCode !== 0);
  const canEnableBlocks = $derived(
    !!block.ssh && block.status === 'running' && !block.nested && !block.injectRequested && pane.depth === block.depth,
  );

  function duration(ms: number): string {
    if (ms < 1000) return `${ms} ms`;
    const s = ms / 1000;
    if (s < 60) return `${s.toFixed(1)} s`;
    return `${Math.floor(s / 60)} min ${Math.round(s % 60)} s`;
  }

  function shortCwd(cwd: string | null): string {
    if (!cwd) return '';
    const parts = cwd.split(/[/\\]/).filter(Boolean);
    return parts.length > 3 ? `…/${parts.slice(-2).join('/')}` : cwd;
  }

  async function copyOutput() {
    await writeText(block.fullText());
    app.notify('Salida copiada');
  }

  async function copyCommand() {
    if (block.command) await writeText(block.command);
    app.notify('Comando copiado');
  }

  async function exportBlock() {
    const content = `$ ${block.command ?? ''}\n${block.fullText()}\n`;
    const path = await api.saveTextFile(`mkshell-bloque-${Math.abs(block.id)}.txt`, content).catch((e) => {
      app.notify(`No se pudo exportar: ${e}`);
      return null;
    });
    if (path) app.notify(`Exportado en ${path}`);
  }
</script>

<article
  class="block"
  class:running={block.status === 'running'}
  class:failed
  class:remote={block.depth > 0}
  class:agent={block.origin.kind === 'agent'}
  data-block={block.id}
>
  <header>
    <span class="status" title={block.status === 'running' ? 'En ejecución' : `Código ${block.exitCode ?? 'desconocido'}`}>
      {#if block.status === 'running'}
        <span class="spinner" aria-label="en ejecución"></span>
      {:else if block.startup}
        <span class="dot">·</span>
      {:else if block.exitCode === 0}
        <span class="ok">✓</span>
      {:else if block.exitCode === null}
        <span class="dot">?</span>
      {:else}
        <span class="err">✗ {block.exitCode}</span>
      {/if}
    </span>

    <code class="cmd" title={block.command ?? ''}>
      {block.startup ? 'Inicio del shell' : (block.command ?? '(comando)')}
    </code>

    {#if block.host}<span class="badge host" title="Ejecutado en un host remoto">{block.host}</span>{/if}
    {#if block.origin.kind === 'agent'}<span class="badge ai">IA · {block.origin.model}</span>{/if}
    {#if block.nested}<span class="badge">sesión activa</span>{/if}

    <span class="meta">
      {#if block.status === 'done' && !block.startup}{duration(block.durationMs)}{/if}
      {#if block.cwd}<span class="cwd" title={block.cwd}>{shortCwd(block.cwd)}</span>{/if}
    </span>

    <span class="actions">
      {#if canEnableBlocks}
        <button class="primary" onclick={() => app.requestBlocks(pane.id, block)} title="Inyecta la integración en el shell remoto">
          Activar bloques en {block.ssh}
        </button>
      {/if}
      {#if !block.startup && block.command}
        <button onclick={copyCommand} title="Copiar comando">⧉ cmd</button>
        {#if block.status === 'done'}
          <button onclick={() => pane.submit(block.command ?? '')} title="Volver a ejecutar">↻</button>
        {/if}
      {/if}
      <button onclick={copyOutput} title="Copiar salida">⧉</button>
      <button onclick={exportBlock} title="Exportar a fichero">⤓</button>
      <button onclick={() => (block.collapsed = !block.collapsed)} title={block.collapsed ? 'Expandir' : 'Contraer'}>
        {block.collapsed ? '▸' : '▾'}
      </button>
    </span>
  </header>

  {#if !block.collapsed && (block.live || segments.length > 0)}
    <div class="body">
      {#each segments as seg, i (i)}
        <div class="frozen">{@html seg}</div>
      {/each}
      {#if block.live}
        {#key block.liveKey}
          <div class="live" style:height="{block.liveRows * pane.rowPx}px" bind:this={liveEl}></div>
        {/key}
      {/if}
    </div>
  {/if}
</article>

<style>
  .block {
    border: 1px solid var(--border);
    border-left: 3px solid var(--border-strong);
    border-radius: 6px;
    background: var(--bg-block);
    margin: 6px 8px;
    overflow: hidden;
  }
  .block.remote {
    border-left-color: var(--accent);
  }
  .block.running {
    border-left-color: var(--warn);
  }
  .block.failed {
    border-left-color: var(--err);
  }
  .block.agent {
    border-left-color: var(--ai);
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 8px;
    font-size: 12px;
    color: var(--fg-muted);
    min-height: 26px;
  }
  .status {
    min-width: 2.6em;
    white-space: nowrap;
    font-family: var(--font-mono);
  }
  .ok {
    color: var(--ok);
  }
  .err {
    color: var(--err);
  }
  .cmd {
    font-family: var(--font-mono);
    font-size: var(--font-size);
    color: var(--fg);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 0 1 auto;
    min-width: 40px;
  }
  .badge {
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    padding: 0 5px;
    font-size: 11px;
    white-space: nowrap;
  }
  .badge.host {
    color: var(--accent);
    border-color: var(--accent);
  }
  .badge.ai {
    color: var(--ai);
    border-color: var(--ai);
  }
  .meta {
    margin-left: auto;
    display: flex;
    gap: 10px;
    white-space: nowrap;
  }
  .cwd {
    font-family: var(--font-mono);
  }
  .actions {
    display: flex;
    gap: 2px;
    opacity: 0;
    transition: opacity 0.1s;
  }
  .block:hover .actions,
  .block.running .actions {
    opacity: 1;
  }
  button {
    background: transparent;
    border: 1px solid transparent;
    color: var(--fg-muted);
    border-radius: 4px;
    padding: 1px 6px;
    font-size: 12px;
    cursor: pointer;
  }
  button:hover {
    border-color: var(--border-strong);
    color: var(--fg);
  }
  button.primary {
    color: var(--accent);
    border-color: var(--accent);
  }
  .body {
    padding: 4px 8px 6px;
    border-top: 1px solid var(--border);
  }
  .frozen {
    font-family: var(--font-mono);
    font-size: var(--font-size);
    line-height: var(--line-height);
    color: var(--fg);
    white-space: pre;
    overflow-x: auto;
    scrollbar-width: thin;
    user-select: text;
  }
  .frozen :global(div) {
    min-height: calc(var(--font-size) * var(--line-height));
  }
  .frozen :global(mark) {
    background: var(--warn);
    color: #000;
    border-radius: 2px;
  }
  .live {
    overflow: hidden;
  }
  .spinner {
    display: inline-block;
    width: 10px;
    height: 10px;
    border: 2px solid var(--warn);
    border-right-color: transparent;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
