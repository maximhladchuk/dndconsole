interface LevelMeterProps {
  /** Linear amplitude, 0..1. */
  level: number;
  active: boolean;
  floorDb?: number;
}

/**
 * Maps amplitude onto a decibel scale before drawing, because a linear bar spends
 * almost all its width on levels nobody can hear.
 *
 * Mirrors `Level::bar` in `crates/pipeline/src/level.rs`.
 */
function bar(level: number, floorDb: number): number {
  if (!Number.isFinite(level) || level <= 0) return 0;
  const db = 20 * Math.log10(level);
  return Math.min(1, Math.max(0, (db - floorDb) / -floorDb));
}

export function LevelMeter({ level, active, floorDb = -60 }: LevelMeterProps) {
  const width = active ? bar(level, floorDb) : 0;

  return (
    <div className="meter" role="meter" aria-valuenow={Math.round(width * 100)} aria-label="Рівень входу">
      <div className="meter__fill" style={{ width: `${width * 100}%` }} />
    </div>
  );
}
