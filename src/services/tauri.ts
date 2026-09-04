import { invoke } from '@tauri-apps/api/core';

import type { CommandError } from '../types/api';

/**
 * A command that failed in the Rust backend.
 *
 * Errors are never swallowed: every command rejection is turned into one of these and
 * surfaced in the UI with the backend's own explanation.
 */
export class BackendError extends Error {
  readonly kind: string;

  constructor(kind: string, message: string) {
    super(message);
    this.name = 'BackendError';
    this.kind = kind;
  }
}

function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as CommandError).kind === 'string' &&
    typeof (value as CommandError).message === 'string'
  );
}

/**
 * The single place the frontend talks to Rust. Components never call `invoke`
 * directly — they go through the feature services, which go through here.
 */
export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (raw) {
    if (isCommandError(raw)) {
      throw new BackendError(raw.kind, raw.message);
    }
    throw new BackendError('unknown', String(raw));
  }
}
