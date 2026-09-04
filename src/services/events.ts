import type { EventDefinition, StoredEvent } from '../types/api';

import { call } from './tauri';

export const eventsService = {
  list: () => call<StoredEvent[]>('list_events'),
  get: (id: string) => call<StoredEvent>('get_event', { id }),
  save: (definition: EventDefinition, soundGroupId: number | null, track: string) =>
    call<StoredEvent>('save_event', { definition, soundGroupId, track }),
  setEnabled: (id: string, enabled: boolean) =>
    call<StoredEvent>('set_event_enabled', { id, enabled }),
  setSoundGroup: (id: string, soundGroupId: number | null) =>
    call<StoredEvent>('set_event_sound_group', { id, soundGroupId }),
  remove: (id: string) => call<StoredEvent[]>('delete_event', { id }),
  reset: (id: string) => call<StoredEvent>('reset_event', { id }),
  restoreDefaults: () => call<StoredEvent[]>('restore_seed_events'),
};
