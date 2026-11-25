import { levelToUnit } from "@/lib/format";
import { useSession } from "@/store/session";

/** Compact input meter; reads only the level slice so 20 Hz updates stay cheap. */
export function LevelMeter() {
  const level = useSession((s) => s.level);
  const rms = levelToUnit(level.rmsDb);
  const peak = levelToUnit(level.peakDb);

  return (
    <div
      role="meter"
      aria-label="Input level"
      aria-valuemin={-60}
      aria-valuemax={0}
      aria-valuenow={Math.round(Math.max(-60, level.rmsDb))}
      className="relative h-1.5 w-14 overflow-hidden rounded-full bg-surface-raised"
    >
      <div className="absolute inset-y-0 left-0 rounded-full bg-accent/80" style={{ width: `${rms * 100}%` }} />
      <div className="absolute inset-y-0 w-px bg-ink/60" style={{ left: `${peak * 100}%` }} />
    </div>
  );
}
