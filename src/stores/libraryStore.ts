import { create } from 'zustand';

import { libraryService } from '../services/library';
import { playbackService } from '../services/playback';
import { BackendError } from '../services/tauri';
import type { SelectionMode, Sound, SoundGroup } from '../types/api';

interface Notice {
  kind: string;
  message: string;
  tone: 'error' | 'info';
}

interface LibraryState {
  sounds: Sound[];
  groups: SoundGroup[];
  members: Record<number, Sound[]>;
  selectedGroupId: number | null;

  loading: boolean;
  busy: boolean;
  notice: Notice | null;
  lastPlayed: Sound | null;

  load: () => Promise<void>;
  importPaths: (paths: string[]) => Promise<void>;
  importDirectory: (path: string) => Promise<void>;
  rescan: () => Promise<void>;

  renameSound: (id: number, name: string) => Promise<void>;
  setSoundEnabled: (id: number, enabled: boolean) => Promise<void>;
  setSoundFavorite: (id: number, favorite: boolean) => Promise<void>;
  setSoundVolume: (id: number, volume: number) => Promise<void>;
  removeSound: (id: number) => Promise<void>;

  selectGroup: (id: number | null) => Promise<void>;
  createGroup: (name: string) => Promise<void>;
  updateGroup: (
    id: number,
    name: string,
    selectionMode: SelectionMode,
    preventRepeat: boolean,
    volume: number,
  ) => Promise<void>;
  deleteGroup: (id: number) => Promise<void>;
  addToGroup: (groupId: number, soundId: number) => Promise<void>;
  removeFromGroup: (groupId: number, soundId: number) => Promise<void>;

  preview: (id: number) => Promise<void>;
  playGroup: (id: number) => Promise<void>;
  stopAll: () => Promise<void>;
  dismissNotice: () => void;
}

function toNotice(err: unknown): Notice {
  if (err instanceof BackendError) return { kind: err.kind, message: err.message, tone: 'error' };
  return {
    kind: 'unknown',
    message: err instanceof Error ? err.message : String(err),
    tone: 'error',
  };
}

export const useLibraryStore = create<LibraryState>((set, get) => {
  /** Run a backend call, surfacing any failure as a notice instead of swallowing it. */
  const guard = async (action: () => Promise<void>) => {
    set({ busy: true, notice: null });
    try {
      await action();
    } catch (err) {
      set({ notice: toNotice(err) });
    } finally {
      set({ busy: false });
    }
  };

  const refreshMembers = async (groupId: number) => {
    const members = await libraryService.groupMembers(groupId);
    set((state) => ({ members: { ...state.members, [groupId]: members } }));
  };

  return {
    sounds: [],
    groups: [],
    members: {},
    selectedGroupId: null,
    loading: true,
    busy: false,
    notice: null,
    lastPlayed: null,

    load: async () => {
      set({ loading: true, notice: null });
      try {
        const [sounds, groups] = await Promise.all([
          libraryService.listSounds(),
          libraryService.listGroups(),
        ]);
        set({ sounds, groups, loading: false });

        const selected = get().selectedGroupId ?? groups[0]?.id ?? null;
        if (selected !== null) {
          set({ selectedGroupId: selected });
          await refreshMembers(selected);
        }
      } catch (err) {
        set({ notice: toNotice(err), loading: false });
      }
    },

    importPaths: (paths) =>
      guard(async () => {
        const report = await libraryService.importSounds(paths);
        set({ sounds: await libraryService.listSounds() });
        set({ notice: importNotice(report.imported.length, report.skipped) });
      }),

    importDirectory: (path) =>
      guard(async () => {
        const report = await libraryService.importDirectory(path);
        set({ sounds: await libraryService.listSounds() });
        set({ notice: importNotice(report.imported.length, report.skipped) });
      }),

    rescan: () =>
      guard(async () => {
        const sounds = await libraryService.rescan();
        const missing = sounds.filter((s) => s.missing).length;
        set({
          sounds,
          notice: {
            kind: 'rescan',
            tone: missing > 0 ? 'error' : 'info',
            message:
              missing > 0
                ? `${missing} file${missing === 1 ? '' : 's'} could not be found on disk.`
                : 'Every sound file is where the library expects it.',
          },
        });
      }),

    renameSound: (id, name) =>
      guard(async () => {
        await libraryService.rename(id, name);
        set({ sounds: await libraryService.listSounds() });
      }),

    setSoundEnabled: (id, enabled) =>
      guard(async () => {
        await libraryService.setEnabled(id, enabled);
        set({ sounds: await libraryService.listSounds() });
        const selected = get().selectedGroupId;
        if (selected !== null) await refreshMembers(selected);
      }),

    setSoundFavorite: (id, favorite) =>
      guard(async () => {
        await libraryService.setFavorite(id, favorite);
        set({ sounds: await libraryService.listSounds() });
      }),

    setSoundVolume: (id, volume) =>
      guard(async () => {
        await libraryService.setVolume(id, volume);
        set({ sounds: await libraryService.listSounds() });
      }),

    removeSound: (id) =>
      guard(async () => {
        const sounds = await libraryService.remove(id);
        set({ sounds });
        const selected = get().selectedGroupId;
        if (selected !== null) await refreshMembers(selected);
      }),

    selectGroup: (id) =>
      guard(async () => {
        set({ selectedGroupId: id });
        if (id !== null) await refreshMembers(id);
      }),

    createGroup: (name) =>
      guard(async () => {
        const group = await libraryService.createGroup(name);
        set({ groups: await libraryService.listGroups(), selectedGroupId: group.id });
        await refreshMembers(group.id);
      }),

    updateGroup: (id, name, selectionMode, preventRepeat, volume) =>
      guard(async () => {
        await libraryService.updateGroup(id, name, selectionMode, preventRepeat, volume);
        set({ groups: await libraryService.listGroups() });
      }),

    deleteGroup: (id) =>
      guard(async () => {
        const groups = await libraryService.deleteGroup(id);
        const next = groups[0]?.id ?? null;
        set({ groups, selectedGroupId: next });
        if (next !== null) await refreshMembers(next);
      }),

    addToGroup: (groupId, soundId) =>
      guard(async () => {
        const members = await libraryService.addToGroup(groupId, soundId);
        set((state) => ({ members: { ...state.members, [groupId]: members } }));
      }),

    removeFromGroup: (groupId, soundId) =>
      guard(async () => {
        const members = await libraryService.removeFromGroup(groupId, soundId);
        set((state) => ({ members: { ...state.members, [groupId]: members } }));
      }),

    preview: (id) =>
      guard(async () => {
        const sound = await playbackService.preview(id);
        set({ lastPlayed: sound });
      }),

    playGroup: (id) =>
      guard(async () => {
        const sound = await playbackService.playGroup(id);
        if (sound) {
          set({ lastPlayed: sound });
        } else {
          set({
            notice: {
              kind: 'emptyGroup',
              tone: 'info',
              message: 'This group has no enabled sounds to play.',
            },
          });
        }
      }),

    stopAll: () =>
      guard(async () => {
        await playbackService.stopAll();
        set({ lastPlayed: null });
      }),

    dismissNotice: () => set({ notice: null }),
  };
});

function importNotice(imported: number, skipped: { path: string; reason: string }[]): Notice {
  if (skipped.length === 0) {
    return {
      kind: 'import',
      tone: 'info',
      message: `Imported ${imported} sound${imported === 1 ? '' : 's'}.`,
    };
  }
  const first = skipped[0];
  return {
    kind: 'import',
    tone: 'error',
    message:
      `Imported ${imported}, skipped ${skipped.length}. ` +
      `First problem: ${first.path} — ${first.reason}`,
  };
}
