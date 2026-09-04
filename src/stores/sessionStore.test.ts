import { beforeEach, describe, expect, it, vi } from 'vitest';

const listeners: ((update: unknown) => void)[] = [];
const subscribe = vi.fn(async (handler: (update: unknown) => void) => {
  listeners.push(handler);
  return () => {};
});

vi.mock('../services/session', () => ({
  sessionService: {
    subscribe: (handler: (update: unknown) => void) => subscribe(handler),
    status: async () => ({
      running: true,
      deviceName: 'test',
      level: 0,
      eventCount: 5,
      startedAtMs: 0,
    }),
  },
}));

const { useSessionStore } = await import('./sessionStore');

describe('sessionStore.subscribe', () => {
  beforeEach(() => {
    listeners.length = 0;
    subscribe.mockClear();
    useSessionStore.getState().clearHistory();
  });

  // React mounts effects twice in development, so both calls start before either has
  // registered its listener. Without a guard taken before the first await, every session
  // event is applied twice and every transcript appears twice in the log.
  it('registers exactly one listener when called concurrently', async () => {
    const store = useSessionStore.getState();
    await Promise.all([store.subscribe(), store.subscribe()]);

    expect(subscribe).toHaveBeenCalledTimes(1);
    expect(listeners).toHaveLength(1);

    listeners[0]({
      kind: 'transcript',
      text: "б'є мечем",
      isFinal: true,
      atMs: 1_700_000_000_000,
      sttMs: 210,
      speechMs: 900,
      language: 'uk',
    });

    expect(useSessionStore.getState().transcripts).toHaveLength(1);
  });
});
