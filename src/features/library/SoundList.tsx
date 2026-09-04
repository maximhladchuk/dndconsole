import type { Sound } from '../../types/api';

interface SoundListProps {
  sounds: Sound[];
  selectedGroupId: number | null;
  memberIds: Set<number>;
  onPreview: (id: number) => void;
  onToggleEnabled: (sound: Sound) => void;
  onToggleFavorite: (sound: Sound) => void;
  onAddToGroup: (soundId: number) => void;
  onRemoveFromGroup: (soundId: number) => void;
  onDelete: (sound: Sound) => void;
}

function duration(sound: Sound): string {
  if (sound.durationMs === null) return '—';
  const seconds = sound.durationMs / 1000;
  return seconds < 10 ? `${seconds.toFixed(1)}s` : `${Math.round(seconds)}s`;
}

export function SoundList({
  sounds,
  selectedGroupId,
  memberIds,
  onPreview,
  onToggleEnabled,
  onToggleFavorite,
  onAddToGroup,
  onRemoveFromGroup,
  onDelete,
}: SoundListProps) {
  if (sounds.length === 0) {
    return (
      <p className="empty">
        No sounds yet. Import files or a folder to get started — nothing is copied unless the
        managed library setting is on.
      </p>
    );
  }

  return (
    <ul className="sounds">
      {sounds.map((sound) => {
        const inGroup = memberIds.has(sound.id);
        return (
          <li key={sound.id} className={sound.missing ? 'sounds__row is-missing' : 'sounds__row'}>
            <button
              type="button"
              className="sounds__play"
              onClick={() => onPreview(sound.id)}
              disabled={sound.missing}
              aria-label={`Preview ${sound.displayName}`}
              title={sound.missing ? 'File is missing from disk' : 'Preview'}
            >
              ▶
            </button>

            <div className="sounds__name">
              <span className={sound.enabled ? '' : 'is-dim'}>{sound.displayName}</span>
              <span className="sounds__meta">
                {sound.format} · {duration(sound)}
                {sound.sampleRate ? ` · ${Math.round(sound.sampleRate / 1000)} kHz` : ''}
                {sound.managed ? ' · managed' : ''}
                {sound.missing ? ' · MISSING' : ''}
              </span>
            </div>

            <button
              type="button"
              className={sound.favorite ? 'icon is-on' : 'icon'}
              onClick={() => onToggleFavorite(sound)}
              aria-label={sound.favorite ? 'Remove from favorites' : 'Mark as favorite'}
            >
              ★
            </button>

            <button
              type="button"
              className={sound.enabled ? 'icon is-on' : 'icon'}
              onClick={() => onToggleEnabled(sound)}
              aria-label={sound.enabled ? 'Disable sound' : 'Enable sound'}
              title={sound.enabled ? 'Enabled' : 'Disabled — never selected'}
            >
              ◉
            </button>

            {selectedGroupId !== null ? (
              <button
                type="button"
                className="icon"
                onClick={() => (inGroup ? onRemoveFromGroup(sound.id) : onAddToGroup(sound.id))}
                aria-label={inGroup ? 'Remove from group' : 'Add to group'}
                title={inGroup ? 'Remove from the selected group' : 'Add to the selected group'}
              >
                {inGroup ? '−' : '+'}
              </button>
            ) : null}

            <button
              type="button"
              className="icon icon--danger"
              onClick={() => onDelete(sound)}
              aria-label={`Remove ${sound.displayName} from the library`}
              title="Remove from library (the file on disk is left alone)"
            >
              ×
            </button>
          </li>
        );
      })}
    </ul>
  );
}
