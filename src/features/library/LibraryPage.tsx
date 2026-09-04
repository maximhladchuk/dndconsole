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

        <Panel title="Groups" subtitle="An event plays from a group, never from one file">
          {store.loading ? (
            <p className="empty">Loading…</p>
          ) : store.groups.length === 0 ? (
            <p className="empty">Download the sound pack to fill these in.</p>
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
                    <span className="picker__count">{store.members[group.id]?.length ?? ''}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </Panel>
      </div>

      <div className="app__column">
        <Panel
          title={selected ? selected.name : 'Sounds'}
          subtitle={
            selected
              ? `${members.length} sounds · one is picked at random, avoiding the last`
              : 'Pick a group to hear what is in it'
          }
        >
          {selected === null ? (
            <p className="empty">Nothing selected.</p>
          ) : members.length === 0 ? (
            <p className="empty">This group is empty.</p>
          ) : (
            <ul className="sound-list">
              {members.map((sound) => (
                <li key={sound.id} className={sound.enabled ? 'sound-row' : 'sound-row is-muted'}>
                  <span className="sound-row__name">
                    {sound.displayName}
                    <span className="sound-row__meta">
                      {sound.durationMs ? `${(sound.durationMs / 1000).toFixed(1)} s` : '—'} ·{' '}
                      {sound.provenance.license || 'local'}
                      {sound.provenance.author ? ` · ${sound.provenance.author}` : ''}
                      {sound.missing ? ' · file missing' : ''}
                    </span>
                  </span>
                  <span className="sound-row__actions">
                    <button type="button" onClick={() => void store.preview(sound.id)}>
                      Play
                    </button>
                    <button
                      type="button"
                      onClick={() => void store.setSoundEnabled(sound.id, !sound.enabled)}
                    >
                      {sound.enabled ? 'Mute' : 'Unmute'}
                    </button>
                  </span>
                </li>
              ))}
            </ul>
          )}

          {store.lastPlayed ? (
            <p className="library__now">Playing: {store.lastPlayed.displayName}</p>
          ) : null}
        </Panel>
      </div>
    </div>
  );
}
