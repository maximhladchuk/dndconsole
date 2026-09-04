import { listen } from '@tauri-apps/api/event';

import type { DownloadUpdate, ModelInfo } from '../types/api';

const DOWNLOAD_EVENT = 'models://progress';

import { call } from './tauri';

export const modelsService = {
  list: () => call<ModelInfo[]>('list_models'),
  download: (id: string) => call<ModelInfo>('download_model', { id }),
  verify: (id: string) => call<string>('verify_model', { id }),
  remove: (id: string) => call<ModelInfo[]>('delete_model', { id }),
  directory: () => call<string>('model_directory'),

  subscribe: (handler: (update: DownloadUpdate) => void) =>
    listen<DownloadUpdate>(DOWNLOAD_EVENT, (event) => handler(event.payload)),
};
