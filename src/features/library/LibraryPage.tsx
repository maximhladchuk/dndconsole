import { useEffect } from 'react';

import { useLibraryStore } from '../../stores/libraryStore';
import { Panel } from '../../ui/Panel';

import { SoundPackPanel } from './SoundPackPanel';

/**
 * What the application will play, grouped by theme.
 *
 * Deliberately read-only apart from previewing and muting: the sounds come from the
 * bundled pack, not from files the user manages. An event points at a group, and the
 * group picks one of its sounds — so this screen is about hearing what a group contains,
 * not about curating a library.
 */
export function LibraryPage() {
  const store = useLibraryStore();

  useEffect(() => {
    void store.load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const selected = store.groups.find((g) => g.id === store.selectedGroupId) ?? null;
  const members = store.selectedGroupId !== null ? (store.members[store.selectedGroupId] ?? []) : [];

  return (
    <div className="app__grid">
      <div className="app__column">
        <SoundPackPanel />

        <Panel title="Групи" subtitle="Подія грає з групи, а не з одного файлу">
          {store.loading ? (
            <p className="empty">Завантаження…</p>
          ) : store.groups.length === 0 ? (
            <p className="empty">Завантаж набір звуків, щоб тут щось з’явилося.</p>
          ) : (
            <ul className="picker">
              {store.groups.map((group) => (
                <li key={group.id}>
                  <button
                    type="button"
                    className={
                      group.id === store.selectedGroupId
                        ? 'picker__item is-selected'
                        : 'picker__item'
                    }
                    onClick={() => void store.selectGroup(group.id)}
                  >
                    <span>{group.name}</span>
                    <span className="picker__count">{store.counts[group.id] ?? ''}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </Panel>
      </div>

      <div className="app__column">
        <Panel
          title={selected ? selected.name : 'Звуки'}
          subtitle={
            selected
              ? `${members.length} звуків · один обирається випадково, минулий не повторюється`
              : 'Обери групу, щоб послухати, що в ній'
          }
        >
          {selected === null ? (
            <p className="empty">Нічого не обрано.</p>
          ) : members.length === 0 ? (
            <p className="empty">Ця група порожня.</p>
          ) : (
            <ul className="sound-list">
              {members.map((sound) => (
                <li key={sound.id} className={sound.enabled ? 'sound-row' : 'sound-row is-muted'}>
                  <span className="sound-row__name">
                    {sound.displayName}
                    <span className="sound-row__meta">
                      {sound.durationMs ? `${(sound.durationMs / 1000).toFixed(1)} с` : '—'} ·{' '}
                      {sound.provenance.license || 'локальний'}
                      {sound.provenance.author ? ` · ${sound.provenance.author}` : ''}
                      {sound.missing ? ' · файл відсутній' : ''}
                    </span>
                  </span>
                  <span className="sound-row__actions">
                    <button type="button" onClick={() => void store.preview(sound.id)}>
                      Слухати
                    </button>
                    <button
                      type="button"
                      onClick={() => void store.setSoundEnabled(sound.id, !sound.enabled)}
                    >
                      {sound.enabled ? 'Вимкнути' : 'Увімкнути'}
                    </button>
                  </span>
                </li>
              ))}
            </ul>
          )}

          {store.lastPlayed ? (
            <p className="library__now">Грає: {store.lastPlayed.displayName}</p>
          ) : null}
        </Panel>
      </div>
    </div>
  );
}
