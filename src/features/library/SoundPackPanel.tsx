import { useEffect } from 'react';

import { useSoundPackStore } from '../../stores/soundPackStore';
import { Panel } from '../../ui/Panel';
import { ErrorBanner } from '../../ui/ErrorBanner';

/**
 * The bundled sound pack.
 *
 * The application ships a list of CC0 sounds rather than the audio itself, and fetches
 * them once. This is the only screen in the application that touches the network; a
 * session never does.
 */
export function SoundPackPanel() {
  const store = useSoundPackStore();

  useEffect(() => {
    void store.refresh();
    // Installing refreshes this itself.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const status = store.status;
  const percent = store.progress
    ? Math.round((store.progress.done / Math.max(store.progress.total, 1)) * 100)
    : 0;

  return (
    <Panel
      title="Sound pack"
      subtitle={
        status === null
          ? 'Checking…'
          : status.installed
            ? `${status.total} sounds ready · everything works offline from here`
            : `${status.present} of ${status.total} sounds · about ${status.megabytes.toFixed(1)} MB to download`
      }
    >
      {store.error ? (
        <ErrorBanner
          kind={store.error.kind}
          message={store.error.message}
          onDismiss={() => store.dismissError()}
        />
      ) : null}

      {store.installing ? (
        <>
          <div
            className="level"
            role="progressbar"
            aria-valuenow={percent}
            aria-valuemin={0}
            aria-valuemax={100}
          >
            <div className="level__fill" style={{ width: `${percent}%` }} />
          </div>
          <p className="empty">
            {store.progress
              ? `${store.progress.done} / ${store.progress.total} — ${store.progress.current}`
              : 'Starting…'}
          </p>
        </>
      ) : (
        <div className="library__actions">
          <button type="button" onClick={() => void store.install()}>
            {status?.installed ? 'Check and repair' : 'Download sounds'}
          </button>
        </div>
      )}

      {store.report ? (
        <p className="empty">
          {store.report.downloaded} downloaded, {store.report.reused} already cached,{' '}
          {store.report.groups.length} groups ready
          {store.report.pruned > 0 ? `, ${store.report.pruned} outdated removed` : ''}.
          {store.report.failed.length > 0
            ? ` ${store.report.failed.length} failed: ${store.report.failed.join('; ')}`
            : ''}
        </p>
      ) : null}

      <p className="empty">
        Every sound is public domain (CC0), fetched from Freesound once and kept on this
        machine. Nothing is downloaded during a game.
      </p>
    </Panel>
  );
}
