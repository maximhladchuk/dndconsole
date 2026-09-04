import { Panel } from '../../ui/Panel';
import { formatBytes } from '../../domain/audio';
import type { AppStatus } from '../../types/api';

interface StatusPanelProps {
  status: AppStatus | null;
}

/**
 * Shows what actually exists today: the app, its database, the active profile and the
 * state of the audio output. Listening controls arrive with the capture pipeline in
 * Phase 3 — a Start Listening button that does nothing would be worse than none.
 */
export function StatusPanel({ status }: StatusPanelProps) {
  return (
    <Panel title="Application" subtitle="Phase 1 — desktop shell, storage and settings">
      <dl className="facts">
        <div>
          <dt>Version</dt>
          <dd>{status?.version ?? '—'}</dd>
        </div>
        <div>
          <dt>Schema version</dt>
          <dd>{status?.schemaVersion ?? '—'}</dd>
        </div>
        <div>
          <dt>Active profile</dt>
          <dd>{status?.activeProfile?.name ?? '—'}</dd>
        </div>
        <div>
          <dt>Audio output</dt>
          <dd>
            {status?.audio.available
              ? `ready · ${status.audio.activeOneShots} playing`
              : (status?.audio.unavailableReason ?? '—')}
          </dd>
        </div>
        <div>
          <dt>Decode cache</dt>
          <dd>{status ? formatBytes(status.audio.cacheUsedBytes) : '—'}</dd>
        </div>
        <div className="facts__wide">
          <dt>Database</dt>
          <dd className="facts__path">{status?.databasePath ?? '—'}</dd>
        </div>
      </dl>
    </Panel>
  );
}
