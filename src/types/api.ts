/**
 * Mirrors of the Rust DTOs exposed by Tauri commands.
 *
 * These are hand-kept rather than generated. The rule is that every field here maps
 * to a field in `src-tauri/src/commands.rs` or `crates/store`; if the Rust side
 * changes, this file changes in the same commit.
 */

export type Language = 'auto' | 'uk' | 'en';

/** Serialized from `dndsound_store::AppSettings` (snake_case on the wire). */
export interface AppSettings {
  input_device: string | null;

  speech_model: string;
  language: Language;

  vad_speech_threshold: number;
  vad_min_speech_ms: number;
  vad_silence_timeout_ms: number;
  vad_pre_roll_ms: number;
  vad_post_roll_ms: number;
  vad_max_segment_ms: number;

  event_sensitivity: number;

  master_volume: number;
  sfx_volume: number;
  ambience_volume: number;
  effects_muted: boolean;
  suppress_mic_during_playback: boolean;

  debug_mode: boolean;
  start_listening_on_launch: boolean;
  managed_library: boolean;
  active_profile_id: number | null;
}

/** Serialized from `dndsound_store::profiles::Profile` (camelCase on the wire). */
export interface Profile {
  id: number;
  name: string;
  description: string;
  isActive: boolean;
  createdAt: number;
  updatedAt: number;
}

export interface AppStatus {
  version: string;
  databasePath: string;
  schemaVersion: number | null;
  activeProfile: Profile | null;
  audio: PlaybackSnapshot;
}

/** Serialized from `CommandError`. `kind` is stable and safe to branch on. */
export interface CommandError {
  kind: string;
  message: string;
}

/** Serialized from `dndsound_store::sounds::Sound`. */
export interface Sound {
  id: number;
  displayName: string;
  filePath: string;
  managed: boolean;
  format: string;
  durationMs: number | null;
  sampleRate: number | null;
  channels: number | null;
  volume: number;
  weight: number;
  enabled: boolean;
  favorite: boolean;
  missing: boolean;
  provenance: Provenance;
}

/** Serialized from `dndsound_store::sounds::Provenance`. */
export interface Provenance {
  /** `local` or `freesound`. */
  source: string;
  sourceId: string;
  sourceUrl: string;
  /** Short licence name, e.g. `CC0`. Empty when unknown. */
  license: string;
  author: string;
  /** The credit line, when the licence requires one. Empty otherwise. */
  attribution: string;
}

// --- sound pack ---------------------------------------------------------------

/** Serialized from `dndsound_lib::commands::sound_pack::PackStatus`. */
export interface PackStatus {
  installed: boolean;
  total: number;
  present: number;
  megabytes: number;
}

/** Serialized from `dndsound_lib::sound_pack::InstallReport`. */
export interface SoundPackReport {
  downloaded: number;
  reused: number;
  groups: string[];
  failed: string[];
  /** Sounds from an older pack that are no longer part of it, removed. */
  pruned: number;
}

/** Emitted on `pack://progress` while installing. */
export interface PackProgress {
  done: number;
  total: number;
  current: string;
}

/** Serialized from `dndsound_store::sounds::GroupCount`. */
export interface GroupCount {
  groupId: number;
  sounds: number;
}

export type SelectionMode = 'random' | 'weighted' | 'sequential';

/** Serialized from `dndsound_store::sounds::SoundGroup`. */
export interface SoundGroup {
  id: number;
  name: string;
  selectionMode: SelectionMode;
  preventRepeat: boolean;
  volume: number;
}

export interface SkippedFile {
  path: string;
  reason: string;
}

export interface ImportReport {
  imported: Sound[];
  skipped: SkippedFile[];
}

export interface PlaybackSnapshot {
  available: boolean;
  unavailableReason: string | null;
  activeOneShots: number;
  activeAmbience: string[];
  cacheUsedBytes: number;
}

/** Serialized from `dndsound_pipeline::InputDevice`. */
export interface InputDevice {
  name: string;
  isDefault: boolean;
  defaultSampleRate: number | null;
  defaultChannels: number | null;
}

/** Serialized from `dndsound_pipeline::CaptureStatus`. */
export type CaptureStatusValue =
  | { state: 'stopped' }
  | { state: 'running' }
  | { state: 'failed'; detail: string };

export interface CaptureSnapshot {
  listening: boolean;
  deviceName: string | null;
  inputSampleRate: number | null;
  inputChannels: number | null;
  level: number;
  status: CaptureStatusValue | null;
}

// --- detection ---------------------------------------------------------------

export type MatchLayer = 'command' | 'exactPhrase' | 'stemPhrase' | 'fuzzy' | 'keyword';

export type RejectionReason =
  | { reason: 'belowThreshold'; detail: { score: number; threshold: number } }
  | { reason: 'negativePhrase'; detail: string }
  | { reason: 'noActionWord' }
  | { reason: 'framedAsMemoryOrHypothesis'; detail: string }
  | { reason: 'disabled' };

export interface Candidate {
  eventId: string;
  confidence: number;
  threshold: number;
  layer: MatchLayer;
  matchedSpan: string;
  accepted: boolean;
  rejection: RejectionReason | null;
  actionWord: string | null;
}

export interface Detection {
  transcript: string;
  normalized: string;
  isFinal: boolean;
  timestampMs: number;
  candidates: Candidate[];
  elapsedUs: number;
}

export type SuppressionReason =
  | { reason: 'cooldown'; detail: { remainingMs: number } }
  | { reason: 'duplicateSpan'; detail: { span: string } }
  | { reason: 'probability'; detail: { probability: number } };

export interface Suppressed {
  eventId: string;
  confidence: number;
  reason: SuppressionReason;
}

export interface Trigger {
  eventId: string;
  confidence: number;
  atMs: number;
  delayMs: number;
  transcript: string;
}

export interface Decision {
  triggers: Trigger[];
  suppressed: Suppressed[];
}

// --- session -----------------------------------------------------------------

export type SessionUpdate =
  | { kind: 'speechStarted'; atMs: number }
  | {
      kind: 'transcript';
      text: string;
      isFinal: boolean;
      atMs: number;
      sttMs: number;
      speechMs: number;
      language: string | null;
    }
  | { kind: 'detection'; detection: Detection; decision: Decision; detectUs: number }
  | {
      kind: 'played';
      eventId: string;
      soundName: string;
      confidence: number;
      atMs: number;
      latencyMs: number;
    }
  | { kind: 'noSound'; eventId: string; reason: string }
  | { kind: 'discarded'; atMs: number; speechMs: number; reason: string }
  | { kind: 'error'; message: string }
  | { kind: 'stopped' };

export interface SessionSnapshot {
  running: boolean;
  deviceName: string | null;
  level: number;
  eventCount: number;
  startedAtMs: number | null;
}

// --- events ------------------------------------------------------------------

export type EventKindValue = 'one_shot' | 'ambience_start' | 'ambience_stop';
export type TermKind = 'keyword' | 'action' | 'negative';
export type PhraseLang = 'en' | 'uk' | 'any';

export interface Phrase {
  lang: PhraseLang;
  text: string;
  isCommand: boolean;
}

export interface EventTerm {
  kind: TermKind;
  lang: PhraseLang;
  text: string;
}

export interface EventDefinition {
  id: string;
  displayName: string;
  category: string;
  kind: EventKindValue;
  phrases: Phrase[];
  terms: EventTerm[];
  confidenceThreshold: number;
  cooldownMs: number;
  probability: number;
  requireActionWord: boolean;
  enabled: boolean;
}

export interface StoredEvent {
  definition: EventDefinition;
  soundGroupId: number | null;
  track: string;
  /** True when the event ships with the application. */
  builtin: boolean;
  /** True once edited here, which stops the built-in definition overwriting it. */
  userModified: boolean;
}

export interface SimulationResult {
  detection: Detection;
  decision: Decision;
  played: string[];
}

// --- models ------------------------------------------------------------------

export type ModelKind = 'vad' | 'speech' | 'embedding' | 'support';

export interface ModelInfo {
  id: string;
  displayName: string;
  kind: ModelKind;
  approxBytes: number;
  license: string;
  languages: string;
  notes: string;
  downloaded: boolean;
  sizeOnDisk: number | null;
}

export interface DownloadUpdate {
  id: string;
  downloadedBytes: number;
  totalBytes: number | null;
  done: boolean;
}
