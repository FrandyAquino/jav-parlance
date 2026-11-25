/**
 * Typed facade over Tauri commands. Contracts come from `src/bindings`,
 * which is generated from Rust by `cargo test`; never edit those by hand.
 */
import { Channel, type InvokeArgs, invoke } from "@tauri-apps/api/core";
import type { AppInfo } from "@/bindings/AppInfo";
import type { AppSettings } from "@/bindings/AppSettings";
import type { HistoryEntry } from "@/bindings/HistoryEntry";
import type { HistoryLine } from "@/bindings/HistoryLine";
import type { IpcError } from "@/bindings/IpcError";
import type { ModelDescriptor } from "@/bindings/ModelDescriptor";
import type { ModelProgress } from "@/bindings/ModelProgress";
import type { SessionEvent } from "@/bindings/SessionEvent";
import type { SessionState } from "@/bindings/SessionState";
import type { SourceDescriptor } from "@/bindings/SourceDescriptor";
import type { StartSessionRequest } from "@/bindings/StartSessionRequest";

export function isIpcError(value: unknown): value is IpcError {
  return typeof value === "object" && value !== null && "code" in value && "message" in value;
}

export function errorMessage(error: unknown): string {
  if (isIpcError(error)) return error.message;
  if (error instanceof Error) return error.message;
  return String(error);
}

/** Commands that return `()` in Rust resolve to `null`; expose them as `void`. */
function command(name: string, args?: InvokeArgs): Promise<void> {
  return invoke(name, args).then(() => undefined);
}

export const ipc = {
  appInfo: () => invoke<AppInfo>("app_info"),
  listSources: () => invoke<SourceDescriptor[]>("list_sources"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  updateSettings: (settings: AppSettings) => invoke<AppSettings>("update_settings", { settings }),
  listModels: () => invoke<ModelDescriptor[]>("list_models"),
  downloadModel: (id: string, onProgress: (progress: ModelProgress) => void) => {
    const channel = new Channel<ModelProgress>();
    channel.onmessage = onProgress;
    return command("download_model", { id, onProgress: channel });
  },
  cancelDownload: (id: string) => command("cancel_download", { id }),
  subscribe: (onEvent: (event: SessionEvent) => void) => {
    const channel = new Channel<SessionEvent>();
    channel.onmessage = onEvent;
    return command("subscribe", { onEvent: channel });
  },
  sessionState: () => invoke<SessionState>("session_state"),
  startSession: (request: StartSessionRequest) => command("start_session", { request }),
  stopSession: () => command("stop_session"),
  pauseSession: () => command("pause_session"),
  resumeSession: () => command("resume_session"),
  listHistory: () => invoke<HistoryEntry[]>("list_history"),
  historyTranscript: (id: number) => invoke<HistoryLine[]>("history_transcript", { id }),
  deleteHistory: (id: number) => command("delete_history", { id }),
};
