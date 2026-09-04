import type { AppSettings, AppStatus, Profile } from '../types/api';

import { call } from './tauri';

export const appService = {
  status: () => call<AppStatus>('app_status'),

  getSettings: () => call<AppSettings>('get_settings'),
  saveSettings: (settings: AppSettings) => call<AppSettings>('save_settings', { settings }),

  listProfiles: () => call<Profile[]>('list_profiles'),
  createProfile: (name: string, description: string) =>
    call<Profile>('create_profile', { name, description }),
  setActiveProfile: (id: number) => call<Profile>('set_active_profile', { id }),
  deleteProfile: (id: number) => call<Profile[]>('delete_profile', { id }),
};
