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
    <Panel title="Перевірка текстом" subtitle="Напиши оповідь і пропусти її через справжній детектор">
      <div className="library__actions">
        <input
          type="text"
          value={text}
          placeholder="Лицар махнув мечем."
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') run();
          }}
          aria-label="Текст для перевірки"
        />
        <button type="button" onClick={run} disabled={!text.trim() || simulating}>
          {simulating ? '…' : 'Перевірити'}
        </button>
      </div>

      <label className="field field--toggle">
        <input type="checkbox" checked={play} onChange={(e) => setPlay(e.target.checked)} />
        <span>
          <span className="field__label">Ще й програти звук</span>
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
            <p className="simulation__miss">Жодна подія не спрацювала.</p>
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
              Придушено: {result.decision.suppressed.map((s) => s.eventId).join(', ')}
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
      return 'нижче порога';
    case 'negativePhrase':
      return `заперечення: ${rejection.detail}`;
    case 'noActionWord':
      return 'немає слова дії';
    case 'framedAsMemoryOrHypothesis':
      return `спогад або припущення: ${rejection.detail}`;
    case 'disabled':
      return 'вимкнено';
  }
}
