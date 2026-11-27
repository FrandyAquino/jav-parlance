import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ipc } from "@/lib/ipc";
import { useApp } from "@/store/app";
import { initialSession, useSession } from "@/store/session";
import { initialCopilot, useCopilot } from "@/store/copilot";
import { App } from "./App";

const native = vi.hoisted(() => ({
  setAlwaysOnTop: vi.fn().mockResolvedValue(undefined),
  minimize: vi.fn(),
  close: vi.fn(),
}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => native }));
vi.mock("./hooks/useWindowMode", () => ({ useWindowMode: () => null }));
vi.mock("./hooks/useTheme", () => ({ useTheme: () => undefined }));
vi.mock("./components/LiveBeam", () => ({ LiveBeam: () => null }));
afterEach(cleanup);
beforeEach(() => {
  vi.restoreAllMocks();
  vi.spyOn(ipc, "subscribe").mockResolvedValue();
  vi.spyOn(ipc, "listProjectProfiles").mockResolvedValue({
    profiles: [],
    activeProfileId: null,
  });
  vi.spyOn(ipc, "startSession").mockResolvedValue();
  vi.spyOn(ipc, "stopSession").mockResolvedValue();
  useCopilot.setState(initialCopilot);
  useApp.setState({
    load: vi.fn().mockResolvedValue(undefined),
    models: [],
    selectedSourceId: "system",
    sources: [],
    error: null,
    settings: {
      sourceLanguage: "en",
      targetLanguage: "es",
      speechModel: "speech",
      languageModel: "llm",
      translationEnabled: true,
      copilotEnabled: true,
      historyEnabled: false,
      theme: "dark",
      alwaysOnTop: true,
    },
  });
  useSession.setState({
    ...initialSession,
    state: { state: "running" },
    order: [1],
    rows: {
      1: {
        segment: {
          id: 1,
          revision: 1,
          stableText: "Why Redis?",
          unstableText: "",
          language: "en",
          startMs: 0,
          endMs: 1000,
          stability: "final",
        },
        translation: null,
      },
    },
  });
  Element.prototype.scrollTo = vi.fn();
});

it("changes modes without restarting audio or remounting the paused transcript", async () => {
  render(<App />);
  await waitFor(() => expect(ipc.subscribe).toHaveBeenCalled());
  const log = screen.getByRole("log");
  Object.defineProperties(log, {
    scrollHeight: { value: 1000 },
    clientHeight: { value: 200 },
  });
  fireEvent.scroll(log);
  expect(screen.getByRole("button", { name: "Live" })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Open copilot" }));
  expect(screen.getByRole("log")).toBe(log);
  act(() => useCopilot.getState().setDraft({ question: "Manual question", idea: "A detail" }));
  fireEvent.click(screen.getByRole("button", { name: "Back to subtitles" }));
  expect(screen.getByRole("log")).toBe(log);
  expect(screen.getByRole("button", { name: "Live" })).toBeInTheDocument();
  expect(useCopilot.getState().question).toBe("Manual question");
  expect(useSession.getState().state.state).toBe("running");
  expect(ipc.startSession).not.toHaveBeenCalled();
  expect(ipc.stopSession).not.toHaveBeenCalled();
});
