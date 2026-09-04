import { Panel } from '../../ui/Panel';
import { formatBytes } from '../../domain/audio';
import type { AppStatus } from '../../types/api';

interface StatusPanelProps {
  status: AppStatus | null;
}

/**
 * Shows what actually exists today: the app, its database, the active profile and the
 * state of the audio output.
 */
export function StatusPanel({ status }: StatusPanelProps) {
  return (
    <Panel title="Програма" subtitle="Стан застосунку, бази даних і звукового виходу">
      <dl className="facts">
        <div>
          <dt>Версія</dt>
          <dd>{status?.version ?? '—'}</dd>
        </div>
        <div>
          <dt>Версія схеми</dt>
          <dd>{status?.schemaVersion ?? '—'}</dd>
        </div>
        <div>
          <dt>Активний профіль</dt>
          <dd>{status?.activeProfile?.name ?? '—'}</dd>
        </div>
        <div>
          <dt>Звуковий вихід</dt>
          <dd>
            {status?.audio.available
              ? `готовий · ${status.audio.activeOneShots} грає`
              : (status?.audio.unavailableReason ?? '—')}
          </dd>
        </div>
        <div>
          <dt>Кеш декодування</dt>
          <dd>{status ? formatBytes(status.audio.cacheUsedBytes) : '—'}</dd>
        </div>
        <div className="facts__wide">
          <dt>База даних</dt>
          <dd className="facts__path">{status?.databasePath ?? '—'}</dd>
        </div>
      </dl>
    </Panel>
  );
}
