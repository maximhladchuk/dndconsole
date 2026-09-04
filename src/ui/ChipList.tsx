import { useState } from 'react';

import { Hint } from './Hint';

/**
 * A list of short strings edited as removable chips.
 *
 * These lists are long — a single event carries eighty keywords — and a textarea of
 * newline-separated words hides that: you cannot see where one entry ends, a stray
 * space is invisible, and deleting the wrong line is one keystroke. Chips make each
 * entry a thing you can see and remove on its own.
 *
 * Adding is deliberately explicit: Enter or the button. Blur does not commit, because a
 * half-typed word left in the box when you click Save should not become a keyword.
 */
interface ChipListProps {
  label: string;
  hint?: string;
  /** Longer explanation, behind a "?" next to the label. */
  explain?: React.ReactNode;
  items: string[];
  onChange: (items: string[]) => void;
  placeholder?: string;
  /** Rendered before the text inside each chip, e.g. a language badge. */
  badge?: (item: string, index: number) => string | null;
}

export function ChipList({
  label,
  hint,
  explain,
  items,
  onChange,
  placeholder,
  badge,
}: ChipListProps) {
  const [draft, setDraft] = useState('');

  const add = () => {
    const text = draft.trim();
    if (!text) return;
    // Silently ignoring a duplicate is confusing; clearing the box says it landed.
    if (!items.includes(text)) onChange([...items, text]);
    setDraft('');
  };

  const remove = (index: number) => onChange(items.filter((_, i) => i !== index));

  return (
    <div className="field">
      <span className="field__label">
        <span className="field__label-text">
          {label}
          {explain ? <Hint label={label}>{explain}</Hint> : null}
        </span>
        <span className="field__value">{items.length}</span>
      </span>

      <ul className="chips">
        {items.map((item, index) => {
          const mark = badge?.(item, index);
          return (
            <li className="chip" key={`${item}-${index}`}>
              {mark ? <span className="chip__badge">{mark}</span> : null}
              <span className="chip__text">{item}</span>
              <button
                type="button"
                className="chip__remove"
                onClick={() => remove(index)}
                aria-label={`Прибрати ${item}`}
                title={`Прибрати ${item}`}
              >
                ×
              </button>
            </li>
          );
        })}
        {items.length === 0 ? <li className="chips__empty">Поки порожньо.</li> : null}
      </ul>

      <div className="chips__add">
        <input
          type="text"
          value={draft}
          placeholder={placeholder}
          aria-label={`Додати до «${label}»`}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              // Otherwise the surrounding form — and the Save button — takes the Enter.
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
