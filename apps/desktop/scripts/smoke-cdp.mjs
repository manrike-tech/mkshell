// Prueba de humo de la app de escritorio vía Chrome DevTools Protocol.
//
// WebView2 (Windows) expone CDP si la app se lanza con
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222
// El script espera a la ventana, diagnostica el puente IPC de Tauri, ejecuta
// un comando desde el Command Input, comprueba que aparece su bloque y guarda
// una captura. Uso:
//   node scripts/smoke-cdp.mjs [--port 9222] [--out smoke.png]
// Requiere Node 22 (WebSocket y fetch globales).

import { writeFileSync } from 'node:fs';

const arg = (name, def) => {
  const i = process.argv.indexOf(name);
  return i >= 0 ? process.argv[i + 1] : def;
};
const port = Number(arg('--port', '9222'));
const out = arg('--out', 'smoke.png');
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function waitFor(what, fn, timeoutMs) {
  const end = Date.now() + timeoutMs;
  let last;
  while (Date.now() < end) {
    try {
      last = await fn();
      if (last) return last;
    } catch (e) {
      last = e;
    }
    await sleep(500);
  }
  throw new Error(`timeout esperando ${what}: ${last instanceof Error ? last.message : JSON.stringify(last)}`);
}

const page = await waitFor(
  'la página de la ventana en CDP',
  async () => {
    const res = await fetch(`http://127.0.0.1:${port}/json/list`);
    const list = await res.json();
    return list.find((t) => t.type === 'page' && !t.url.startsWith('devtools://') && t.url !== 'about:blank');
  },
  60_000,
);
console.log(`página: ${page.url}`);

const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  ws.onopen = resolve;
  ws.onerror = reject;
});
let seq = 0;
const pending = new Map();
const consoleLines = [];
ws.onmessage = (m) => {
  const msg = JSON.parse(m.data);
  if (msg.id && pending.has(msg.id)) {
    pending.get(msg.id)(msg);
    pending.delete(msg.id);
  } else if (msg.method === 'Runtime.consoleAPICalled') {
    consoleLines.push(`console.${msg.params.type}: ${msg.params.args.map((a) => a.value ?? a.description).join(' ')}`);
  } else if (msg.method === 'Runtime.exceptionThrown') {
    consoleLines.push(`excepción: ${msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text}`);
  }
};
const send = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const id = ++seq;
    pending.set(id, (msg) => (msg.error ? reject(new Error(`${method}: ${msg.error.message}`)) : resolve(msg.result)));
    ws.send(JSON.stringify({ id, method, params }));
  });
const evaluate = async (expression) => {
  const r = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
  if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
  return r.result.value;
};
const screenshot = async (path) => {
  const r = await send('Page.captureScreenshot', { format: 'png' });
  writeFileSync(path, Buffer.from(r.data, 'base64'));
  console.log(`captura: ${path}`);
};

let failed = false;
try {
  await send('Runtime.enable');
  await send('Page.enable');

  // Espera a que el shell esté listo (placeholder «Comando…») o a una pantalla de error.
  const state = await waitFor(
    'el prompt del shell o una pantalla de error',
    () =>
      evaluate(`(() => {
        const err = document.querySelector('.fatal') ?? document.querySelector('#mk-fatal');
        if (err) return 'error:\\n' + err.innerText;
        if ((document.querySelector('.pane .cm-placeholder')?.innerText ?? '').startsWith('Comando')) return 'ok';
        return null;
      })()`),
    90_000,
  );
  if (state !== 'ok') throw new Error(`la app no arrancó: ${state}`);
  await evaluate(`document.querySelector('.pane .cm-content').focus(), true`);
  await send('Input.insertText', { text: 'echo mkshell-smoke' });
  const enter = { key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13, nativeVirtualKeyCode: 13 };
  await send('Input.dispatchKeyEvent', { type: 'keyDown', ...enter, text: '\r' });
  await send('Input.dispatchKeyEvent', { type: 'keyUp', ...enter });

  const block = await waitFor(
    'el bloque de «echo mkshell-smoke»',
    () =>
      evaluate(`(() => {
        const b = [...document.querySelectorAll('.block')].find((x) => x.innerText.includes('echo mkshell-smoke'));
        if (!b || b.classList.contains('running')) return null;
        return { text: b.innerText, failed: b.classList.contains('failed') };
      })()`),
    30_000,
  );
  console.log(`bloque:\n${block.text}`);
  const lines = block.text.split('\n').filter((l) => l.trim() === 'mkshell-smoke');
  if (block.failed || lines.length === 0) throw new Error('el bloque no contiene la salida esperada');
  console.log('resultado: OK');
} catch (e) {
  failed = true;
  console.error(`resultado: FALLO — ${e.message}`);
} finally {
  try {
    const diag = await evaluate(`JSON.stringify({
      href: location.href,
      isTauri: window.isTauri === true,
      internals: typeof window.__TAURI_INTERNALS__,
      invoke: typeof window.__TAURI_INTERNALS__?.invoke,
      webview2: typeof window.chrome?.webview,
      userAgent: navigator.userAgent,
    })`);
    console.log(`diagnóstico: ${diag}`);
  } catch (e) {
    console.error(`sin diagnóstico: ${e.message}`);
  }
  try {
    await screenshot(out);
  } catch (e) {
    console.error(`sin captura: ${e.message}`);
  }
  if (consoleLines.length) console.log(`consola:\n${consoleLines.join('\n')}`);
  ws.close();
}
process.exit(failed ? 1 : 0);
