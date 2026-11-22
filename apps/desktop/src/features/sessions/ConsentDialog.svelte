<script lang="ts">
  // Consentimiento explícito antes de enviar la integración a otro shell
  // (host SSH, WSL o shell sin integración).
  import { app } from '../../lib/state/app.svelte';

  function focusOnMount(el: HTMLButtonElement) {
    el.focus();
  }
</script>

{#if app.consent}
  {@const c = app.consent}
  <div class="backdrop" role="presentation" onclick={() => (app.consent = null)}></div>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="consent-title">
    <h2 id="consent-title">{c.host ? `Activar bloques en ${c.host}` : 'Activar bloques en este shell'}</h2>
    <p>
      MKShell enviará <strong>una línea</strong> al shell en primer plano que carga la integración OSC 133 en memoria
      (Bash o Zsh). No instala nada ni escribe en disco o en los dotfiles del host, y desaparece al cerrar la sesión.
    </p>
    <p class="muted">
      Requisitos: <code>base64</code> en el host. La línea empieza por un espacio para no quedar en el historial si
      <code>HISTCONTROL</code>/<code>HIST_IGNORE_SPACE</code> lo permiten. Con Fish, sh o BusyBox no tiene efecto.
    </p>
    {#if c.host}
      <label>
        <input type="checkbox" bind:checked={c.remember} />
        Recordar <code>{c.host}</code> y activarlo automáticamente en próximas conexiones
      </label>
    {/if}
    <div class="buttons">
      <button onclick={() => (app.consent = null)}>Cancelar</button>
      <button class="primary" use:focusOnMount onclick={() => app.confirmConsent()}>Activar</button>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    z-index: 40;
  }
  .dialog {
    position: fixed;
    top: 18%;
    left: 50%;
    transform: translateX(-50%);
    width: min(560px, 92vw);
    background: var(--bg-elev);
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    padding: 16px 20px;
    z-index: 41;
    color: var(--fg);
    font-size: 13px;
  }
  h2 {
    font-size: 15px;
    margin: 0 0 8px;
  }
  .muted {
    color: var(--fg-muted);
  }
  code {
    font-family: var(--font-mono);
    font-size: 12px;
  }
  label {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 10px 0;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 12px;
  }
  button {
    background: transparent;
    border: 1px solid var(--border-strong);
    color: var(--fg);
    border-radius: 5px;
    padding: 5px 14px;
    cursor: pointer;
  }
  button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #000;
  }
</style>
