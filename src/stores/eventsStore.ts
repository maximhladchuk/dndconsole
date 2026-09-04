import { create } from 'zustand';

import { eventsService } from '../services/events';
import { libraryService } from '../services/library';
import type { EventDefinition, SoundGroup, StoredEvent } from '../types/api';

import { toError } from './errors';

interface EventsState {
  events: StoredEvent[];
  groups: SoundGroup[];
  selectedId: string | null;
  loading: boolean;
  saving: boolean;
  error: { kind: string; message: string } | null;

  load: () => Promise<void>;
  select: (id: string | null) => void;
  save: (definition: EventDefinition, soundGroupId: number | null, track: string) => Promise<void>;
  setEnabled: (id: string, enabled: boolean) => Promise<void>;
  setSoundGroup: (id: string, groupId: number | null) => Promise<void>;
  remove: (id: string) => Promise<void>;
  reset: (id: string) => Promise<void>;
  restoreDefaults: () => Promise<void>;
  dismissError: () => void;
}

export const useEventsStore = create<EventsState>((set, get) => {
  const guard = async (action: () => Promise<void>) => {
    set({ saving: true, error: null });
    try {
      await action();
    } catch (err) {
      set({ error: toError(err) });
    } finally {
      set({ saving: false });
    }
  };

  return {
    events: [],
    groups: [],
    selectedId: null,
    loading: true,
    saving: false,
    error: null,

    load: async () => {
      set({ loading: true, error: null });
      try {
        const [events, groups] = await Promise.all([
          eventsService.list(),
          libraryService.listGroups(),
        ]);
        set({
          events,
          groups,
          loading: false,
          selectedId: get().selectedId ?? events[0]?.definition.id ?? null,
        });
      } catch (err) {
        set({ error: toError(err), loading: false });
      }
    },

    select: (id) => set({ selectedId: id }),

    save: (definition, soundGroupId, track) =>
      guard(async () => {
        await eventsService.save(definition, soundGroupId, track);
        set({ events: await eventsService.list(), selectedId: definition.id });
      }),

    setEnabled: (id, enabled) =>
      guard(async () => {
        await eventsService.setEnabled(id, enabled);
        set({ events: await eventsService.list() });
      }),

    setSoundGroup: (id, groupId) =>
      guard(async () => {
        await eventsService.setSoundGroup(id, groupId);
        set({ events: await eventsService.list() });
      }),

    remove: (id) =>
      guard(async () => {
        const events = await eventsService.remove(id);
        set({ events, selectedId: events[0]?.definition.id ?? null });
      }),

    reset: (id) =>
      guard(async () => {
        const updated = await eventsService.reset(id);
        set((state) => ({
          events: state.events.map((e) =>
            e.definition.id === updated.definition.id ? updated : e,
          ),
        }));
      }),

    restoreDefaults: () =>
      guard(async () => {
        set({ events: await eventsService.restoreDefaults() });
      }),

    dismissError: () => set({ error: null }),
  };
});
