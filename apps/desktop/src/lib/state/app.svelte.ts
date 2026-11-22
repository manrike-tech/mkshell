// Estado global de la interfaz: sesiones, foco, controladores de panel y
// ejecución de comandos del bus (atajos, botones).

import { SvelteMap } from 'svelte/reactivity';
import { readText, writeText } from '@tauri-apps/plugin-clipboard-manager';
import { api } from '../ipc/api';
import type { Bootstrap, LayoutNode, SessionInfo, SplitDirection } from '../ipc/types';
import { applyTheme, resolveTheme } from '../theme/themes';
import { neighbor, paneIds, type Dir } from '../util/layout';
import { measureCell, type TermSettings } from '../../features/terminal/term';
import { BlockModel, PaneController, type PaneHost } from '../../features/terminal/pane.svelte';

export interface ConsentRequest {
  pane: number;
  block: BlockModel | null;
  host: string | null;
  remember: boolean;
}

class AppStore implements PaneHost {
  boot = $state<Bootstrap | null>(null);
  error = $state<string | null>(null);
  sessions = $state<SessionInfo[]>([]);
  activeSession = $state<number | null>(null);
  focusedPane = $state<number | null>(null);
  renaming = $state<number | null>(null);
  toast = $state<string | null>(null);
  consent = $state<ConsentRequest | null>(null);
  trustedHosts = $state<string[]>([]);
  showHelp = $state(false);

  readonly controllers = new SvelteMap<number, PaneController>();
  private focusBySession = new Map<number, number>();
  private toastTimer: ReturnType<typeof setTimeout> | null = null;

  settings: TermSettings = {
    fontFamily: 'monospace',
    fontSize: 13,
    lineHeight: 1.2,
    scrollback: 10_000,
    theme: resolveTheme('mk-dark').terminal,
  };
  ttyFallbackMs = 3000;
  maxBlocks = 500;
  cell = { w: 8, h: 16 };

  // ───────────────────────── arranque ─────────────────────────

  async init() {
    try {
      const boot = await api.bootstrap();
      const a = boot.config.appearance;
      const theme = resolveTheme(a.theme);
      applyTheme(theme, { family: a.font_family, size: a.font_size, lineHeight: a.line_height });
      this.settings = {
        fontFamily: a.font_family,
        fontSize: a.font_size,
        lineHeight: a.line_height,
        scrollback: boot.config.terminal.scrollback,
        theme: theme.terminal,
      };
      this.ttyFallbackMs = boot.config.terminal.tty_fallback_ms;
      this.maxBlocks = boot.config.terminal.max_blocks;
      this.trustedHosts = boot.sshTrustedHosts;
      await document.fonts?.ready;
      this.cell = measureCell(a.font_family, a.font_size, a.line_height);
      this.boot = boot;
      await api.onSessions((s) => this.setSessions(s));
      this.setSessions(boot.sessions);
      if (boot.sessions.length === 0) await this.newSession();
      if (boot.configError) this.notify(`Configuración inválida, se usan valores por defecto: ${boot.configError}`, 8000);
    } catch (e) {
      this.error = String(e);
    }
  }

  // ───────────────────────── sesiones y paneles ─────────────────────────

  setSessions(list: SessionInfo[]) {
    this.sessions = list;
    const alive = new Set<number>();
    for (const s of list) {
      for (const p of s.panes) {
        alive.add(p.id);
        const existing = this.controllers.get(p.id);
        if (existing) {
          existing.info = p;
        } else {
          const c = new PaneController(p, this);
          this.controllers.set(p.id, c);
          c.attach().catch((e) => this.notify(`No se pudo conectar el panel: ${e}`));
        }
      }
    }
    for (const [id, c] of this.controllers) {
      if (!alive.has(id)) {
        c.dispose();
        this.controllers.delete(id);
      }
    }
    if (!list.some((s) => s.id === this.activeSession)) {
      this.activeSession = list.at(-1)?.id ?? null;
    }
    const session = this.currentSession();
    if (session) {
      const ids = paneIds(session.layout);
      const remembered = this.focusBySession.get(session.id);
      const target = this.focusedPane !== null && ids.includes(this.focusedPane) ? this.focusedPane : remembered;
      this.setFocus(target !== undefined && ids.includes(target) ? target : ids[0]);
    } else {
      this.focusedPane = null;
    }
  }

  currentSession(): SessionInfo | null {
    return this.sessions.find((s) => s.id === this.activeSession) ?? null;
  }

  currentLayout(): LayoutNode | null {
    return this.currentSession()?.layout ?? null;
  }

  setFocus(pane: number | null) {
    this.focusedPane = pane;
    const s = this.currentSession();
    if (s && pane !== null) this.focusBySession.set(s.id, pane);
    if (pane !== null) queueMicrotask(() => this.controllers.get(pane)?.focus());
  }

  activate(sessionId: number) {
    this.activeSession = sessionId;
    const s = this.currentSession();
    if (!s) return;
    const ids = paneIds(s.layout);
    const remembered = this.focusBySession.get(s.id);
    this.setFocus(remembered !== undefined && ids.includes(remembered) ? remembered : ids[0]);
  }

  async newSession(profileId: string | null = null) {
    try {
      const s = await api.createSession(profileId);
      this.activeSession = s.id;
      this.setSessions(this.sessions.some((x) => x.id === s.id) ? this.sessions : [...this.sessions, s]);
    } catch (e) {
      this.notify(`No se pudo crear la sesión: ${e}`);
    }
  }

  async split(direction: SplitDirection) {
    if (this.focusedPane === null) return;
    // Paneles existentes antes de dividir (el evento de sesiones puede llegar
    // antes que la respuesta del comando).
    const before = new Set(this.sessions.flatMap((x) => paneIds(x.layout)));
    try {
      const s = await api.splitPane(this.focusedPane, direction);
      this.sessions = this.sessions.map((x) => (x.id === s.id ? s : x));
      this.setSessions(this.sessions);
      const created = paneIds(s.layout).find((id) => !before.has(id));
      if (created !== undefined) this.setFocus(created);
    } catch (e) {
      this.notify(`No se pudo dividir: ${e}`);
    }
  }

  focusDir(dir: Dir) {
    const layout = this.currentLayout();
    if (!layout || this.focusedPane === null) return;
    const n = neighbor(layout, this.focusedPane, dir);
    if (n !== null) this.setFocus(n);
  }

  cycleSession(delta: number) {
    if (this.sessions.length < 2) return;
    const i = this.sessions.findIndex((s) => s.id === this.activeSession);
    const next = this.sessions[(i + delta + this.sessions.length) % this.sessions.length];
    this.activate(next.id);
  }

  // ───────────────────────── PaneHost ─────────────────────────

  isTrustedHost(host: string): boolean {
    return this.trustedHosts.includes(host);
  }

  isFocused(pane: number): boolean {
    return this.focusedPane === pane;
  }

  notify(msg: string, ms = 4000) {
    this.toast = msg;
    if (this.toastTimer) clearTimeout(this.toastTimer);
    this.toastTimer = setTimeout(() => (this.toast = null), ms);
  }

  // ───────────────────────── consentimiento SSH ─────────────────────────

  requestBlocks(pane: number, block: BlockModel | null) {
    this.consent = { pane, block, host: block?.ssh ?? null, remember: false };
  }

  async confirmConsent() {
    const c = this.consent;
    this.consent = null;
    if (!c) return;
    const ctrl = this.controllers.get(c.pane);
    const hosts = await ctrl?.enableBlocks(c.block, c.remember && c.host !== null);
    if (hosts) this.trustedHosts = hosts;
    ctrl?.focus();
  }

  // ───────────────────────── bus de comandos ─────────────────────────

  focused(): PaneController | null {
    return this.focusedPane === null ? null : (this.controllers.get(this.focusedPane) ?? null);
  }

  async run(command: string) {
    const pane = this.focused();
    switch (command) {
      case 'session.new':
        return this.newSession();
      case 'session.close':
        if (this.activeSession !== null) return api.closeSession(this.activeSession);
        return;
      case 'session.rename':
        this.renaming = this.activeSession;
        return;
      case 'session.next':
        return this.cycleSession(1);
      case 'session.prev':
        return this.cycleSession(-1);
      case 'pane.split_vertical':
        return this.split('vertical');
      case 'pane.split_horizontal':
        return this.split('horizontal');
      case 'pane.close':
        if (this.focusedPane !== null) return api.closePane(this.focusedPane);
        return;
      case 'pane.focus_left':
        return this.focusDir('left');
      case 'pane.focus_right':
        return this.focusDir('right');
      case 'pane.focus_up':
        return this.focusDir('up');
      case 'pane.focus_down':
        return this.focusDir('down');
      case 'pane.search':
        if (pane) pane.searchOpen = !pane.searchOpen;
        return;
      case 'pane.clear':
        return pane?.clear();
      case 'pane.toggle_tty':
        return pane?.toggleTty();
      case 'pane.interrupt':
        return pane?.interrupt();
      case 'input.focus':
        return pane?.input?.focus();
      case 'clipboard.copy': {
        const text = pane?.selection() ?? window.getSelection()?.toString() ?? '';
        if (text) await writeText(text);
        return;
      }
      case 'clipboard.paste': {
        const text = await readText().catch(() => '');
        if (text) pane?.paste(text);
        return;
      }
      case 'palette.open':
        return this.notify('La paleta de comandos llega en la fase 3.');
      case 'ai.toggle_panel':
        return this.notify('El panel de IA llega en la fase 4.');
      case 'mode.toggle':
        return this.notify('IDE Mode llega en la fase 5.');
      default:
        this.notify(`Comando desconocido: ${command}`);
    }
  }
}

export const app = new AppStore();
