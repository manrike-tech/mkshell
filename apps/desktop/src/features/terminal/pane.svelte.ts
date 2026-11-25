// Controlador de un panel: recibe los mensajes del núcleo, mantiene la lista
// de bloques y gestiona los terminales xterm.js (bloque vivo, TTY clásico y
// pantalla alternativa). Los componentes Svelte solo pintan este estado.

import { api, Channel } from '../../lib/ipc/api';
import { decodeFrame, Region, type Frame } from '../../lib/ipc/frames';
import type { Origin, PaneEvent, PaneInfo } from '../../lib/ipc/types';
import {
  createTerm,
  disposeTerm,
  freezeHtml,
  openTerm,
  plainText,
  usedRows,
  type TermHandle,
  type TermSettings,
} from './term';

export type PaneMode = 'pending' | 'blocks' | 'tty';

export interface PaneHost {
  settings: TermSettings;
  ttyFallbackMs: number;
  maxBlocks: number;
  isTrustedHost(host: string): boolean;
  isFocused(pane: number): boolean;
  notify(msg: string): void;
}

let startupSeq = 0;

export class BlockModel {
  readonly id: number;
  readonly command: string | null;
  readonly cwd: string | null;
  readonly host: string | null;
  readonly depth: number;
  readonly origin: Origin;
  readonly ssh: string | null;
  readonly startup: boolean;
  readonly startedAt = Date.now();

  status = $state<'running' | 'done'>('running');
  exitCode = $state<number | null>(null);
  durationMs = $state(0);
  /** HTML congelado de los tramos ya terminados. */
  segments = $state<string[]>([]);
  /** Texto plano de los tramos congelados (copiar, buscar, exportar). */
  text = $state('');
  /** Hay un terminal vivo para este bloque. */
  live = $state(false);
  /** Incrementa con cada terminal vivo nuevo (para remontar la vista). */
  liveKey = $state(0);
  liveRows = $state(1);
  collapsed = $state(false);
  /** Un shell anidado (ssh) está activo dentro de este bloque. */
  nested = $state(false);
  injectRequested = $state(false);

  handle: TermHandle | null = null;
  injectLine: number | null = null;
  idleTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(init: {
    id: number;
    command: string | null;
    cwd: string | null;
    host: string | null;
    depth: number;
    origin: Origin;
    ssh: string | null;
    startup?: boolean;
  }) {
    this.id = init.id;
    this.command = init.command;
    this.cwd = init.cwd;
    this.host = init.host;
    this.depth = init.depth;
    this.origin = init.origin;
    this.ssh = init.ssh;
    this.startup = !!init.startup;
  }

  /** Texto completo (tramos congelados + terminal vivo). */
  fullText(): string {
    const live = this.handle ? plainText(this.handle.term) : '';
    return [this.text, live].filter((t) => t.length > 0).join('\n');
  }
}

export class PaneController {
  readonly id: number;
  info = $state<PaneInfo>() as PaneInfo;

  mode = $state<PaneMode>('pending');
  ready = $state(false);
  running = $state(false);
  depth = $state(0);
  cwd = $state<string | null>(null);
  host = $state<string | null>(null);
  blocks = $state<BlockModel[]>([]);
  alt = $state(false);
  exited = $state(false);
  searchOpen = $state(false);
  query = $state('');
  /** Incrementa cuando cambian los terminales TTY/alt (para remontar). */
  ttyKey = $state(0);

  history: string[] = [];
  cols = 80;
  rows = 24;
  rowPx = 16;

  tty: TermHandle | null = null;
  altTerm: TermHandle | null = null;
  input: { focus(): void; setText(t: string): void; insert(t: string): void } | null = null;

  private host_: PaneHost;
  private rawBuffer: Uint8Array[] = [];
  private rawBytes = 0;
  private blocksRequested = false;
  private fallbackTimer: ReturnType<typeof setTimeout> | null = null;
  private ackPending = 0;
  private ackTimer: ReturnType<typeof setTimeout> | null = null;
  private resizeTimer: ReturnType<typeof setTimeout> | null = null;
  private disposed = false;

  constructor(info: PaneInfo, host: PaneHost) {
    this.id = info.id;
    this.info = info;
    this.cwd = info.cwd;
    this.host_ = host;
  }

  // ───────────────────────── ciclo de vida ─────────────────────────

  async attach() {
    const channel = new Channel<ArrayBuffer>();
    channel.onmessage = (msg) => {
      if (this.disposed) return;
      try {
        this.handleFrame(decodeFrame(msg as unknown as ArrayBuffer));
      } catch (e) {
        console.error('mensaje de panel inválido', e);
      }
    };
    if (!this.info.integrated) {
      this.enterTty();
    } else {
      this.fallbackTimer = setTimeout(() => {
        if (this.mode === 'pending') this.enterTty();
      }, this.host_.ttyFallbackMs);
    }
    await api.attachPane(this.id, channel);
    this.loadHistory();
  }

  dispose() {
    this.disposed = true;
    if (this.fallbackTimer) clearTimeout(this.fallbackTimer);
    for (const b of this.blocks) this.releaseBlock(b);
    disposeTerm(this.tty);
    disposeTerm(this.altTerm);
    this.tty = this.altTerm = null;
  }

  async loadHistory() {
    try {
      const entries = await api.history(this.info.profileId, null, 500);
      this.history = entries.map((e) => e.command);
    } catch {
      this.history = [];
    }
  }

  // ───────────────────────── mensajes del núcleo ─────────────────────────

  private handleFrame(f: Frame) {
    if (f.kind === 'event') {
      this.onEvent(f.event);
      return;
    }
    const n = f.bytes.length;
    if (this.mode === 'pending' && this.rawBytes < 2 * 1024 * 1024) {
      this.rawBuffer.push(f.bytes.slice());
      this.rawBytes += n;
    }
    if (this.mode === 'tty' && this.tty) {
      this.tty.term.write(f.bytes, () => this.ack(n));
      return;
    }
    if (f.alt && this.mode === 'blocks') {
      const h = this.ensureAlt();
      h.term.write(f.bytes, () => this.ack(n));
      return;
    }
    if (f.region === Region.Output && f.block !== null) {
      const b = this.blocks.find((x) => x.id === f.block);
      if (b) {
        this.writeBlock(b, f.bytes, n);
        return;
      }
    } else if (f.region === Region.Startup && this.mode !== 'tty') {
      this.writeBlock(this.startupBlock(), f.bytes, n);
      return;
    }
    // Prompt y eco: ocultos en modo bloques (el comando ya está en la cabecera).
    this.ack(n);
  }

  private onEvent(e: PaneEvent) {
    switch (e.type) {
      case 'promptStart':
        if (this.mode === 'pending') this.enterBlocks();
        else if (this.mode === 'tty' && this.blocksRequested) this.enterBlocks();
        this.depth = e.depth;
        break;
      case 'ready':
        this.depth = e.depth;
        this.ready = true;
        this.cwd = e.cwd ?? this.cwd;
        this.host = e.depth > 0 ? e.host : null;
        this.updateRunning();
        if (this.host_.isFocused(this.id) && !this.alt) queueMicrotask(() => this.focus());
        break;
      case 'blockStarted': {
        const b = new BlockModel({
          id: e.block,
          command: e.command,
          cwd: e.cwd,
          host: e.depth > 0 ? e.host : null,
          depth: e.depth,
          origin: e.origin,
          ssh: e.ssh,
        });
        this.blocks.push(b);
        this.ready = false;
        this.trimBlocks();
        this.updateRunning();
        break;
      }
      case 'blockFinished': {
        const b = this.blocks.find((x) => x.id === e.block);
        if (b) {
          b.status = 'done';
          b.exitCode = e.exitCode;
          b.durationMs = e.durationMs;
          if (b.idleTimer) clearTimeout(b.idleTimer);
          this.freeze(b);
        }
        this.updateRunning();
        break;
      }
      case 'cwdChanged':
        if (e.depth === this.depth) {
          this.cwd = e.cwd;
          this.host = e.depth > 0 ? e.host : null;
        }
        break;
      case 'altScreen':
        this.alt = e.on;
        if (!e.on) {
          disposeTerm(this.altTerm);
          this.altTerm = null;
          this.ttyKey++;
          if (this.host_.isFocused(this.id)) queueMicrotask(() => this.focus());
        }
        break;
      case 'shellEntered': {
        this.depth = e.depth;
        const outer = [...this.blocks].reverse().find((b) => b.status === 'running' && b.depth === e.depth - 1);
        if (outer) {
          outer.nested = true;
          // Se oculta el eco de la línea de arranque inyectada.
          this.freeze(outer, outer.injectLine ?? undefined);
        }
        this.updateRunning();
        break;
      }
      case 'shellExited': {
        this.depth = Math.max(0, e.depth - 1);
        const outer = [...this.blocks].reverse().find((b) => b.nested && b.depth === this.depth);
        if (outer) outer.nested = false;
        this.host = null;
        this.updateRunning();
        break;
      }
      case 'exited':
        this.exited = true;
        this.ready = false;
        this.running = false;
        for (const b of this.blocks) if (b.status === 'running') this.freeze(b);
        break;
    }
  }

  private updateRunning() {
    this.running = this.blocks.some((b) => b.status === 'running' && !b.nested && b.depth === this.depth);
  }

  // ───────────────────────── modos ─────────────────────────

  private enterBlocks() {
    if (this.fallbackTimer) clearTimeout(this.fallbackTimer);
    this.fallbackTimer = null;
    this.rawBuffer = [];
    this.rawBytes = 0;
    if (this.mode === 'tty') {
      disposeTerm(this.tty);
      this.tty = null;
      this.ttyKey++;
    }
    this.mode = 'blocks';
    this.blocksRequested = false;
    const s = this.blocks.find((b) => b.startup && b.status === 'running');
    if (s) {
      s.status = 'done';
      this.freeze(s, undefined, true);
    }
  }

  private enterTty() {
    if (this.mode === 'tty') return;
    if (this.fallbackTimer) clearTimeout(this.fallbackTimer);
    this.fallbackTimer = null;
    this.mode = 'tty';
    // El bloque de inicio no tiene sentido en TTY: se repite todo en la terminal.
    this.blocks = this.blocks.filter((b) => {
      if (b.startup) this.releaseBlock(b);
      return !b.startup;
    });
    this.tty = this.newTerm();
    for (const chunk of this.rawBuffer) this.tty.term.write(chunk);
    this.rawBuffer = [];
    this.rawBytes = 0;
    this.ttyKey++;
  }

  /** Alterna manualmente entre modo bloques y TTY clásico. */
  toggleTty() {
    if (this.mode === 'tty') {
      if (this.info.integrated || this.blocks.length > 0) {
        disposeTerm(this.tty);
        this.tty = null;
        this.mode = 'blocks';
        this.ttyKey++;
        // Un Enter redibuja el prompt y reactiva las marcas.
        api.writeInput(this.id, '\r');
      } else {
        this.host_.notify('Este shell no tiene integración: usa «Activar bloques».');
      }
    } else {
      this.enterTty();
      api.writeInput(this.id, '\x0c'); // Ctrl+L: el shell redibuja su prompt
    }
  }

  private newTerm(): TermHandle {
    const h = createTerm(this.host_.settings, this.cols, this.rows);
    h.term.onData((data) => {
      api.writeInput(this.id, data).catch(() => {});
    });
    h.term.onBinary((data) => {
      api.writeInput(this.id, data).catch(() => {});
    });
    return h;
  }

  private ensureAlt(): TermHandle {
    if (!this.altTerm) {
      this.altTerm = this.newTerm();
      this.ttyKey++;
    }
    return this.altTerm;
  }

  /** La vista TTY/alt monta su terminal aquí. */
  mountTty(el: HTMLElement) {
    const h = this.alt && this.mode === 'blocks' ? this.altTerm : this.tty;
    if (!h) return;
    openTerm(h, el, true);
    this.measureRow(el);
    if (this.host_.isFocused(this.id)) h.term.focus();
  }

  // ───────────────────────── bloques ─────────────────────────

  private startupBlock(): BlockModel {
    let s = this.blocks.find((b) => b.startup);
    if (!s) {
      s = new BlockModel({
        id: -++startupSeq,
        command: null,
        cwd: this.cwd,
        host: null,
        depth: 0,
        origin: { kind: 'user' },
        ssh: null,
        startup: true,
      });
      this.blocks.unshift(s);
    }
    return s;
  }

  private writeBlock(b: BlockModel, bytes: Uint8Array, n: number) {
    if (!b.handle) {
      b.handle = this.newTerm();
      b.live = true;
      b.liveKey++;
    }
    const h = b.handle;
    h.term.write(bytes, () => {
      this.ack(n);
      if (b.handle === h) {
        b.liveRows = Math.max(1, Math.min(usedRows(h.term), this.rows));
        this.maybeAutoInject(b);
      }
    });
  }

  /** La vista de un bloque monta su terminal vivo aquí. */
  mountBlock(b: BlockModel, el: HTMLElement) {
    if (!b.handle) return;
    openTerm(b.handle, el, false);
    this.measureRow(el);
    if (b.status === 'running' && this.host_.isFocused(this.id) && !this.alt) b.handle.term.focus();
  }

  private measureRow(el: HTMLElement) {
    const row = el.querySelector('.xterm-rows > div') as HTMLElement | null;
    const px = row?.getBoundingClientRect().height;
    if (px && px > 4) this.rowPx = px;
  }

  /** Congela el terminal vivo del bloque como HTML. `toLine` excluye el resto. */
  private freeze(b: BlockModel, toLine?: number, dropIfEmpty = false) {
    const h = b.handle;
    if (!h) {
      if (dropIfEmpty && b.segments.length === 0) this.removeBlock(b);
      return;
    }
    b.handle = null;
    h.term.write('', () => {
      const html = freezeHtml(h, toLine);
      const text = plainText(h.term, 0, toLine);
      if (html) b.segments.push(html);
      if (text) b.text = b.text ? `${b.text}\n${text}` : text;
      disposeTerm(h);
      b.live = false;
      if (dropIfEmpty && b.segments.length === 0) this.removeBlock(b);
    });
  }

  private releaseBlock(b: BlockModel) {
    if (b.idleTimer) clearTimeout(b.idleTimer);
    disposeTerm(b.handle);
    b.handle = null;
    b.live = false;
  }

  private removeBlock(b: BlockModel) {
    this.releaseBlock(b);
    this.blocks = this.blocks.filter((x) => x !== b);
  }

  private trimBlocks() {
    const max = this.host_.maxBlocks;
    while (this.blocks.length > max) {
      const victim = this.blocks.find((b) => b.status === 'done');
      if (!victim) break;
      this.removeBlock(victim);
    }
  }

  clear() {
    this.blocks = this.blocks.filter((b) => {
      const keep = b.status === 'running' && !b.startup;
      if (!keep) this.releaseBlock(b);
      return keep;
    });
    if (this.mode === 'tty') this.tty?.term.clear();
  }

  // ───────────────────────── SSH ─────────────────────────

  /** Inyección automática en hosts autorizados: espera a que el prompt remoto esté quieto. */
  private maybeAutoInject(b: BlockModel) {
    if (!b.ssh || b.status !== 'running' || b.nested || b.injectRequested) return;
    if (!this.host_.isTrustedHost(b.ssh)) return;
    if (b.idleTimer) clearTimeout(b.idleTimer);
    b.idleTimer = setTimeout(() => {
      const h = b.handle;
      if (!h || b.injectRequested || b.nested) return;
      const buf = h.term.buffer.active;
      const line = buf.getLine(buf.baseY + buf.cursorY)?.translateToString(true) ?? '';
      if (/[$#%>]\s?$/.test(line)) this.enableBlocks(b, false);
    }, 700);
  }

  /** Envía la integración al shell en primer plano (bloque ssh o panel TTY). */
  async enableBlocks(b: BlockModel | null, remember: boolean): Promise<string[] | null> {
    if (b) {
      b.injectRequested = true;
      const buf = b.handle?.term.buffer.active;
      b.injectLine = buf ? buf.baseY + buf.cursorY : null;
    } else {
      this.blocksRequested = true;
    }
    try {
      return await api.enableBlocks(this.id, b?.ssh ?? null, remember);
    } catch (e) {
      this.host_.notify(`No se pudo activar la integración: ${e}`);
      if (b) b.injectRequested = false;
      return null;
    }
  }

  // ───────────────────────── entrada ─────────────────────────

  isFocused(): boolean {
    return this.host_.isFocused(this.id);
  }

  async submit(text: string) {
    const t = text.replace(/\s+$/, '');
    if (this.mode === 'tty') {
      await api.writeInput(this.id, `${t}\r`);
      return;
    }
    // Antes del primer prompt (p. ej. una pregunta de los ficheros de arranque
    // del shell) el texto es entrada estándar, no un comando.
    if (!this.ready && !this.running) {
      await api.writeInput(this.id, `${t}\r`);
      return;
    }
    if (t.trim().length === 0) return;
    if (!this.running) {
      this.history = [t, ...this.history.filter((h) => h !== t)].slice(0, 1000);
    }
    await api.runCommand(this.id, t);
  }

  interrupt() {
    api.writeInput(this.id, '\x03').catch(() => {});
  }

  /** Bloque en ejecución en el shell activo (recibe el teclado). */
  activeBlock(): BlockModel | null {
    return [...this.blocks].reverse().find((b) => b.status === 'running' && !b.nested && b.depth === this.depth) ?? null;
  }

  focus() {
    if (this.alt && this.altTerm) return this.altTerm.term.focus();
    if (this.mode === 'tty' && this.tty) return this.tty.term.focus();
    const active = this.running ? this.activeBlock() : null;
    if (active?.handle?.opened) return active.handle.term.focus();
    this.input?.focus();
  }

  paste(text: string) {
    if (this.alt && this.altTerm) return this.altTerm.term.paste(text);
    if (this.mode === 'tty' && this.tty) return this.tty.term.paste(text);
    const active = this.running ? this.activeBlock() : null;
    if (active?.handle && document.activeElement?.closest('.xterm')) return active.handle.term.paste(text);
    this.input?.insert(text);
  }

  /** Texto seleccionado en cualquiera de los terminales del panel. */
  selection(): string {
    const terms = [this.altTerm, this.tty, ...this.blocks.map((b) => b.handle)];
    for (const h of terms) {
      const s = h?.term.getSelection();
      if (s) return s;
    }
    return window.getSelection()?.toString() ?? '';
  }

  exportText(): string {
    return this.blocks
      .map((b) => {
        const head = b.startup ? '# inicio' : `$ ${b.command ?? ''}`;
        const code = b.exitCode === null ? '' : `  # código ${b.exitCode}`;
        return `${head}${b.status === 'done' && !b.startup ? code : ''}\n${b.fullText()}`;
      })
      .join('\n\n');
  }

  // ───────────────────────── tamaño y control de flujo ─────────────────────────

  /** La vista informa del área disponible; se recalculan columnas y filas. */
  resize(widthPx: number, heightPx: number, cell: { w: number; h: number }) {
    const cols = Math.max(20, Math.floor((widthPx - 4) / cell.w));
    const rows = Math.max(5, Math.floor(heightPx / cell.h));
    if (cols === this.cols && rows === this.rows) return;
    this.cols = cols;
    this.rows = rows;
    for (const h of [this.tty, this.altTerm, ...this.blocks.map((b) => b.handle)]) {
      if (h) h.term.resize(cols, rows);
    }
    if (this.resizeTimer) clearTimeout(this.resizeTimer);
    this.resizeTimer = setTimeout(() => {
      api.resizePane(this.id, this.cols, this.rows).catch(() => {});
    }, 40);
  }

  private ack(n: number) {
    if (n === 0) return;
    this.ackPending += n;
    if (this.ackPending > 256 * 1024) return this.flushAck();
    if (!this.ackTimer) this.ackTimer = setTimeout(() => this.flushAck(), 30);
  }

  private flushAck() {
    if (this.ackTimer) clearTimeout(this.ackTimer);
    this.ackTimer = null;
    const n = this.ackPending;
    this.ackPending = 0;
    if (n > 0 && !this.disposed) api.ackOutput(this.id, n).catch(() => {});
  }
}
