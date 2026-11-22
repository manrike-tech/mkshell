// Tipos que reflejan la serialización serde del núcleo Rust.
// Mantener sincronizados con crates/mksh-core y crates/mksh-session.

export type ShellKind =
  | 'bash'
  | 'zsh'
  | 'fish'
  | 'pwsh'
  | 'powerShell'
  | 'cmd'
  | 'wsl'
  | 'posix'
  | 'other';

export interface Profile {
  id: string;
  name: string;
  program: string;
  args: string[];
  cwd: string | null;
  env: Record<string, string>;
  kind: ShellKind;
  integration: boolean;
}

export type SplitDirection = 'vertical' | 'horizontal';

export type LayoutNode =
  | { type: 'pane'; id: number }
  | {
      type: 'split';
      id: number;
      direction: SplitDirection;
      ratio: number;
      first: LayoutNode;
      second: LayoutNode;
    };

export interface PaneInfo {
  id: number;
  profileId: string;
  profileName: string;
  kind: ShellKind;
  integrated: boolean;
  cwd: string | null;
  pid: number | null;
}

export interface SessionInfo {
  id: number;
  name: string;
  layout: LayoutNode;
  panes: PaneInfo[];
}

export type Origin = { kind: 'user' } | { kind: 'agent'; model: string };

export type PaneEvent =
  | { type: 'promptStart'; depth: number }
  | { type: 'ready'; depth: number; cwd: string | null; host: string | null }
  | {
      type: 'blockStarted';
      block: number;
      depth: number;
      command: string | null;
      cwd: string | null;
      host: string | null;
      origin: Origin;
      ssh: string | null;
    }
  | { type: 'blockFinished'; block: number; exitCode: number | null; durationMs: number }
  | { type: 'cwdChanged'; depth: number; cwd: string; host: string | null }
  | { type: 'altScreen'; on: boolean }
  | { type: 'shellEntered'; depth: number }
  | { type: 'shellExited'; depth: number }
  | { type: 'exited'; code: number | null };

export type BlockOrder = 'newest-first' | 'oldest-first';

export interface Appearance {
  theme: string;
  font_family: string;
  font_size: number;
  line_height: number;
  block_order: BlockOrder;
}

export interface TerminalSettings {
  scrollback: number;
  tty_fallback_ms: number;
  max_blocks: number;
  copy_on_select: boolean;
}

export interface CommandDescriptor {
  id: string;
  title: string;
  phase: number;
}

export interface Bootstrap {
  version: string;
  platform: string;
  config: {
    appearance: Appearance;
    terminal: TerminalSettings;
    keybindings: Record<string, string>;
  };
  configPath: string;
  configError: string | null;
  profiles: Profile[];
  sessions: SessionInfo[];
  commands: CommandDescriptor[];
  sshTrustedHosts: string[];
  historyAvailable: boolean;
}

export interface HistoryEntry {
  id: number;
  ts: number;
  profile: string;
  command: string;
  cwd: string | null;
  host: string | null;
  exitCode: number | null;
  durationMs: number;
  origin: string;
}
