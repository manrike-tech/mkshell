// Envoltorios tipados de los comandos IPC (apps/desktop/src-tauri/src/ipc).

import { Channel, invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { Bootstrap, HistoryEntry, SessionInfo, SplitDirection } from './types';

export const SESSIONS_EVENT = 'mksh://sessions';

export const api = {
  bootstrap: () => invoke<Bootstrap>('get_bootstrap'),

  createSession: (profileId: string | null = null, name: string | null = null) =>
    invoke<SessionInfo>('create_session', { profileId, name }),
  closeSession: (sessionId: number) => invoke<void>('close_session', { sessionId }),
  renameSession: (sessionId: number, name: string) => invoke<void>('rename_session', { sessionId, name }),

  splitPane: (paneId: number, direction: SplitDirection, profileId: string | null = null) =>
    invoke<SessionInfo>('split_pane', { paneId, direction, profileId }),
  closePane: (paneId: number) => invoke<void>('close_pane', { paneId }),
  setSplitRatio: (sessionId: number, splitId: number, ratio: number) =>
    invoke<void>('set_split_ratio', { sessionId, splitId, ratio }),

  attachPane: (paneId: number, onOutput: Channel<ArrayBuffer>) => invoke<void>('attach_pane', { paneId, onOutput }),
  runCommand: (paneId: number, text: string) => invoke<boolean>('run_command', { paneId, text }),
  writeInput: (paneId: number, data: string) => invoke<void>('write_input', { paneId, data }),
  resizePane: (paneId: number, cols: number, rows: number) => invoke<void>('resize_pane', { paneId, cols, rows }),
  ackOutput: (paneId: number, bytes: number) => invoke<void>('ack_output', { paneId, bytes }),

  history: (profileId: string | null, prefix: string | null = null, limit = 500) =>
    invoke<HistoryEntry[]>('history', { profileId, prefix, limit }),
  completePath: (paneId: number, prefix: string) => invoke<string[]>('complete_path', { paneId, prefix }),
  enableBlocks: (paneId: number, host: string | null, remember: boolean) =>
    invoke<string[]>('enable_blocks', { paneId, host, remember }),
  saveTextFile: (suggestedName: string, content: string) =>
    invoke<string | null>('save_text_file', { suggestedName, content }),

  onSessions: (cb: (sessions: SessionInfo[]) => void): Promise<UnlistenFn> =>
    listen<SessionInfo[]>(SESSIONS_EVENT, (e) => cb(e.payload)),
};

export { Channel };
