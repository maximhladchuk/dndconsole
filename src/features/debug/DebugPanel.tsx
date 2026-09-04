import { useSessionStore } from '../../stores/sessionStore';
import { Panel } from '../../ui/Panel';

/**
 * Debug Mode: every candidate the detector considered, with its score, its threshold and
 * the reason it was rejected — plus the measured time each stage took.
 *
 * This is what makes tuning possible without guessing.
 */
export function DebugPanel() {
  const detections = useSessionStore((s) => s.detections);

  if (detections.length === 0) {
    return (
      <Panel title="Debug" subtitle="Candidate scores and rejection reasons">
        <p className="empty">Nothing detected yet.</p>
      </Panel>
    );
  }

  return (
    <Panel title="Debug" subtitle="Every candidate, accepted or not">
      <ul className="debug">
        {detections.map((record) => (
          <li key={record.id}>
            <p className="debug__transcript">
              {record.detection.isFinal ? '' : '(partial) '}
              “{record.detection.transcript}”
              <span className="log__meta"> {record.detectUs} µs</span>
            </p>

            {record.detection.candidates.length === 0 ? (
              <p className="empty">no candidates</p>
            ) : (
              <ul className="candidates">
                {record.detection.candidates.map((candidate) => (
                  <li key={candidate.eventId} className={candidate.accepted ? 'is-accepted' : ''}>
                    <span>{candidate.eventId}</span>
                    <span className="candidates__score">
                      {candidate.confidence.toFixed(2)} / {candidate.threshold.toFixed(2)}
                    </span>
                    <span className="candidates__why">
                      {candidate.accepted
                        ? `${candidate.layer}${candidate.actionWord ? ` · ${candidate.actionWord}` : ''}`
                        : (candidate.rejection?.reason ?? '')}
                    </span>
                  </li>
                ))}
              </ul>
            )}

            {record.decision.suppressed.length > 0 ? (
              <p className="debug__suppressed">
                suppressed: {record.decision.suppressed.map((s) => `${s.eventId} (${s.reason.reason})`).join(', ')}
              </p>
            ) : null}
          </li>
        ))}
      </ul>
    </Panel>
  );
}
