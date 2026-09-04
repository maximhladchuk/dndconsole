import { useEffect, useState } from 'react';

import { ChipList } from '../../ui/ChipList';
import { PhraseList } from '../../ui/PhraseList';
import { Slider } from '../../ui/Slider';
import { Toggle } from '../../ui/Toggle';
import type {
  EventDefinition,
  EventTerm,
  Phrase,
  PhraseLang,
  SoundGroup,
  StoredEvent,
  TermKind,
} from '../../types/api';

interface EventEditorProps {
  event: StoredEvent;
  groups: SoundGroup[];
  saving: boolean;
  onSave: (definition: EventDefinition, soundGroupId: number | null, track: string) => void;
  onDelete: () => void;
  onReset: () => void;
}

/**
 * Phrases and terms are edited as removable chips rather than as newline-separated text.
 * The lists run to eighty entries; a textarea makes each one indistinguishable from its
 * neighbours and every edit a whole-list edit.
 */
function examplesOf(phrases: Phrase[]): Phrase[] {
  return phrases.filter((phrase) => !phrase.isCommand);
}

function commandsOf(phrases: Phrase[]): string[] {
  return phrases.filter((phrase) => phrase.isCommand).map((phrase) => phrase.text);
}

function termsOf(terms: EventTerm[], kind: TermKind): string[] {
  return terms.filter((term) => term.kind === kind).map((term) => term.text);
}

function toTerms(texts: string[], kind: TermKind): EventTerm[] {
  return texts.map((text) => ({ kind, lang: 'any' as PhraseLang, text }));
}

export function EventEditor({
  event,
  groups,
  saving,
  onSave,
  onDelete,
  onReset,
}: EventEditorProps) {
  const [draft, setDraft] = useState(event.definition);
  const [groupId, setGroupId] = useState<number | null>(event.soundGroupId);
  const [examples, setExamples] = useState(() => examplesOf(event.definition.phrases));
  const [commands, setCommands] = useState(() => commandsOf(event.definition.phrases));
  const [keywords, setKeywords] = useState(() => termsOf(event.definition.terms, 'keyword'));
  const [actions, setActions] = useState(() => termsOf(event.definition.terms, 'action'));
  const [negatives, setNegatives] = useState(() => termsOf(event.definition.terms, 'negative'));

  useEffect(() => {
    setDraft(event.definition);
    setGroupId(event.soundGroupId);
    setExamples(examplesOf(event.definition.phrases));
    setCommands(commandsOf(event.definition.phrases));
    setKeywords(termsOf(event.definition.terms, 'keyword'));
    setActions(termsOf(event.definition.terms, 'action'));
    setNegatives(termsOf(event.definition.terms, 'negative'));
  }, [event]);

  const save = () => {
    onSave(
      {
        ...draft,
        phrases: [
          ...examples,
          ...commands.map((text) => ({ lang: 'any' as PhraseLang, text, isCommand: true })),
        ],
        terms: [
          ...toTerms(keywords, 'keyword'),
          ...toTerms(actions, 'action'),
          ...toTerms(negatives, 'negative'),
        ],
      },
      groupId,
      event.track,
    );
  };

  return (
    <div className="event-editor">
      <div className="group-editor__row">
        <input
          type="text"
          value={draft.displayName}
          onChange={(e) => setDraft({ ...draft, displayName: e.target.value })}
          aria-label="Event name"
        />
        <button type="button" onClick={save} disabled={saving}>
          {saving ? 'Saving…' : 'Save'}
        </button>
        {event.builtin && event.userModified ? (
          <button type="button" onClick={onReset} disabled={saving}>
            Reset to default
          </button>
        ) : null}
        <button type="button" className="icon icon--danger" onClick={onDelete} aria-label="Delete event">
          ×
        </button>
      </div>
      <p className="group-editor__count">
        <code className="field__mono">{draft.id}</code> · {draft.category || 'uncategorised'}
        {event.builtin
          ? event.userModified
            ? ' · edited — built-in improvements no longer apply'
            : ' · built-in, kept up to date automatically'
          : ' · your own event'}
      </p>

      <label className="field">
        <span className="field__label">Sound group</span>
        <select
          value={groupId ?? ''}
          onChange={(e) => setGroupId(e.target.value ? Number(e.target.value) : null)}
        >
          <option value="">— none —</option>
          {groups.map((group) => (
            <option key={group.id} value={group.id}>
              {group.name}
            </option>
          ))}
        </select>
        <span className="field__hint">
          {groupId === null ? 'This event will detect but play nothing.' : ''}
        </span>
      </label>

      <PhraseList
        label="Example phrases"
        phrases={examples}
        onChange={setExamples}
        placeholder="відчиняє двері"
        hint="Whole sentences the way they get said. These are matched as phrases, so they fire even when no single keyword does."
      />

      <ChipList
        label="Keywords"
        items={keywords}
        onChange={setKeywords}
        placeholder="двері"
        hint="The object: door, sword, wolf. Every case form of a word that changes its stem needs its own entry — двері, дверей, дверима."
      />

      <ChipList
        label="Action words"
        items={actions}
        onChange={setActions}
        placeholder="відчиняє"
        hint="What has to be happening. Without one of these, a bare keyword will not fire."
      />

      <ChipList
        label="Negative phrases"
        items={negatives}
        onChange={setNegatives}
        placeholder="двері зачинені"
        hint="If any of these appear, the event is suppressed."
      />

      <ChipList
        label="Spoken commands"
        items={commands}
        onChange={setCommands}
        placeholder="sound door"
        hint="Said deliberately, and always wins over automatic detection."
      />

      <Slider
        label="Confidence threshold"
        value={draft.confidenceThreshold}
        min={0.3}
        max={0.99}
        onChange={(v) => setDraft({ ...draft, confidenceThreshold: v })}
        format={(v) => v.toFixed(2)}
      />
      <Slider
        label="Probability"
        value={draft.probability}
        onChange={(v) => setDraft({ ...draft, probability: v })}
      />
      <Slider
        label="Cooldown"
        value={draft.cooldownMs}
        min={0}
        max={30_000}
        step={100}
        format={(v) => `${(v / 1000).toFixed(1)} s`}
        onChange={(v) => setDraft({ ...draft, cooldownMs: Math.round(v) })}
      />

      <Toggle
        label="Require an action word"
        hint="Stops “a sword lies on the table” from triggering a sword swing."
        checked={draft.requireActionWord}
        onChange={(v) => setDraft({ ...draft, requireActionWord: v })}
      />
      <Toggle
        label="Enabled"
        checked={draft.enabled}
        onChange={(v) => setDraft({ ...draft, enabled: v })}
      />
    </div>
  );
}
