import { create } from 'zustand';

import { microphoneService } from '../services/microphone';
import type { CaptureSnapshot, InputDevice } from '../types/api';

import { toError } from './errors';

/** How often the level meter is refreshed while listening. */
const POLL_INTERVAL_MS = 100;

interface CaptureState {
  devices: InputDevice[];
  snapshot: CaptureSnapshot | null;
  error: { kind: string; message: string } | null;
  pollHandle: number | null;

  loadDevices: () => Promise<void>;
  start: () => Promise<void>;
  stop: () => Promise<void>;
  refresh: () => Promise<void>;
  dismissError: () => void;
}

export const useCaptureStore = create<CaptureState>((set, get) => {
  /** Poll while listening; stop as soon as the stream is no longer running. */
  const startPolling = () => {
    if (get().pollHandle !== null) return;
    const handle = window.setInterval(() => {
      void get().refresh();
    }, POLL_INTERVAL_MS);
    set({ pollHandle: handle });
  };

  const stopPolling = () => {
    const handle = get().pollHandle;
    if (handle !== null) {
      window.clearInterval(handle);
      set({ pollHandle: null });
    }
  };

  return {
    devices: [],
    snapshot: null,
    error: null,
    pollHandle: null,

    loadDevices: async () => {
      try {
        set({ devices: await microphoneService.list(), error: null });
      } catch (err) {
        set({ error: toError(err) });
      }
    },

    start: async () => {
      try {
        set({ snapshot: await microphoneService.start(), error: null });
        startPolling();
      } catch (err) {
        set({ error: toError(err) });
      }
    },

    stop: async () => {
      stopPolling();
      try {
        set({ snapshot: await microphoneService.stop(), error: null });
      } catch (err) {
        set({ error: toError(err) });
      }
    },

    refresh: async () => {
      try {
        const snapshot = await microphoneService.status();
        set({ snapshot });

        // The microphone can vanish mid-session. Surface it and stop polling rather
        // than spinning forever against a dead stream.
        if (snapshot.status?.state === 'failed') {
          stopPolling();
          set({ error: { kind: 'microphoneLost', message: snapshot.status.detail } });
        } else if (!snapshot.listening) {
          stopPolling();
        }
      } catch (err) {
        stopPolling();
        set({ error: toError(err) });
      }
    },

    dismissError: () => set({ error: null }),
  };
});
