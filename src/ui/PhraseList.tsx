import { useState } from 'react';

import { Hint } from './Hint';

import type { Phrase, PhraseLang } from '../types/api';

/**
 * Example phrases, which carry a language alongside the text.
 *
 * Same reasoning as [`ChipList`]: the previous editor encoded the language as an
 * `en:` prefix inside a textarea, which meant a typo in the prefix silently produced a
 * phrase in the wrong language with the prefix still attached to the text.
 */
interface PhraseListProps {
  label: string;
  hint?: string;
  /** Longer explanation, behind a "?" next to the label. */
  explain?: React.ReactNode;
  phrases: Phrase[];
  onChange: (phrases: Phrase[]) => void;
  placeholder?: string;
}

const LANGS: { value: PhraseLang; label: string }[] = [
  { value: 'uk', label: 'uk' },
  { value: 'en', label: 'en' },
  { value: 'any', label: 'any' },
];

export function PhraseList({
  label,
  hint,
  explain,
  phrases,
  onChange,
  placeholder,
}: PhraseListProps) {
  const [draft, setDraft] = useState('');
  const [lang, setLang] = useState<PhraseLang>('uk');

  const add = () => {
    const text = draft.trim();
    if (!text) return;
    if (!phrases.some((p) => p.text === text && p.lang === lang)) {
      onChange([...phrases, { lang, text, isCommand: false }]);
    }
    setDraft('');
  };

  const remove = (index: number) => onChange(phrases.filter((_, i) => i !== index));

  return (
    <div className="field">
      <span className="field__label">
        <span className="field__label-text">
          {label}
          {explain ? <Hint label={label}>{explain}</Hint> : null}
        </span>
        <span className="field__value">{phrases.length}</span>
      </span>

      <ul className="chips">
        {phrases.map((phrase, index) => (
          <li className="chip" key={`${phrase.lang}-${phrase.text}-${index}`}>
            <span className="chip__badge">{phrase.lang}</span>
            <span className="chip__text">{phrase.text}</span>
            <button
              type="button"
              className="chip__remove"
              onClick={() => remove(index)}
              aria-label={`Прибрати ${phrase.text}`}
              title={`Прибрати ${phrase.text}`}
            >
              ×
            </button>
          </li>
        ))}
        {phrases.length === 0 ? <li className="chips__empty">Поки порожньо.</li> : null}
      </ul>

      <div className="chips__add">
        <select
          value={lang}
          aria-label="Мова нової фрази"
          onChange={(e) => setLang(e.target.value as PhraseLang)}
        >
          {LANGS.map((l) => (
            <option key={l.value} value={l.value}>
              {l.label}
            </option>
          ))}
        </select>
        <input
          type="text"
          value={draft}
          placeholder={placeholder}
          aria-label={`Додати до «${label}»`}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              add();
            }
          }}
        />
        <button type="button" onClick={add} disabled={!draft.trim()}>
          Додати
        </button>
      </div>

      {hint ? <span className="field__hint">{hint}</span> : null}
    </div>
  );
}
