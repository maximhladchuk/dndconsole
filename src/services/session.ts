import { listen } from '@tauri-apps/api/event';

import type { SessionSnapshot, SessionUpdate, SimulationResult } from '../types/api';

import { call } from './tauri';

/** Tauri event names, mirroring the constants in the Rust side. */
const SESSION_EVENT = 'session://event';

export const sessionService = {
  start: () => call<SessionSnapshot>('start_session'),
  stop: () => call<SessionSnapshot>('stop_session'),
  status: () => call<SessionSnapshot>('session_status'),

  simulate: (text: string, play: boolean) =>
    call<SimulationResult>('simulate_transcript', { text, play }),
  resetHistory: () => call<void>('reset_detection_history'),

  /** Subscribe to live session updates. Returns an unsubscribe function. */
  subscribe: (handler: (update: SessionUpdate) => void) =>
    listen<SessionUpdate>(SESSION_EVENT, (event) => handler(event.payload)),
};
