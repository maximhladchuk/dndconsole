import { useEffect } from 'react';

import { useEventsStore } from '../../stores/eventsStore';
import { Panel } from '../../ui/Panel';

import { EventEditor } from './EventEditor';

export function EventsPage() {
  const store = useEventsStore();

  useEffect(() => {
    void store.load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const selected = store.events.find((event) => event.definition.id === store.selectedId) ?? null;

  return (
    <div className="library">
      <Panel title="Events" subtitle={`${store.events.length} defined`}>
        <div className="library__actions">
          <button type="button" onClick={() => void store.restoreDefaults()}>
            Restore defaults
          </button>
        </div>

        {store.loading ? (
          <p className="empty">Loading…</p>
        ) : (
          <ul className="sounds">
            {store.events.map(({ definition, soundGroupId }) => (
              <li
                key={definition.id}
                className={
                  definition.id === store.selectedId ? 'sounds__row is-selected' : 'sounds__row'
                }
              >
                <button
                  type="button"
                  className="sounds__name event-row"
                  onClick={() => store.select(definition.id)}
                >
                  <span className={definition.enabled ? '' : 'is-dim'}>{definition.displayName}</span>
                  <span className="sounds__meta">
                    {definition.category} · {definition.phrases.length} phrases ·{' '}
                    {soundGroupId === null ? 'no sound' : 'sound assigned'}
                  </span>
                </button>

                <button
                  type="button"
                  className={definition.enabled ? 'icon is-on' : 'icon'}
                  onClick={() => void store.setEnabled(definition.id, !definition.enabled)}
                  aria-label={definition.enabled ? 'Disable event' : 'Enable event'}
                >
                  ◉
                </button>
              </li>
            ))}
          </ul>
        )}
      </Panel>

      <Panel
        title="Event"
        subtitle={selected ? 'Changes apply immediately, even mid-session' : 'Select an event'}
      >
        {selected ? (
          <EventEditor
            event={selected}
            groups={store.groups}
            saving={store.saving}
            onSave={(definition, groupId, track) => void store.save(definition, groupId, track)}
            onDelete={() => void store.remove(selected.definition.id)}
            onReset={() => void store.reset(selected.definition.id)}
          />
        ) : (
          <p className="empty">No event selected.</p>
        )}
      </Panel>
    </div>
  );
}
