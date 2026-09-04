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
      <Panel title="Події" subtitle={`${store.events.length} усього`}>
        <div className="library__actions">
          <button type="button" onClick={() => void store.restoreDefaults()}>
            Повернути типові
          </button>
        </div>

        {store.loading ? (
          <p className="empty">Завантаження…</p>
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
                    {definition.category} · {definition.phrases.length} фраз ·{' '}
                    {soundGroupId === null ? 'без звуку' : 'звук призначено'}
                  </span>
                </button>

                <button
                  type="button"
                  className={definition.enabled ? 'icon is-on' : 'icon'}
                  onClick={() => void store.setEnabled(definition.id, !definition.enabled)}
                  aria-label={definition.enabled ? 'Вимкнути подію' : 'Увімкнути подію'}
                >
                  ◉
                </button>
              </li>
            ))}
          </ul>
        )}
      </Panel>

      <Panel
        title="Подія"
        subtitle={
          selected ? 'Зміни діють одразу, навіть під час сесії' : 'Обери подію зі списку'
        }
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
          <p className="empty">Подію не обрано.</p>
        )}
      </Panel>
    </div>
  );
}
