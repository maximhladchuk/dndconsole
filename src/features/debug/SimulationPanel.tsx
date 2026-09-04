import { useState } from 'react';

import type { RejectionReason } from '../../types/api';

import { useSessionStore } from '../../stores/sessionStore';
import { Panel } from '../../ui/Panel';

/**
 * Text Simulation Mode.
 *
 * Typed narration goes through the exact detection path the microphone uses, which makes
 * tuning events a matter of seconds rather than a matter of talking to your laptop.
 */
export function SimulationPanel() {
  const [text, setText] = useState('');
  const [play, setPlay] = useState(true);

  const simulate = useSessionStore((s) => s.simulate);
  const simulating = useSessionStore((s) => s.simulating);
  const result = useSessionStore((s) => s.simulation);

  const run = () => {
    if (text.trim()) void simulate(text, play);
  };

  const best = result?.detection.candidates.find((candidate) => candidate.accepted) ?? null;

  return (
    <Panel title="Simulate" subtitle="Type narration and run it through the real detector">
      <div className="library__actions">
        <input
          type="text"
          value={text}
          placeholder="The knight swings his sword at you."
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') run();
          }}
          aria-label="Narration to simulate"
        />
        <button type="button" onClick={run} disabled={!text.trim() || simulating}>
          {simulating ? '…' : 'Run'}
        </button>
      </div>

      <label className="field field--toggle">
        <input type="checkbox" checked={play} onChange={(e) => setPlay(e.target.checked)} />
        <span>
          <span className="field__label">Play the sound too</span>
        </span>
      </label>

      {result ? (
        <div className="simulation">
          {best ? (
            <p className="simulation__hit">
              <strong>{best.eventId}</strong> · {Math.round(best.confidence * 100)}%
              {result.played.length > 0 ? ` → ${result.played.join(', ')}` : ''}
            </p>
          ) : (
            <p className="simulation__miss">No event fired.</p>
          )}

          <ul className="candidates">
            {result.detection.candidates.map((candidate) => (
              <li key={candidate.eventId} className={candidate.accepted ? 'is-accepted' : ''}>
                <span>{candidate.eventId}</span>
                <span className="candidates__score">
                  {candidate.confidence.toFixed(2)} / {candidate.threshold.toFixed(2)}
                </span>
                <span className="candidates__why">
                  {candidate.accepted ? candidate.layer : explain(candidate.rejection)}
                </span>
              </li>
            ))}
          </ul>

          {result.decision.suppressed.length > 0 ? (
            <p className="simulation__miss">
              Suppressed: {result.decision.suppressed.map((s) => s.eventId).join(', ')}
            </p>
          ) : null}
        </div>
      ) : null}
    </Panel>
  );
}

/** Mirrors `RejectionReason::explain` in Rust, kept short for the compact list. */
function explain(rejection: RejectionReason | null): string {
  if (!rejection) return '';
  switch (rejection.reason) {
    case 'belowThreshold':
      return 'below threshold';
    case 'negativePhrase':
      return `negative: ${rejection.detail}`;
    case 'noActionWord':
      return 'no action word';
    case 'framedAsMemoryOrHypothesis':
      return `memory: ${rejection.detail}`;
    case 'disabled':
      return 'disabled';
  }
}
