import type { PlaybackSnapshot, Sound } from '../types/api';

import { call } from './tauri';

export const playbackService = {
  preview: (id: number) => call<Sound>('preview_sound', { id }),
  playGroup: (id: number) => call<Sound | null>('play_sound_group', { id }),
  startAmbience: (id: number) => call<Sound>('start_ambience', { id }),
  stopAmbience: (id: number) => call<void>('stop_ambience', { id }),
  stopAll: () => call<void>('stop_all_sounds'),
  status: () => call<PlaybackSnapshot>('playback_status'),
};
