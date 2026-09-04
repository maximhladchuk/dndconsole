import { useEffect } from 'react';

import { useAppStore } from '../../stores/appStore';
import { useSessionStore } from '../../stores/sessionStore';
import { Panel } from '../../ui/Panel';
import { LevelMeter } from '../../ui/LevelMeter';
import { DebugPanel } from '../debug/DebugPanel';
import { SimulationPanel } from '../debug/SimulationPanel';

import { MicrophonePanel } from './MicrophonePanel';
import { StatusPanel } from './StatusPanel';

function clock(atMs: number): string {
  return new Date(atMs).toLocaleTimeString(undefined, { hour12: false });
}

export function SessionPage() {
  const session = useSessionStore();
  const status = useAppStore((s) => s.status);
  const settings = useAppStore((s) => s.settings);
  const debugMode = settings?.debug_mode ?? false;

  useEffect(() => {
    void session.subscribe();
    // Subscribing once on mount is deliberate; the subscription lives for the app's life.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const running = session.snapshot?.running ?? false;
  const latest = session.transcripts[0];

  return (
    <div className="app__grid">
      <div className="app__column">
        <Panel
          title="Listening"
          subtitle={
            running
              ? `${session.snapshot?.deviceName ?? 'microphone'} · ${session.snapshot?.eventCount ?? 0} events armed`
              : 'Speech recognition runs entirely on this machine'
          }
        >
          <div className="session__state">
            <span className={running ? 'dot is-live' : 'dot'} aria-hidden="true" />
            <strong>{running ? (session.speaking ? 'Speaking' : 'Listening') : 'Stopped'}</strong>
          </div>

          <LevelMeter level={session.snapshot?.level ?? 0} active={running} />

          <div className="library__actions">
            {running ? (
              <button type="button" onClick={() => void session.stop()}>
                Stop session
              </button>
            ) : (
              <button type="button" onClick={() => void session.start()}>
                Start session
              </button>
            )}
            <button type="button" onClick={() => session.clearHistory()}>
              Clear history
            </button>
          </div>

          {latest ? (
            <p className={latest.isFinal ? 'session__latest' : 'session__latest is-partial'}>
              “{latest.text}”
            </p>
          ) : (
            <p className="empty">
              Start a session and narrate. Transcripts and triggered events appear here.
            </p>
          )}
        </Panel>

        <Panel title="Live transcript" subtitle="Most recent first, bounded history">
          {session.transcripts.length === 0 ? (
            <p className="empty">Nothing yet.</p>
          ) : (
            <ul className="log">
              {session.transcripts.map((line) => (
                <li key={line.id} className={line.isFinal ? '' : 'is-partial'}>
                  <span className="log__time">{clock(line.atMs)}</span>
                  <span className="log__text">{line.text}</span>
                  <span className="log__meta">
                    {line.language ?? ''} {line.sttMs}ms
                  </span>
                </li>
              ))}
            </ul>
          )}
        </Panel>
      </div>

      <div className="app__column">
        <Panel title="Triggered" subtitle="What actually played, and how fast">
          {session.activity.length === 0 ? (
            <p className="empty">No events have fired yet.</p>
          ) : (
            <ul className="log">
              {session.activity.map((line) => (
                <li key={line.id} className={line.soundName ? '' : 'is-warn'}>
                  <span className="log__time">{clock(line.atMs)}</span>
                  <span className="log__text">
                    <strong>{line.eventId}</strong>
                    {line.soundName ? ` → ${line.soundName}` : ` — ${line.note ?? 'no sound'}`}
                  </span>
                  <span className="log__meta">
                    {line.soundName ? `${Math.round(line.confidence * 100)}% · ${line.latencyMs}ms` : ''}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </Panel>

        <MicrophonePanel />
        <SimulationPanel />
        {debugMode ? <DebugPanel /> : null}
        <StatusPanel status={status} />
      </div>
    </div>
  );
}
