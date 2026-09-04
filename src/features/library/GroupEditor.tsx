import { useEffect, useState } from 'react';

import { Slider } from '../../ui/Slider';
import { Toggle } from '../../ui/Toggle';
import type { SelectionMode, Sound, SoundGroup } from '../../types/api';

interface GroupEditorProps {
  group: SoundGroup;
  members: Sound[];
  onUpdate: (
    name: string,
    selectionMode: SelectionMode,
    preventRepeat: boolean,
    volume: number,
  ) => void;
  onPlay: () => void;
  onDelete: () => void;
}

const MODES: { value: SelectionMode; label: string; hint: string }[] = [
  { value: 'random', label: 'Random', hint: 'Every enabled sound equally likely.' },
  { value: 'weighted', label: 'Weighted', hint: 'Likelihood follows each sound’s weight.' },
  { value: 'sequential', label: 'Sequential', hint: 'Cycle through the group in order.' },
];

export function GroupEditor({ group, members, onUpdate, onPlay, onDelete }: GroupEditorProps) {
  const [name, setName] = useState(group.name);

  useEffect(() => setName(group.name), [group.id, group.name]);

  const playable = members.filter((m) => m.enabled && !m.missing).length;
  const mode = MODES.find((m) => m.value === group.selectionMode) ?? MODES[0];

  return (
    <div className="group-editor">
      <div className="group-editor__row">
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onBlur={() => {
            const trimmed = name.trim();
            if (trimmed && trimmed !== group.name) {
              onUpdate(trimmed, group.selectionMode, group.preventRepeat, group.volume);
            } else {
              setName(group.name);
            }
          }}
          aria-label="Group name"
        />
        <button type="button" onClick={onPlay} disabled={playable === 0}>
          Play
        </button>
        <button type="button" className="icon icon--danger" onClick={onDelete} aria-label="Delete group">
          ×
        </button>
      </div>

      <p className="group-editor__count">
        {members.length} sound{members.length === 1 ? '' : 's'}
        {playable !== members.length ? ` · ${playable} playable` : ''}
      </p>

      <label className="field">
        <span className="field__label">Selection</span>
        <select
          value={group.selectionMode}
          onChange={(e) =>
            onUpdate(group.name, e.target.value as SelectionMode, group.preventRepeat, group.volume)
          }
        >
          {MODES.map((m) => (
            <option key={m.value} value={m.value}>
              {m.label}
            </option>
          ))}
        </select>
        <span className="field__hint">{mode.hint}</span>
      </label>

      <Toggle
        label="Prevent immediate repetition"
        hint="Never play the same file twice in a row when the group has alternatives."
        checked={group.preventRepeat}
        onChange={(v) => onUpdate(group.name, group.selectionMode, v, group.volume)}
      />

      <Slider
        label="Group volume"
        value={group.volume}
        onChange={(v) => onUpdate(group.name, group.selectionMode, group.preventRepeat, v)}
      />

      {members.length > 0 ? (
        <ol className="group-editor__members">
          {members.map((m) => (
            <li key={m.id} className={m.enabled && !m.missing ? '' : 'is-dim'}>
              {m.displayName}
              {m.missing ? ' — missing' : ''}
            </li>
          ))}
        </ol>
      ) : (
        <p className="empty">Add sounds from the list on the left with the + button.</p>
      )}
    </div>
  );
}
