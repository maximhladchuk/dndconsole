import { useState } from 'react';

import { EventsPage } from './features/events/EventsPage';
import { LibraryPage } from './features/library/LibraryPage';
import { SetupPage } from './features/models/SetupPage';
import { ProfilePanel } from './features/profiles/ProfilePanel';
import { SessionPage } from './features/session/SessionPage';
import { SettingsPanel } from './features/settings/SettingsPanel';
import { useBootstrap } from './hooks/useBootstrap';
import { useAppStore } from './stores/appStore';
import { useCaptureStore } from './stores/captureStore';
import { useEventsStore } from './stores/eventsStore';
import { useLibraryStore } from './stores/libraryStore';
import { useSessionStore } from './stores/sessionStore';
import { ErrorBanner } from './ui/ErrorBanner';

type Tab = 'session' | 'events' | 'library' | 'models' | 'settings';

const TABS: { id: Tab; label: string }[] = [
  { id: 'session', label: 'Сесія' },
  { id: 'events', label: 'Події' },
  { id: 'library', label: 'Звуки' },
  { id: 'models', label: 'Налаштування' },
  { id: 'settings', label: 'Параметри' },
];

export default function App() {
  useBootstrap();
  const [tab, setTab] = useState<Tab>('session');

  const settings = useAppStore((s) => s.settings);
  const profiles = useAppStore((s) => s.profiles);
  const loading = useAppStore((s) => s.loading);
  const saving = useAppStore((s) => s.saving);
  const appError = useAppStore((s) => s.error);

  const updateSettings = useAppStore((s) => s.updateSettings);
  const createProfile = useAppStore((s) => s.createProfile);
  const activateProfile = useAppStore((s) => s.activateProfile);
  const deleteProfile = useAppStore((s) => s.deleteProfile);
  const dismissAppError = useAppStore((s) => s.dismissError);

  const notice = useLibraryStore((s) => s.notice);
  const dismissNotice = useLibraryStore((s) => s.dismissNotice);
  const captureError = useCaptureStore((s) => s.error);
  const dismissCaptureError = useCaptureStore((s) => s.dismissError);
  const sessionError = useSessionStore((s) => s.error);
  const dismissSessionError = useSessionStore((s) => s.dismissError);
  const eventsError = useEventsStore((s) => s.error);
  const dismissEventsError = useEventsStore((s) => s.dismissError);

  return (
    <div className="app">
      <header className="app__header">
        <div>
          <h1>dndsound</h1>
          <p>Звуки за голосом — усе працює на цьому комп’ютері.</p>
        </div>
        <nav className="tabs">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              className={t.id === tab ? 'is-active' : ''}
              onClick={() => setTab(t.id)}
            >
              {t.label}
            </button>
          ))}
        </nav>
      </header>

      {appError ? (
        <ErrorBanner kind={appError.kind} message={appError.message} onDismiss={dismissAppError} />
      ) : null}
      {sessionError ? (
        <ErrorBanner
          kind={sessionError.kind}
          message={sessionError.message}
          onDismiss={dismissSessionError}
        />
      ) : null}
      {captureError ? (
        <ErrorBanner
          kind={captureError.kind}
          message={captureError.message}
          onDismiss={dismissCaptureError}
        />
      ) : null}
      {eventsError ? (
        <ErrorBanner
          kind={eventsError.kind}
          message={eventsError.message}
          onDismiss={dismissEventsError}
        />
      ) : null}
      {notice ? (
        <ErrorBanner
          kind={notice.kind}
          message={notice.message}
          tone={notice.tone}
          onDismiss={dismissNotice}
        />
      ) : null}

      {loading ? (
        <p className="app__loading">Завантаження…</p>
      ) : (
        <main>
          {tab === 'session' ? <SessionPage /> : null}
          {tab === 'events' ? <EventsPage /> : null}
          {tab === 'library' ? <LibraryPage /> : null}
          {tab === 'models' ? <SetupPage /> : null}
          {tab === 'settings' && settings ? (
            <div className="app__grid">
              <div className="app__column">
                <SettingsPanel settings={settings} saving={saving} onChange={updateSettings} />
              </div>
              <div className="app__column">
                <ProfilePanel
                  profiles={profiles}
                  onActivate={activateProfile}
                  onCreate={createProfile}
                  onDelete={deleteProfile}
                />
              </div>
            </div>
          ) : null}
        </main>
      )}
    </div>
  );
}
