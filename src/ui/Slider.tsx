import { Hint } from './Hint';

interface SliderProps {
  label: string;
  /** Longer explanation, behind a "?" next to the label. */
  explain?: React.ReactNode;
  value: number;
  min?: number;
  max?: number;
  step?: number;
  format?: (value: number) => string;
  onChange: (value: number) => void;
}

export function Slider({
  label,
  explain,
  value,
  min = 0,
  max = 1,
  step = 0.01,
  format = (v) => `${Math.round(v * 100)}%`,
  onChange,
}: SliderProps) {
  return (
    <label className="field field--slider">
      <span className="field__label">
        <span className="field__label-text">
          {label}
          {explain ? <Hint label={label}>{explain}</Hint> : null}
        </span>
        <span className="field__value">{format(value)}</span>
      </span>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
      />
    </label>
  );
}
