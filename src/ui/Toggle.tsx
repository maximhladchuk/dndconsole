import { Hint } from './Hint';

interface ToggleProps {
  label: string;
  hint?: string;
  /** Longer explanation, behind a "?" next to the label. */
  explain?: React.ReactNode;
  checked: boolean;
  onChange: (checked: boolean) => void;
}

export function Toggle({ label, hint, explain, checked, onChange }: ToggleProps) {
  return (
    <label className="field field--toggle">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span>
        <span className="field__label">
          <span className="field__label-text">
            {label}
            {explain ? <Hint label={label}>{explain}</Hint> : null}
          </span>
        </span>
        {hint ? <span className="field__hint">{hint}</span> : null}
      </span>
    </label>
  );
}
