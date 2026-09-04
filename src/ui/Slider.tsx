interface SliderProps {
  label: string;
  value: number;
  min?: number;
  max?: number;
  step?: number;
  format?: (value: number) => string;
  onChange: (value: number) => void;
}

export function Slider({
  label,
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
        {label}
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
