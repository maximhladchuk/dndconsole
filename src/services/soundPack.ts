import { listen } from '@tauri-apps/api/event';

import type { PackProgress, PackStatus, SoundPackReport } from '../types/api';

import { call } from './tauri';

const PROGRESS_EVENT = 'pack://progress';

export const soundPackService = {
  status: () => call<PackStatus>('sound_pack_status'),
  install: () => call<SoundPackReport>('install_sound_pack'),

  /** Subscribe to install progress. Returns an unsubscribe function. */
  onProgress: (handler: (progress: PackProgress) => void) =>
    listen<PackProgress>(PROGRESS_EVENT, (event) => handler(event.payload)),
};
