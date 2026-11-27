import { Pause, Play, Square } from "lucide-react";
import { useEffect, useState } from "react";
import { shortcutLabel } from "@/hooks/useShortcuts";
import { formatClock } from "@/lib/format";
import { useApp } from "@/store/app";
import { useSession } from "@/store/session";
import { Button } from "./ui/button";
import { Tooltip } from "./ui/tooltip";

interface SessionControlsProps {
  onToggle: () => void;
  onPauseToggle: () => void;
  canStart: boolean;
}

function useElapsed(startedAt: number | null): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (startedAt === null) return;
    const timer = window.setInterval(() => setNow(Date.now()), 500);
    return () => window.clearInterval(timer);
  }, [startedAt]);
  return startedAt === null ? 0 : now - startedAt;
}

export function SessionControls({ onToggle, onPauseToggle, canStart }: SessionControlsProps) {
  const platform = useApp((s) => s.info?.platform);
  const state = useSession((s) => s.state.state);
  const startedAt = useSession((s) => s.startedAt);
  const elapsed = useElapsed(startedAt);

  const live = state === "running" || state === "paused";
  const busy = state === "starting" || state === "stopping";

  if (!live) {
    return (
      <Tooltip label={`Start listening (${shortcutLabel("Enter", platform)})`}>
        <Button variant="record" size="sm" onClick={onToggle} disabled={!canStart || busy} aria-label="Start listening">
          <span className="size-2 rounded-full bg-white" aria-hidden />
          {state === "starting" ? "Starting…" : "Listen"}
        </Button>
      </Tooltip>
    );
  }

  return (
    <div className="flex items-center gap-1">
      <span className="flex items-center gap-1.5 pr-1 text-[12px] tabular-nums text-ink" aria-live="polite">
        <span
          className={state === "running" ? "recording-dot size-2 rounded-full bg-record" : "size-2 rounded-full bg-warn"}
          aria-hidden
        />
        <span className="sr-only">{state === "running" ? "Recording" : "Paused"}</span>
        {formatClock(elapsed)}
      </span>
      <Tooltip label={`${state === "paused" ? "Resume" : "Pause"} (${shortcutLabel("P", platform)})`}>
        <Button size="icon" onClick={onPauseToggle} aria-label={state === "paused" ? "Resume" : "Pause"}>
          {state === "paused" ? <Play className="size-3.5" /> : <Pause className="size-3.5" />}
        </Button>
      </Tooltip>
      <Tooltip label={`Stop (${shortcutLabel("Enter", platform)})`}>
        <Button size="icon" onClick={onToggle} aria-label="Stop" disabled={busy}>
          <Square className="size-3.5 fill-current" />
        </Button>
      </Tooltip>
    </div>
  );
}
