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
      title="Набір звуків"
      subtitle={
        status === null
          ? 'Перевіряю…'
          : status.installed
            ? `${status.total} звуків готово · далі все працює без інтернету`
            : `${status.present} з ${status.total} звуків · завантажити близько ${status.megabytes.toFixed(1)} МБ`
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
              : 'Починаю…'}
          </p>
        </>
      ) : (
        <div className="library__actions">
          <button type="button" onClick={() => void store.install()}>
            {status?.installed ? 'Перевірити й полагодити' : 'Завантажити звуки'}
          </button>
        </div>
      )}

      {store.report ? (
        <p className="empty">
          Завантажено {store.report.downloaded}, уже було {store.report.reused}, груп готово{' '}
          {store.report.groups.length}
          {store.report.pruned > 0 ? `, застарілих видалено ${store.report.pruned}` : ''}.
          {store.report.failed.length > 0
            ? ` Не вдалося ${store.report.failed.length}: ${store.report.failed.join('; ')}`
            : ''}
        </p>
      ) : null}

      <p className="empty">
        Усі звуки — суспільне надбання (CC0), завантажуються з Freesound один раз і лежать
        на цьому комп’ютері. Під час гри нічого не качається.
      </p>
    </Panel>
  );
}
