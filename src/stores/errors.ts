import { BackendError } from '../services/tauri';

export interface UiError {
  kind: string;
  message: string;
}

/**
 * Turn anything thrown by the service layer into something renderable.
 *
 * A `BackendError` keeps the backend's own `kind`, which the UI uses to decide what to
 * offer next — `modelMissing` points at the Models tab, `freesoundKeyMissing` at
 * Settings. Anything else is reported as-is rather than swallowed.
 */
export function toError(err: unknown): UiError {
  if (err instanceof BackendError) return { kind: err.kind, message: err.message };
  return { kind: 'unknown', message: err instanceof Error ? err.message : String(err) };
}
