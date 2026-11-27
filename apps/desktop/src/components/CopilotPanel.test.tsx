import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ipc } from "@/lib/ipc";
import { useApp } from "@/store/app";
import { initialCopilot, useCopilot } from "@/store/copilot";
import { useProfiles } from "@/store/profiles";
import { CopilotPanel } from "./CopilotPanel";

vi.mock("./LiveBeam", () => ({ LiveBeam: () => null }));
afterEach(cleanup);
beforeEach(() => {
  vi.restoreAllMocks();
  useCopilot.setState({ ...initialCopilot, tab: "manual" });
  useProfiles.setState({
    profiles: [],
    activeProfileId: null,
    loaded: true,
    busy: false,
    error: null,
  });
  useApp.setState({
    settings: {
      sourceLanguage: null,
      targetLanguage: "es",
      speechModel: "speech",
      languageModel: "llm",
      translationEnabled: true,
      copilotEnabled: false,
      historyEnabled: false,
      theme: "dark",
      alwaysOnTop: true,
    },
  });
});

it("submits a written question while automatic suggestions are off, without starting audio", async () => {
  vi.spyOn(ipc, "askCopilot").mockResolvedValue();
  const start = vi.spyOn(ipc, "startSession");
  render(<CopilotPanel enabled={false} onOpenSettings={() => {}} />);
  fireEvent.change(screen.getByLabelText("Your question"), {
    target: { value: "Why use Redis?" },
  });
  fireEvent.change(screen.getByLabelText("Your idea (optional)"), {
    target: { value: "Mention latency" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Prepare answer" }));
  await waitFor(() =>
    expect(ipc.askCopilot).toHaveBeenCalledWith(
      expect.objectContaining({
        question: "Why use Redis?",
        idea: "Mention latency",
        replyLanguage: "en",
        userLanguage: "es",
        profileId: null,
      }),
    ),
  );
  expect(start).not.toHaveBeenCalled();
});

it("keeps a failed request editable and displays its error", async () => {
  vi.spyOn(ipc, "askCopilot").mockRejectedValue(new Error("Reduce the project context."));
  render(<CopilotPanel enabled={false} onOpenSettings={() => {}} />);
  fireEvent.change(screen.getByLabelText("Your question"), {
    target: { value: "How do we deploy?" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Prepare answer" }));
  expect(await screen.findByText("Reduce the project context.")).toBeInTheDocument();
  expect(screen.getByLabelText("Your question")).toHaveValue("How do we deploy?");
});

it("shows a cancellation failure without discarding the question", async () => {
  vi.spyOn(ipc, "cancelCopilot").mockRejectedValue(new Error("Could not cancel the answer."));
  useCopilot.setState({ question: "How do we deploy?", manualRequestId: "pending", submitting: true });
  render(<CopilotPanel enabled={false} onOpenSettings={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: "Cancel answer" }));
  expect(await screen.findByText("Could not cancel the answer.")).toBeInTheDocument();
  expect(screen.getByLabelText("Your question")).toHaveValue("How do we deploy?");
});

it("submits from the idea field with Command Enter", async () => {
  vi.spyOn(ipc, "askCopilot").mockResolvedValue();
  useCopilot.getState().setDraft({ question: "How do we deploy?", idea: "Mention staging" });
  render(<CopilotPanel enabled={false} onOpenSettings={() => {}} />);
  fireEvent.keyDown(screen.getByLabelText("Your idea (optional)"), { key: "Enter", metaKey: true });
  await waitFor(() => expect(ipc.askCopilot).toHaveBeenCalledWith(expect.objectContaining({ idea: "Mention staging" })));
});

it("does not duplicate same-language replies and keeps snapshot attribution", () => {
  useCopilot.setState({ manualRequestId: "m" });
  act(() =>
    useCopilot.getState().receive({
      id: 1,
      requestId: "m",
      origin: "manual",
      questionSegmentId: null,
      question: "Redis?",
      idea: "",
      speakerLanguage: "es",
      userLanguage: "es",
      replyInSpeakerLanguage: "Usamos una caché.",
      replyInUserLanguage: "Usamos una caché.",
      profile: { id: "old", name: "Previous project", revision: 2 },
      status: "complete",
      isComplete: true,
      error: null,
      conversationTruncated: false,
    }),
  );
  render(<CopilotPanel enabled onOpenSettings={() => {}} />);
  expect(screen.getAllByText("Usamos una caché.")).toHaveLength(1);
  expect(screen.getByText(/Previous project/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Pin answer to read" }));
  expect(useCopilot.getState().pinned?.profile?.name).toBe("Previous project");
});
