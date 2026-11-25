import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Pin, PinOff, Settings, Sparkles, X } from "lucide-react";
import { shortcutLabel } from "@/hooks/useShortcuts";
import { languageName } from "@/lib/languages";
import { cn } from "@/lib/cn";
import { useApp } from "@/store/app";
import { isActive, useSession } from "@/store/session";
import { LevelMeter } from "./LevelMeter";
import { SessionControls } from "./SessionControls";
import { SourcePicker } from "./SourcePicker";
import { Button } from "./ui/button";
import { Tooltip } from "./ui/tooltip";

interface TitleBarProps {
  canStart: boolean;
  copilotOpen: boolean;
  onToggleSession: () => void;
  onTogglePause: () => void;
  onToggleCopilot: () => void;
  onToggleAlwaysOnTop: () => void;
  onOpenSettings: () => void;
}

export function TitleBar(props: TitleBarProps) {
  const state = useSession((s) => s.state);
  const detected = useSession((s) => s.detectedLanguage);
  const settings = useApp((s) => s.settings);
  const platform = useApp((s) => s.info?.platform);
  const window = getCurrentWindow();

  const spoken = settings?.sourceLanguage ?? detected;
  const pinned = settings?.alwaysOnTop ?? true;

  return (
    <header data-tauri-drag-region className="glass flex h-11 shrink-0 items-center gap-1 rounded-xl px-2">
      <SessionControls canStart={props.canStart} onToggle={props.onToggleSession} onPauseToggle={props.onTogglePause} />
      <SourcePicker disabled={isActive(state)} />
      <LevelMeter />

      <button
        type="button"
        onClick={props.onOpenSettings}
        className="ml-1 rounded-md px-1.5 py-0.5 text-[12px] text-ink-muted hover:bg-surface-raised hover:text-ink"
        aria-label="Change languages"
      >
        {settings?.sourceLanguage ? languageName(spoken) : `Auto${detected ? ` · ${languageName(detected)}` : ""}`}
        <span className="px-1 text-ink-faint" aria-hidden>
          →
        </span>
        {languageName(settings?.targetLanguage)}
      </button>

      <div data-tauri-drag-region className="h-full flex-1" />

      <Tooltip label={`Copilot (${shortcutLabel("J", platform)})`}>
        <Button
          size="icon"
          onClick={props.onToggleCopilot}
          aria-pressed={props.copilotOpen}
          aria-label="Toggle copilot"
          className={cn(props.copilotOpen && "text-accent")}
        >
          <Sparkles className="size-3.5" />
        </Button>
      </Tooltip>
      <Tooltip label={`${pinned ? "Unpin from top" : "Keep on top"} (${shortcutLabel("T", platform)})`}>
        <Button size="icon" onClick={props.onToggleAlwaysOnTop} aria-label="Toggle always on top" aria-pressed={pinned}>
          {pinned ? <Pin className="size-3.5" /> : <PinOff className="size-3.5" />}
        </Button>
      </Tooltip>
      <Tooltip label={`Settings (${shortcutLabel(",", platform)})`}>
        <Button size="icon" onClick={props.onOpenSettings} aria-label="Settings">
          <Settings className="size-3.5" />
        </Button>
      </Tooltip>
      <div className="mx-1 h-4 w-px bg-line" aria-hidden />
      <Button size="icon" onClick={() => void window.minimize()} aria-label="Minimize">
        <Minus className="size-3.5" />
      </Button>
      <Button size="icon" onClick={() => void window.close()} aria-label="Close" className="hover:bg-record hover:text-white">
        <X className="size-3.5" />
      </Button>
    </header>
  );
}
