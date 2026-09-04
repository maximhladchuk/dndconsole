import type { ImportReport, SelectionMode, Sound, SoundGroup } from '../types/api';

import { call } from './tauri';

export const libraryService = {
  listSounds: () => call<Sound[]>('list_sounds'),
  importSounds: (paths: string[]) => call<ImportReport>('import_sounds', { paths }),
  importDirectory: (path: string) => call<ImportReport>('import_sound_directory', { path }),
  rescan: () => call<Sound[]>('rescan_sounds'),

  rename: (id: number, name: string) => call<Sound>('rename_sound', { id, name }),
  setVolume: (id: number, volume: number) => call<Sound>('set_sound_volume', { id, volume }),
  setWeight: (id: number, weight: number) => call<Sound>('set_sound_weight', { id, weight }),
  setEnabled: (id: number, enabled: boolean) => call<Sound>('set_sound_enabled', { id, enabled }),
  setFavorite: (id: number, favorite: boolean) =>
    call<Sound>('set_sound_favorite', { id, favorite }),
  remove: (id: number) => call<Sound[]>('delete_sound', { id }),

  tags: (id: number) => call<string[]>('sound_tags', { id }),
  setTags: (id: number, tags: string[]) => call<string[]>('set_sound_tags', { id, tags }),

  listGroups: () => call<SoundGroup[]>('list_sound_groups'),
  createGroup: (name: string) => call<SoundGroup>('create_sound_group', { name }),
  updateGroup: (
    id: number,
    name: string,
    selectionMode: SelectionMode,
    preventRepeat: boolean,
    volume: number,
  ) =>
    call<SoundGroup>('update_sound_group', {
      id,
      name,
      selectionMode,
      preventRepeat,
      volume,
    }),
  deleteGroup: (id: number) => call<SoundGroup[]>('delete_sound_group', { id }),
  groupMembers: (id: number) => call<Sound[]>('sound_group_members', { id }),
  addToGroup: (groupId: number, soundId: number) =>
    call<Sound[]>('add_sound_to_group', { groupId, soundId }),
  removeFromGroup: (groupId: number, soundId: number) =>
    call<Sound[]>('remove_sound_from_group', { groupId, soundId }),
};
