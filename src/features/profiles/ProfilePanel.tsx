import { useState } from 'react';

import { Panel } from '../../ui/Panel';
import type { Profile } from '../../types/api';

interface ProfilePanelProps {
  profiles: Profile[];
  onActivate: (id: number) => void;
  onCreate: (name: string, description: string) => void;
  onDelete: (id: number) => void;
}

export function ProfilePanel({ profiles, onActivate, onCreate, onDelete }: ProfilePanelProps) {
  const [name, setName] = useState('');

  const submit = () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    onCreate(trimmed, '');
    setName('');
  };

  return (
    <Panel title="Campaigns" subtitle="Each campaign scopes its own events and tuning">
      <ul className="profiles">
        {profiles.map((profile) => (
          <li key={profile.id} className={profile.isActive ? 'profiles__item is-active' : 'profiles__item'}>
            <button type="button" onClick={() => onActivate(profile.id)} disabled={profile.isActive}>
              <span className="profiles__dot" aria-hidden="true" />
              {profile.name}
            </button>
            <button
              type="button"
              className="profiles__delete"
              onClick={() => onDelete(profile.id)}
              aria-label={`Delete ${profile.name}`}
            >
              ×
            </button>
          </li>
        ))}
      </ul>

      <div className="profiles__new">
        <input
          type="text"
          value={name}
          placeholder="New campaign name"
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') submit();
          }}
        />
        <button type="button" onClick={submit} disabled={!name.trim()}>
          Add
        </button>
      </div>
    </Panel>
  );
}
