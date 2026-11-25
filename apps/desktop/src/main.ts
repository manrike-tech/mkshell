import { mount } from 'svelte';
import '@xterm/xterm/css/xterm.css';
import './app.css';
import App from './App.svelte';

// Errores no capturados: se muestran en pantalla en lugar de dejar la ventana vacía.
function showFatal(msg: string) {
  let el = document.getElementById('mk-fatal');
  if (!el) {
    el = document.createElement('pre');
    el.id = 'mk-fatal';
    el.style.cssText =
      'position:fixed;left:8px;right:8px;bottom:8px;max-height:40vh;overflow:auto;z-index:99;' +
      'background:#2a1215;color:#ffb4ab;border:1px solid #ff7b72;padding:8px;font:12px monospace;white-space:pre-wrap';
    document.body.appendChild(el);
  }
  el.textContent += `${msg}\n`;
}
window.addEventListener('error', (e) => showFatal(`Error: ${e.message} (${e.filename}:${e.lineno})`));
window.addEventListener('unhandledrejection', (e) => showFatal(`Promesa rechazada: ${String(e.reason)}`));

const target = document.getElementById('app');
if (!target) throw new Error('falta #app');

let app: ReturnType<typeof mount> | undefined;
try {
  app = mount(App, { target });
} catch (e) {
  showFatal(`Fallo al montar la interfaz: ${e instanceof Error ? (e.stack ?? e.message) : String(e)}`);
}
export default app;
