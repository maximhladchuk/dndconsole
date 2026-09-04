import type { CaptureSnapshot, InputDevice } from '../types/api';

import { call } from './tauri';

export const microphoneService = {
  list: () => call<InputDevice[]>('list_microphones'),
  start: () => call<CaptureSnapshot>('start_listening'),
  stop: () => call<CaptureSnapshot>('stop_listening'),
  status: () => call<CaptureSnapshot>('capture_status'),
};
