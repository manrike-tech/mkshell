<script lang="ts">
  // Command Input: editor CodeMirror arriba del panel. Enter ejecuta,
  // Shift+Enter añade línea, ↑/↓ recorren el historial, Tab completa rutas.
  import { onDestroy, onMount } from 'svelte';
  import { Compartment, EditorState, Prec } from '@codemirror/state';
  import { EditorView, keymap, placeholder, drawSelection, tooltips } from '@codemirror/view';
  import { defaultKeymap, history, historyKeymap, insertNewline } from '@codemirror/commands';
  import { HighlightStyle, StreamLanguage, syntaxHighlighting } from '@codemirror/language';
  import { shell } from '@codemirror/legacy-modes/mode/shell';
  import { powerShell } from '@codemirror/legacy-modes/mode/powershell';
  import {
    acceptCompletion,
    autocompletion,
    completionKeymap,
    completionStatus,
    startCompletion,
    type CompletionContext,
    type CompletionResult,
  } from '@codemirror/autocomplete';
  import { tags as t } from '@lezer/highlight';
  import { api } from '../../lib/ipc/api';
  import type { PaneController } from '../terminal/pane.svelte';

  let { pane }: { pane: PaneController } = $props();

  let host: HTMLDivElement;
  let view: EditorView | null = null;
  const placeholderCfg = new Compartment();
  let histIdx = -1;
  let draft = '';

  const highlight = HighlightStyle.define([
    { tag: t.keyword, color: 'var(--accent)' },
    { tag: [t.string, t.special(t.string)], color: 'var(--ok)' },
    { tag: t.comment, color: 'var(--fg-muted)', fontStyle: 'italic' },
    { tag: [t.variableName, t.special(t.variableName)], color: 'var(--ai)' },
    { tag: [t.number, t.bool], color: 'var(--warn)' },
    { tag: t.operator, color: 'var(--err)' },
  ]);

  const language = () =>
    pane.info.kind === 'pwsh' || pane.info.kind === 'powerShell' ? StreamLanguage.define(powerShell) : StreamLanguage.define(shell);

  function placeholderText(): string {
    if (pane.exited) return 'El shell terminó.';
    if (pane.mode === 'pending') return 'Iniciando shell…';
    if (pane.running) return 'En ejecución · Enter envía texto al programa · Ctrl+C interrumpe';
    return 'Comando · Enter ejecuta · Shift+Enter nueva línea · Tab completa';
  }

  function setDoc(text: string) {
    if (!view) return;
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: text },
      selection: { anchor: text.length },
    });
  }

  function submit(v: EditorView): boolean {
    const text = v.state.doc.toString();
    histIdx = -1;
    draft = '';
    setDoc('');
    if (text.trim().length > 0 || pane.running) pane.submit(text);
    return true;
  }

  function historyMove(v: EditorView, dir: 1 | -1): boolean {
    const sel = v.state.selection.main;
    const line = v.state.doc.lineAt(sel.head);
    if (dir === 1 && line.number !== 1) return false;
    if (dir === -1 && line.number !== v.state.doc.lines) return false;
    if (completionStatus(v.state) === 'active') return false;
    const list = pane.history;
    if (dir === 1) {
      if (histIdx + 1 >= list.length) return true;
      if (histIdx === -1) draft = v.state.doc.toString();
      histIdx++;
      setDoc(list[histIdx]);
    } else {
      if (histIdx === -1) return false;
      histIdx--;
      setDoc(histIdx === -1 ? draft : list[histIdx]);
    }
    return true;
  }

  async function complete(ctx: CompletionContext): Promise<CompletionResult | null> {
    const doc = ctx.state.doc.toString();
    const word = ctx.matchBefore(/[^\s'"|;&<>]*/);
    const pathLike = word && /[/\\~.]/.test(word.text);
    if ((ctx.explicit || pathLike) && word) {
      const entries = await api.completePath(pane.id, word.text).catch(() => [] as string[]);
      if (entries.length > 0) {
        return {
          from: word.from,
          filter: false,
          options: entries.map((e) => ({ label: e, type: /[/\\]$/.test(e) ? 'namespace' : 'text' })),
        };
      }
    }
    if (doc.trim().length > 0 && ctx.pos === doc.length) {
      const options = pane.history
        .filter((h) => h.startsWith(doc) && h !== doc)
        .slice(0, 15)
        .map((h) => ({ label: h, type: 'keyword', detail: 'historial' }));
      if (options.length > 0) return { from: 0, filter: false, options };
    }
    return null;
  }

  const mkKeymap = Prec.highest(
    keymap.of([
      { key: 'Enter', run: (v) => (completionStatus(v.state) === 'active' ? acceptCompletion(v) : submit(v)) },
      { key: 'Shift-Enter', run: insertNewline },
      { key: 'ArrowUp', run: (v) => historyMove(v, 1) },
      { key: 'ArrowDown', run: (v) => historyMove(v, -1) },
      { key: 'Tab', run: (v) => (completionStatus(v.state) === 'active' ? acceptCompletion(v) : startCompletion(v)) },
      {
        key: 'Ctrl-c',
        run: (v) => {
          if (!v.state.selection.main.empty) return false; // copiar
          if (pane.running) pane.interrupt();
          else setDoc('');
          return true;
        },
      },
      {
        key: 'Ctrl-l',
        run: () => {
          pane.clear();
          return true;
        },
      },
    ]),
  );

  onMount(() => {
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: '',
        extensions: [
          mkKeymap,
          history(),
          drawSelection(),
          language(),
          syntaxHighlighting(highlight),
          // Las sugerencias se pintan en <body> para que el contenedor con scroll no las recorte.
          tooltips({ parent: document.body, position: 'fixed' }),
          autocompletion({ override: [complete], activateOnTyping: true, icons: false, maxRenderedOptions: 30 }),
          keymap.of([...completionKeymap, ...historyKeymap, ...defaultKeymap]),
          placeholderCfg.of(placeholder(placeholderText())),
          EditorView.lineWrapping,
          EditorView.contentAttributes.of({ 'aria-label': 'Command Input', spellcheck: 'false', autocapitalize: 'off' }),
        ],
      }),
    });
    pane.input = {
      focus: () => view?.focus(),
      setText: (text) => setDoc(text),
      insert: (text) => view?.dispatch(view.state.replaceSelection(text)),
    };
    // Panel recién creado con el foco (p. ej. tras dividir).
    if (pane.isFocused() && !pane.running) view.focus();
  });

  $effect(() => {
    const text = placeholderText();
    view?.dispatch({ effects: placeholderCfg.reconfigure(placeholder(text)) });
  });

  onDestroy(() => {
    if (pane.input) pane.input = null;
    view?.destroy();
  });
</script>

<div class="input" class:running={pane.running} class:disabled={pane.exited}>
  <span class="sigil" aria-hidden="true">{pane.running ? '…' : '❯'}</span>
  <div class="editor" bind:this={host}></div>
</div>

<style>
  .input {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 10px;
    background: var(--bg-input);
    border-bottom: 1px solid var(--border);
    min-height: 38px;
    max-height: 40%;
    overflow: auto;
  }
  .input.running .sigil {
    color: var(--warn);
  }
  .input.disabled {
    opacity: 0.5;
  }
  .sigil {
    color: var(--accent);
    font-family: var(--font-mono);
    font-size: var(--font-size);
    line-height: 22px;
    user-select: none;
  }
  .editor {
    flex: 1;
    min-width: 0;
  }
  .editor :global(.cm-editor) {
    background: transparent;
    color: var(--fg);
    font-family: var(--font-mono);
    font-size: var(--font-size);
  }
  .editor :global(.cm-editor.cm-focused) {
    outline: none;
  }
  .editor :global(.cm-content) {
    padding: 2px 0;
    caret-color: var(--accent);
    font-family: var(--font-mono);
  }
  .editor :global(.cm-line) {
    padding: 0;
    line-height: 18px;
  }
  .editor :global(.cm-placeholder) {
    color: var(--fg-muted);
  }
  .editor :global(.cm-selectionBackground),
  .editor :global(.cm-focused .cm-selectionBackground) {
    background: var(--selection) !important;
  }
</style>
