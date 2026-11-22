<script lang="ts">
  // Pestañas de sesión: cambiar, renombrar (doble clic), cerrar y crear con perfil.
  import { api } from '../../lib/ipc/api';
  import { app } from '../../lib/state/app.svelte';
  import { chordFor } from '../../lib/commands/keymap';

  let profileMenu = $state(false);
  let draft = $state('');

  const bindings = $derived(app.boot?.config.keybindings ?? {});

  function startRename(id: number, name: string) {
    app.renaming = id;
    draft = name;
  }

  $effect(() => {
    if (app.renaming !== null) {
      const s = app.sessions.find((x) => x.id === app.renaming);
      if (s && !draft) draft = s.name;
    }
  });

  async function commitRename() {
    const id = app.renaming;
    app.renaming = null;
    const name = draft.trim();
    draft = '';
    if (id !== null && name) await api.renameSession(id, name).catch((e) => app.notify(String(e)));
    app.focused()?.focus();
  }

  function focusOnMount(el: HTMLInputElement) {
    el.focus();
    el.select();
  }
</script>

<nav class="tabs" aria-label="Sesiones">
  {#each app.sessions as s (s.id)}
    <div class="tab" class:active={s.id === app.activeSession}>
      {#if app.renaming === s.id}
        <input
          class="rename"
          bind:value={draft}
          use:focusOnMount
          onblur={commitRename}
          onkeydown={(e) => {
            if (e.key === 'Enter') commitRename();
            if (e.key === 'Escape') {
              app.renaming = null;
              draft = '';
            }
          }}
        />
      {:else}
        <button
          class="name"
          onclick={() => app.activate(s.id)}
          ondblclick={() => startRename(s.id, s.name)}
          title="Doble clic para renombrar ({chordFor(bindings, 'session.rename') ?? ''})"
        >
          {s.name}
          {#if s.panes.length > 1}<span class="count">{s.panes.length}</span>{/if}
        </button>
        <button class="close" onclick={() => api.closeSession(s.id)} title="Cerrar sesión">✕</button>
      {/if}
    </div>
  {/each}

  <div class="new">
    <button onclick={() => app.newSession()} title="Nueva sesión ({chordFor(bindings, 'session.new') ?? ''})">＋</button>
    <button class="more" onclick={() => (profileMenu = !profileMenu)} title="Nueva sesión con otro perfil">▾</button>
    {#if profileMenu}
      <div class="menu" role="menu">
        {#each app.boot?.profiles ?? [] as p (p.id)}
          <button
            role="menuitem"
            onclick={() => {
              profileMenu = false;
              app.newSession(p.id);
            }}
          >
            {p.name}
            <span class="prog">{p.program}</span>
            {#if !p.integration}<span class="prog">· TTY</span>{/if}
          </button>
        {/each}
      </div>
    {/if}
  </div>
</nav>

<style>
  .tabs {
    display: flex;
    align-items: stretch;
    gap: 2px;
    padding: 4px 6px 0;
    background: var(--bg-elev);
    border-bottom: 1px solid var(--border);
    min-height: 32px;
    overflow-x: auto;
  }
  .tab {
    display: flex;
    align-items: center;
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: 6px 6px 0 0;
    padding: 0 2px 0 4px;
  }
  .tab.active {
    background: var(--bg);
    border-color: var(--border);
  }
  button {
    background: transparent;
    border: none;
    color: var(--fg-muted);
    cursor: pointer;
    padding: 4px 8px;
    font-size: 13px;
  }
  .tab.active .name {
    color: var(--fg);
  }
  .count {
    font-size: 11px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    padding: 0 5px;
    margin-left: 4px;
  }
  .close {
    font-size: 11px;
    padding: 2px 6px;
    opacity: 0.6;
  }
  .close:hover {
    color: var(--err);
    opacity: 1;
  }
  .rename {
    background: var(--bg-input);
    border: 1px solid var(--accent);
    color: var(--fg);
    border-radius: 4px;
    padding: 2px 6px;
    width: 140px;
  }
  .new {
    position: relative;
    display: flex;
    align-items: center;
  }
  .more {
    padding: 4px 4px;
  }
  .menu {
    position: absolute;
    top: 100%;
    left: 0;
    z-index: 20;
    background: var(--bg-elev);
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    min-width: 260px;
    padding: 4px;
    display: flex;
    flex-direction: column;
  }
  .menu button {
    text-align: left;
    color: var(--fg);
    border-radius: 4px;
  }
  .menu button:hover {
    background: var(--accent-soft);
  }
  .prog {
    color: var(--fg-muted);
    font-family: var(--font-mono);
    font-size: 11px;
    margin-left: 6px;
  }
</style>
