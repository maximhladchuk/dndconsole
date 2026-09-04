import { create } from 'zustand';

import { soundPackService } from '../services/soundPack';
import type { PackProgress, PackStatus, SoundPackReport } from '../types/api';

import { toError } from './errors';
import type { UiError } from './errors';

interface SoundPackState {
  status: PackStatus | null;
  installing: boolean;
  progress: PackProgress | null;
  report: SoundPackReport | null;
  error: UiError | null;

  refresh: () => Promise<void>;
  install: () => Promise<void>;
  dismissError: () => void;
}

let unlisten: (() => void) | null = null;
/** Set before the first await; see the note in `sessionStore`. */
let subscribing: Promise<void> | null = null;

export const useSoundPackStore = create<SoundPackState>((set, get) => ({
  status: null,
  installing: false,
  progress: null,
  report: null,
  error: null,

  refresh: async () => {
    try {
      set({ status: await soundPackService.status() });
    } catch (err) {
      set({ error: toError(err) });
    }
  },

  install: async () => {
    if (get().installing) return;
    set({ installing: true, error: null, report: null, progress: null });

    try {
      if (!unlisten && !subscribing) {
        subscribing = (async () => {
          try {
            unlisten = await soundPackService.onProgress((progress) => set({ progress }));
          } finally {
            subscribing = null;
          }
        })();
      }
      await subscribing;

      set({ report: await soundPackService.install() });
      await get().refresh();
    } catch (err) {
      set({ error: toError(err) });
    } finally {
      set({ installing: false, progress: null });
    }
  },

  dismissError: () => set({ error: null }),
}));
