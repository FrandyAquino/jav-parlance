import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useMemo, useState } from "react";
import { CopilotPanel } from "./components/CopilotPanel";
import { EmptyState } from "./components/EmptyState";
import { Notices } from "./components/Notices";
import { PinnedReply } from "./components/PinnedReply";
import { SettingsDialog } from "./components/SettingsDialog";
import { TitleBar } from "./components/TitleBar";
import { TranscriptView } from "./components/TranscriptView";
import { NoiseTexture } from "./components/ui/noise-texture";
import { TooltipProvider } from "./components/ui/tooltip";
import { useShortcuts } from "./hooks/useShortcuts";
import { useTheme } from "./hooks/useTheme";
import { useWindowMode } from "./hooks/useWindowMode";
import { errorMessage, ipc } from "./lib/ipc";
import { missingRequiredModels, useApp } from "./store/app";
import { dispatchSessionEvent, isActive, useSession } from "./store/session";
import { useCopilot } from "./store/copilot";
import { useProfiles } from "./store/profiles";
import { Button } from "./components/ui/button";

export function App() {
  const load = useApp((s) => s.load);
  const info = useApp((s) => s.info);
  const settings = useApp((s) => s.settings);
  const models = useApp((s) => s.models);
  const sourceId = useApp((s) => s.selectedSourceId);
  const microphoneId = useApp((s) => s.microphoneId);
  const saveSettings = useApp((s) => s.saveSettings);
  const appError = useApp((s) => s.error);
  const state = useSession((s) => s.state);

  const [settingsOpen, setSettingsOpen] = useState(false);
  const mode = useCopilot((s) => s.mode);
  const pinnedReply = useCopilot((s) => s.pinned);
  const [actionError, setActionError] = useState<string | null>(null);
  const resizeError = useWindowMode(mode);

  useTheme(settings?.theme, info?.platform);

  useEffect(() => {
    void ipc.subscribe(dispatchSessionEvent);
    void load().then(() => {
      if (!useCopilot.getState().question)
        useCopilot.setState({
          replyLanguage: useApp.getState().settings?.sourceLanguage ?? "en",
        });
    });
    void useProfiles.getState().load();
  }, [load]);

  const run = useCallback(async (action: () => Promise<void>) => {
    try {
      setActionError(null);
      await action();
    } catch (error) {
      setActionError(errorMessage(error));
    }
  }, []);

  const canStart = sourceId !== null && missingRequiredModels(models, settings).length === 0;

  const toggleSession = useCallback(() => {
    if (state.state === "starting" || state.state === "stopping") return;
    if (isActive(state)) return void run(ipc.stopSession);
    if (!sourceId || !canStart) return;
    void run(() => ipc.startSession(microphoneId ? { sourceId, microphoneId } : { sourceId }));
  }, [state, sourceId, microphoneId, canStart, run]);

  const togglePause = useCallback(() => {
    if (state.state === "running") void run(ipc.pauseSession);
    else if (state.state === "paused") void run(ipc.resumeSession);
  }, [state, run]);

  const toggleCopilot = useCallback(() => {
    const current = useCopilot.getState();
    current.setMode(current.mode === "compact" ? "expanded" : "compact");
  }, []);

  const toggleAlwaysOnTop = useCallback(() => {
    const next = !(settings?.alwaysOnTop ?? true);
    void run(async () => {
      await getCurrentWindow().setAlwaysOnTop(next);
      await saveSettings({ alwaysOnTop: next });
    });
  }, [settings, saveSettings, run]);

  useEffect(() => {
    if (settings) void getCurrentWindow().setAlwaysOnTop(settings.alwaysOnTop);
  }, [settings]);

  const shortcuts = useMemo(
    () => ({
      "mod+enter": toggleSession,
      "mod+p": togglePause,
      "mod+,": () => setSettingsOpen(true),
      "mod+j": toggleCopilot,
      "mod+t": toggleAlwaysOnTop,
    }),
    [toggleSession, togglePause, toggleAlwaysOnTop, toggleCopilot],
  );
  useShortcuts(shortcuts);

  const failure = state.state === "failed" ? state.reason : null;

  return (
    <TooltipProvider>
      <main className="window relative flex h-full flex-col gap-2 overflow-hidden p-2">
        <div className="ambient" aria-hidden />
        <NoiseTexture className="-z-10 rounded-[inherit] opacity-[0.07] dark:opacity-[0.09]" aria-hidden />
        <TitleBar
          canStart={canStart}
          copilotOpen={mode !== "compact"}
          onToggleSession={toggleSession}
          onTogglePause={togglePause}
          onToggleCopilot={toggleCopilot}
          onToggleAlwaysOnTop={toggleAlwaysOnTop}
          onOpenSettings={() => setSettingsOpen(true)}
        />
        <div className="flex shrink-0 items-center justify-between gap-2 px-2 text-[11px] text-ink-muted">
          <span>
            {state.state === "running"
              ? "Listening"
              : state.state === "paused"
                ? "Paused"
                : state.state === "starting"
                  ? "Starting capture…"
                  : state.state === "stopping"
                    ? "Stopping capture…"
                    : "Audio off"}{" "}
            · {mode === "compact" ? "Subtitles" : mode === "reading" ? "Reading" : "Preparation"}
          </span>
          <div className="flex gap-1">
            {pinnedReply && mode !== "reading" && (
              <Button size="sm" className="text-[11px]" onClick={() => useCopilot.getState().setMode("reading")}>
                Pinned answer
              </Button>
            )}
            <Button size="sm" className="text-[11px]" onClick={toggleCopilot}>
              {mode === "compact" ? "Open copilot" : "Back to subtitles"}
            </Button>
          </div>
        </div>
        <div
          className={
            mode === "reading" ? "hidden" : `flex min-h-0 flex-1 flex-col ${mode === "expanded" ? "min-h-24" : ""}`
          }
        >
          <TranscriptView emptyState={<EmptyState />} />
        </div>
        {mode === "expanded" && (
          <CopilotPanel enabled={settings?.copilotEnabled ?? false} onOpenSettings={() => setSettingsOpen(true)} />
        )}
        {mode === "reading" && <PinnedReply />}
        <Notices error={actionError ?? resizeError ?? failure ?? appError} />
        <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} />
      </main>
    </TooltipProvider>
  );
}
