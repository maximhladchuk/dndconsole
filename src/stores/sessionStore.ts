import { create } from 'zustand';

import { sessionService } from '../services/session';
import type {
  Detection,
  Decision,
  SessionSnapshot,
  SessionUpdate,
  SimulationResult,
} from '../types/api';

import { toError } from './errors';

/**
 * Bounded history sizes.
 *
 * A four-hour session produces thousands of transcripts. Keeping them all would grow
 * memory for the whole evening.
 */
const MAX_TRANSCRIPTS = 200;
const MAX_ACTIVITY = 200;
const MAX_DETECTIONS = 50;

export interface TranscriptLine {
  id: number;
  text: string;
  isFinal: boolean;
  atMs: number;
  sttMs: number;
  speechMs: number;
  language: string | null;
}

export interface ActivityLine {
  id: number;
  atMs: number;
  eventId: string;
  soundName: string | null;
  confidence: number;
  latencyMs: number | null;
  note: string | null;
}

export interface DetectionRecord {
  id: number;
  detection: Detection;
  decision: Decision;
  detectUs: number;
}

interface SessionState {
  snapshot: SessionSnapshot | null;
  transcripts: TranscriptLine[];
  activity: ActivityLine[];
  detections: DetectionRecord[];
  speaking: boolean;
  error: { kind: string; message: string } | null;

  simulation: SimulationResult | null;
  simulating: boolean;

  subscribe: () => Promise<void>;
  refresh: () => Promise<void>;
  start: () => Promise<void>;
  stop: () => Promise<void>;
  simulate: (text: string, play: boolean) => Promise<void>;
  clearHistory: () => void;
  dismissError: () => void;
}

let nextId = 1;
const id = () => nextId++;

/** Keep the newest `limit` entries. */
function bounded<T>(items: T[], next: T, limit: number): T[] {
  const combined = [next, ...items];
  return combined.length > limit ? combined.slice(0, limit) : combined;
}

let unsubscribe: (() => void) | null = null;
/**
 * Set synchronously, before the first `await`.
 *
 * `unsubscribe` alone is not a guard: it is only assigned once the listener has been
 * registered, so two callers that start before that both pass the check and both
 * subscribe. React's development mode mounts effects twice, which makes this happen every
 * single run — every session event was applied twice and every transcript appeared twice
 * in the log.
 */
let subscribing: Promise<void> | null = null;
let levelPoll: number | null = null;

export const useSessionStore = create<SessionState>((set, get) => ({
  snapshot: null,
  transcripts: [],
  activity: [],
  detections: [],
  speaking: false,
  error: null,
  simulation: null,
  simulating: false,

  subscribe: async () => {
    if (unsubscribe) return;
    if (subscribing) return subscribing;

    subscribing = (async () => {
      try {
        unsubscribe = await sessionService.subscribe((update) => apply(set, get, update));
        await get().refresh();
      } finally {
        subscribing = null;
      }
    })();

    return subscribing;
  },

  refresh: async () => {
    try {
      set({ snapshot: await sessionService.status() });
    } catch (err) {
      set({ error: toError(err) });
    }
  },

  start: async () => {
    set({ error: null });
    try {
      const snapshot = await sessionService.start();
      set({ snapshot });

      // The level meter is polled rather than pushed: it changes constantly and a
      // dropped frame costs nothing, while an event per audio block would flood IPC.
      if (levelPoll === null) {
        levelPoll = window.setInterval(() => {
          void get().refresh();
        }, 150);
      }
    } catch (err) {
      set({ error: toError(err) });
    }
  },

  stop: async () => {
    if (levelPoll !== null) {
      window.clearInterval(levelPoll);
      levelPoll = null;
    }
    try {
      set({ snapshot: await sessionService.stop(), speaking: false });
    } catch (err) {
      set({ error: toError(err) });
    }
  },

  simulate: async (text, play) => {
    set({ simulating: true, error: null });
    try {
      set({ simulation: await sessionService.simulate(text, play) });
    } catch (err) {
      set({ error: toError(err) });
    } finally {
      set({ simulating: false });
    }
  },

  clearHistory: () => set({ transcripts: [], activity: [], detections: [] }),
  dismissError: () => set({ error: null }),
}));

type Setter = (partial: Partial<SessionState> | ((state: SessionState) => Partial<SessionState>)) => void;

function apply(set: Setter, get: () => SessionState, update: SessionUpdate) {
  switch (update.kind) {
    case 'speechStarted':
      set({ speaking: true });
      break;

    case 'transcript':
      set((state) => ({
        speaking: !update.isFinal,
        transcripts: bounded(
          // A partial is replaced by its successor rather than stacking up.
          update.isFinal ? state.transcripts : state.transcripts.filter((t) => t.isFinal),
          {
            id: id(),
            text: update.text,
            isFinal: update.isFinal,
            atMs: update.atMs,
            sttMs: update.sttMs,
            speechMs: update.speechMs,
            language: update.language,
          },
          MAX_TRANSCRIPTS,
        ),
      }));
      break;

    case 'detection':
      set((state) => ({
        detections: bounded(
          state.detections,
          {
            id: id(),
            detection: update.detection,
            decision: update.decision,
            detectUs: update.detectUs,
          },
          MAX_DETECTIONS,
        ),
      }));
      break;

    case 'played':
      set((state) => ({
        activity: bounded(
          state.activity,
          {
            id: id(),
            atMs: update.atMs,
            eventId: update.eventId,
            soundName: update.soundName,
            confidence: update.confidence,
            latencyMs: update.latencyMs,
            note: null,
          },
          MAX_ACTIVITY,
        ),
      }));
      break;

    case 'noSound':
      set((state) => ({
        activity: bounded(
          state.activity,
          {
            id: id(),
            atMs: Date.now(),
            eventId: update.eventId,
            soundName: null,
            confidence: 0,
            latencyMs: null,
            note: update.reason,
          },
          MAX_ACTIVITY,
        ),
      }));
      break;

    case 'discarded':
      set({ speaking: false });
      break;

    case 'error':
      set({ error: { kind: 'session', message: update.message } });
      break;

    case 'stopped':
      set({ speaking: false });
      void get().refresh();
      break;
  }
}
