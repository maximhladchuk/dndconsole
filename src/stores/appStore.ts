import { create } from 'zustand';

import { appService } from '../services/app';
import type { AppSettings, AppStatus, Profile } from '../types/api';

import { toError } from './errors';

/**
 * Client state only. No business rules live here — the store calls services and
 * holds the result. Anything that decides something belongs in Rust.
 */
interface AppState {
  status: AppStatus | null;
  settings: AppSettings | null;
  profiles: Profile[];

  loading: boolean;
  saving: boolean;
  error: { kind: string; message: string } | null;

  load: () => Promise<void>;
  updateSettings: (patch: Partial<AppSettings>) => Promise<void>;
  createProfile: (name: string, description: string) => Promise<void>;
  activateProfile: (id: number) => Promise<void>;
  deleteProfile: (id: number) => Promise<void>;
  dismissError: () => void;
}

export const useAppStore = create<AppState>((set, get) => ({
  status: null,
  settings: null,
  profiles: [],
  loading: true,
  saving: false,
  error: null,

  load: async () => {
    set({ loading: true, error: null });
    try {
      const [status, settings, profiles] = await Promise.all([
        appService.status(),
        appService.getSettings(),
        appService.listProfiles(),
      ]);
      set({ status, settings, profiles, loading: false });
    } catch (err) {
      set({ error: toError(err), loading: false });
    }
  },

  updateSettings: async (patch) => {
    const current = get().settings;
    if (!current) return;

    // Optimistic: sliders must feel immediate. The backend's validated result is
    // written back on success, and the previous value is restored on failure.
    const optimistic = { ...current, ...patch };
    set({ settings: optimistic, saving: true, error: null });
    try {
      const saved = await appService.saveSettings(optimistic);
      set({ settings: saved, saving: false });
    } catch (err) {
      set({ settings: current, saving: false, error: toError(err) });
    }
  },

  createProfile: async (name, description) => {
    set({ error: null });
    try {
      await appService.createProfile(name, description);
      set({ profiles: await appService.listProfiles() });
    } catch (err) {
      set({ error: toError(err) });
    }
  },

  activateProfile: async (id) => {
    set({ error: null });
    try {
      await appService.setActiveProfile(id);
      const [profiles, status] = await Promise.all([
        appService.listProfiles(),
        appService.status(),
      ]);
      set({ profiles, status });
    } catch (err) {
      set({ error: toError(err) });
    }
  },

  deleteProfile: async (id) => {
    set({ error: null });
    try {
      const profiles = await appService.deleteProfile(id);
      set({ profiles, status: await appService.status() });
    } catch (err) {
      set({ error: toError(err) });
    }
  },

  dismissError: () => set({ error: null }),
}));
