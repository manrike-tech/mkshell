<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { app } from './lib/state/app.svelte';
  import { matchCommand, prettyChord } from './lib/commands/keymap';
  import SessionTabs from './features/sessions/SessionTabs.svelte';
  import ConsentDialog from './features/sessions/ConsentDialog.svelte';
  import LayoutView from './features/layout/LayoutView.svelte';

  const bindings = $derived(app.boot?.config.keybindings ?? {});
  const hints = $derived(
    [
      ['session.new', 'nueva sesión'],
      ['pane.split_vertical', 'dividir ◫'],
      ['pane.split_horizontal', 'dividir ⊟'],
      ['pane.search', 'buscar'],
    ]
      .map(([cmd, label]) => {
        const chord = Object.entries(bindings).find(([, c]) => c === cmd)?.[0];
        return chord ? `${prettyChord(chord)} ${label}` : null;
      })
      .filter(Boolean)
      .join(' · '),
  );

  function onKeydown(e: KeyboardEvent) {
    if (!app.boot || app.consent) return;
    if (e.key === 'F1') {
      e.preventDefault();
      app.showHelp = !app.showHelp;
      return;
    }
    const cmd = matchCommand(bindings, e);
    if (!cmd) return;
    // Mientras se renombra una sesión, el teclado es del campo de texto.
    if (app.renaming !== null && cmd !== 'session.rename') return;
    e.preventDefault();
    e.stopPropagation();
    app.run(cmd).catch((err) => app.notify(String(err)));
  }

  onMount(() => {
    window.addEventListener('keydown', onKeydown, true);
    app.init();
  });
  onDestroy(() => window.removeEventListener('keydown', onKeydown, true));
</script>

<div class="app">
  {#if app.error}
    <div class="fatal">
      <h1>No se pudo iniciar MKShell</h1>
      <pre>{app.error}</pre>
    </div>
  {:else if !app.boot}
    <div class="loading">Iniciando…</div>
  {:else}
    <SessionTabs />
    <main>
      {#each app.sessions as s (s.id)}
        <div class="session" class:hidden={s.id !== app.activeSession}>
          <LayoutView node={s.layout} sessionId={s.id} />
        </div>
      {/each}
      {#if app.sessions.length === 0}
        <div class="empty">
          <p>No hay sesiones abiertas.</p>
          <div class="profiles">
            {#each app.boot.profiles as p (p.id)}
              <button onclick={() => app.newSession(p.id)}>{p.name}</button>
            {/each}
          </div>
        </div>
      {/if}
    </main>
    <footer>
      <span>MKShell {app.boot.version}</span>
      <span class="sep">·</span>
      <span>{app.boot.platform}</span>
      {#if !app.boot.historyAvailable}<span class="warn">· historial no disponible</span>{/if}
      <span class="spacer"></span>
      <span class="hints">{hints} · F1 atajos</span>
    </footer>
  {/if}

  {#if app.showHelp && app.boot}
    <div class="help" role="dialog" aria-label="Atajos de teclado">
      <h2>Atajos de teclado</h2>
      <p class="muted">Configurables en <code>{app.boot.configPath}</code></p>
      <table>
        <tbody>
          {#each Object.entries(bindings) as [chord, cmd] (chord)}
            {@const desc = app.boot.commands.find((c) => c.id === cmd)}
            <tr>
              <td><kbd>{prettyChord(chord)}</kbd></td>
              <td>{desc?.title ?? cmd}</td>
              <td class="muted">{desc && desc.phase > 2 ? `fase ${desc.phase}` : ''}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      <button onclick={() => (app.showHelp = false)}>Cerrar</button>
    </div>
  {/if}

  <ConsentDialog />

  {#if app.toast}
    <div class="toast" role="status">{app.toast}</div>
  {/if}
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: var(--bg);
    color: var(--fg);
  }
  main {
    flex: 1;
    min-height: 0;
    position: relative;
    padding: 6px;
  }
  .session {
    position: absolute;
    inset: 6px;
    display: flex;
  }
  .session.hidden {
    visibility: hidden;
    pointer-events: none;
  }
  footer {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 2px 10px;
    font-size: 11px;
    color: var(--fg-muted);
    border-top: 1px solid var(--border);
    background: var(--bg-elev);
    min-height: 22px;
  }
  .spacer {
    flex: 1;
  }
  .warn {
    color: var(--warn);
  }
  .loading,
  .fatal,
  .empty {
    margin: auto;
    padding: 40px;
    text-align: center;
    color: var(--fg-muted);
  }
  .fatal pre {
    text-align: left;
    color: var(--err);
    white-space: pre-wrap;
  }
  .profiles {
    display: flex;
    gap: 8px;
    justify-content: center;
    flex-wrap: wrap;
  }
  .profiles button,
  .help button {
    background: transparent;
    border: 1px solid var(--border-strong);
    color: var(--fg);
    border-radius: 5px;
    padding: 6px 14px;
    cursor: pointer;
  }
  .toast {
    position: fixed;
    bottom: 32px;
    left: 50%;
    transform: translateX(-50%);
    background: var(--bg-elev);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    padding: 8px 14px;
    font-size: 13px;
    z-index: 50;
    max-width: 80vw;
  }
  .help {
    position: fixed;
    top: 10%;
    left: 50%;
    transform: translateX(-50%);
    width: min(620px, 92vw);
    max-height: 75vh;
    overflow: auto;
    background: var(--bg-elev);
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    padding: 16px 20px;
    z-index: 45;
    font-size: 13px;
  }
  .help h2 {
    font-size: 15px;
    margin: 0 0 4px;
  }
  .help table {
    width: 100%;
    border-collapse: collapse;
    margin: 8px 0 12px;
  }
  .help td {
    padding: 3px 6px;
    border-bottom: 1px solid var(--border);
  }
  .muted {
    color: var(--fg-muted);
  }
  kbd,
  code {
    font-family: var(--font-mono);
    font-size: 12px;
  }
</style>
