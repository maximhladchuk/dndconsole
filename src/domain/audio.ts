/**
 * Audio formats the importer accepts.
 *
 * Mirrors `SUPPORTED_EXTENSIONS` in `crates/sound/src/probe.rs`. The backend is the
 * authority — this list only shapes the file picker's filter.
 */
export const SUPPORTED_AUDIO_EXTENSIONS = ['wav', 'mp3', 'ogg', 'oga', 'flac'] as const;

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
