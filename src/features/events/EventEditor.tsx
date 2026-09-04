import { useEffect, useState } from 'react';

import { ChipList } from '../../ui/ChipList';
import { Hint } from '../../ui/Hint';
import { PhraseList } from '../../ui/PhraseList';
import { Slider } from '../../ui/Slider';
import { Toggle } from '../../ui/Toggle';
import {
  ACTIONS_HINT,
  COMMANDS_HINT,
  COOLDOWN_HINT,
  HOW_MATCHING_WORKS,
  KEYWORDS_HINT,
  NEGATIVES_HINT,
  PHRASES_HINT,
  PROBABILITY_HINT,
  REQUIRE_ACTION_HINT,
  THRESHOLD_HINT,
} from './hints';
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
          aria-label="Назва події"
        />
        <button type="button" onClick={save} disabled={saving}>
          {saving ? 'Зберігаю…' : 'Зберегти'}
        </button>
        {event.builtin && event.userModified ? (
          <button type="button" onClick={onReset} disabled={saving}>
            Повернути типове
          </button>
        ) : null}
        <button
          type="button"
          className="icon icon--danger"
          onClick={onDelete}
          aria-label="Видалити подію"
        >
          ×
        </button>
      </div>
      <p className="group-editor__count">
        <code className="field__mono">{draft.id}</code> · {draft.category || 'uncategorised'}
        {event.builtin
          ? event.userModified
            ? ' · змінено вручну — оновлення вбудованих фраз більше не застосовуються'
            : ' · вбудована, оновлюється автоматично'
          : ' · твоя власна подія'}
      </p>

      <p className="event-editor__how">
        Як це працює
        <Hint label="як працює розпізнавання">{HOW_MATCHING_WORKS}</Hint>
      </p>

      <label className="field">
        <span className="field__label">Група звуків</span>
        <select
          value={groupId ?? ''}
          onChange={(e) => setGroupId(e.target.value ? Number(e.target.value) : null)}
        >
          <option value="">— немає —</option>
          {groups.map((group) => (
            <option key={group.id} value={group.id}>
              {group.name}
            </option>
          ))}
        </select>
        <span className="field__hint">
          {groupId === null ? 'Подія розпізнаватиметься, але нічого не гратиме.' : ''}
        </span>
      </label>

      <ChipList
        label="Ключові слова"
        items={keywords}
        onChange={setKeywords}
        placeholder="двері"
        explain={KEYWORDS_HINT}
        hint="Об’єкт: двері, меч, вовк."
      />

      <ChipList
        label="Слова дії"
        items={actions}
        onChange={setActions}
        placeholder="відчиняє"
        explain={ACTIONS_HINT}
        hint="Що з об’єктом відбувається. Без цього саме ключове слово не спрацює."
      />

      <PhraseList
        label="Приклади фраз"
        phrases={examples}
        onChange={setExamples}
        placeholder="відчиняє двері"
        explain={PHRASES_HINT}
        hint="Цілі речення так, як ти їх кажеш."
      />

      <ChipList
        label="Заперечення"
        items={negatives}
        onChange={setNegatives}
        placeholder="двері зачинені"
        explain={NEGATIVES_HINT}
        hint="Якщо збіглося — подія не грає."
      />

      <ChipList
        label="Голосові команди"
        items={commands}
        onChange={setCommands}
        placeholder="sound door"
        explain={COMMANDS_HINT}
        hint="Сказане навмисне; завжди виграє в автоматичного розпізнавання."
      />

      <Slider
        label="Поріг упевненості"
        explain={THRESHOLD_HINT}
        value={draft.confidenceThreshold}
        min={0.3}
        max={0.99}
        onChange={(v) => setDraft({ ...draft, confidenceThreshold: v })}
        format={(v) => v.toFixed(2)}
      />
      <Slider
        label="Ймовірність"
        explain={PROBABILITY_HINT}
        value={draft.probability}
        onChange={(v) => setDraft({ ...draft, probability: v })}
      />
      <Slider
        label="Затримка між спрацюваннями"
        explain={COOLDOWN_HINT}
        value={draft.cooldownMs}
        min={0}
        max={30_000}
        step={100}
        format={(v) => `${(v / 1000).toFixed(1)} s`}
        onChange={(v) => setDraft({ ...draft, cooldownMs: Math.round(v) })}
      />

      <Toggle
        label="Вимагати слово дії"
        explain={REQUIRE_ACTION_HINT}
        hint="Не дає «Меч лежить на столі» зіграти удар мечем."
        checked={draft.requireActionWord}
        onChange={(v) => setDraft({ ...draft, requireActionWord: v })}
      />
      <Toggle
        label="Увімкнено"
        checked={draft.enabled}
        onChange={(v) => setDraft({ ...draft, enabled: v })}
      />
    </div>
  );
}
