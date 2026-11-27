import { beforeEach, expect, it, vi } from "vitest";
import type { Suggestion } from "@/bindings/Suggestion";
import { ipc } from "@/lib/ipc";
import { initialCopilot, useCopilot } from "./copilot";

vi.mock("@/lib/ipc", () => ({
  ipc: { askCopilot: vi.fn(), cancelCopilot: vi.fn() },
  errorMessage: (error: unknown) => (error instanceof Error ? error.message : String(error)),
}));

const reply = (overrides: Partial<Suggestion> = {}): Suggestion => ({
  id: 1,
  requestId: "auto-1",
  origin: "automatic",
  questionSegmentId: 1,
  question: "Why Redis?",
  idea: "",
  speakerLanguage: "en",
  userLanguage: "es",
  replyInSpeakerLanguage: "We cache reads.",
  replyInUserLanguage: "Cacheamos lecturas.",
  profile: { id: "a", name: "API", revision: 1 },
  status: "complete",
  error: null,
  conversationTruncated: false,
  isComplete: true,
  ...overrides,
});

beforeEach(() => {
  useCopilot.setState(initialCopilot);
  vi.clearAllMocks();
});

it("keeps a manual draft and result when automatic suggestions arrive", () => {
  useCopilot.getState().setDraft({ question: "My question", idea: "Mention latency" });
  useCopilot.setState({ manualRequestId: "mine" });
  useCopilot.getState().receive(reply({ requestId: "mine", origin: "manual" }));
  useCopilot.getState().receive(reply({ id: 2, requestId: "auto-2" }));
  expect(useCopilot.getState()).toMatchObject({
    tab: "manual",
    question: "My question",
    idea: "Mention latency",
    manualResult: { requestId: "mine" },
    latestAutomatic: { requestId: "auto-2" },
  });
});

it("pins an immutable finished response and rejects stale callbacks", () => {
  const result = reply();
  useCopilot.getState().receive(result);
  useCopilot.getState().pin(result);
  useCopilot.getState().receive(
    reply({
      status: "generating",
      isComplete: false,
      replyInSpeakerLanguage: "late",
    }),
  );
  useCopilot.getState().receive(
    reply({
      id: 2,
      requestId: "auto-2",
      replyInSpeakerLanguage: "Another answer",
    }),
  );
  useCopilot.getState().setMode("compact");
  expect(useCopilot.getState().pinned?.replyInSpeakerLanguage).toBe("We cache reads.");
  expect(useCopilot.getState().pinned?.profile?.name).toBe("API");
  expect(useCopilot.getState().pinned).not.toBe(result);
});

it("correlates an event arriving before IPC acceptance and sends a concrete language", async () => {
  useCopilot.getState().setDraft({
    question: "Why Redis?",
    idea: "Be concise",
    replyLanguage: "en",
  });
  vi.mocked(ipc.askCopilot).mockImplementation(async (request) => {
    useCopilot.getState().receive(
      reply({
        requestId: request.requestId,
        origin: "manual",
        questionSegmentId: null,
      }),
    );
  });
  await useCopilot.getState().ask("es", "a");
  expect(ipc.askCopilot).toHaveBeenCalledWith(
    expect.objectContaining({
      question: "Why Redis?",
      replyLanguage: "en",
      userLanguage: "es",
      profileId: "a",
    }),
  );
  expect(useCopilot.getState().manualResult?.status).toBe("complete");
  expect(useCopilot.getState().submitting).toBe(false);
});

it("preserves inputs on rejected submission and gives a retry a new identity", async () => {
  useCopilot.getState().setDraft({ question: "Why Redis?", idea: "Explain cache invalidation" });
  vi.mocked(ipc.askCopilot).mockRejectedValueOnce(new Error("Context is too large"));
  await useCopilot.getState().ask("es", null);
  const firstId = useCopilot.getState().manualRequestId;
  if (firstId === null) throw new Error("Request was not assigned an identity");
  expect(useCopilot.getState()).toMatchObject({
    question: "Why Redis?",
    idea: "Explain cache invalidation",
    submissionError: "Context is too large",
  });
  vi.mocked(ipc.askCopilot).mockResolvedValueOnce(undefined);
  await useCopilot.getState().ask("es", null);
  expect(useCopilot.getState().manualRequestId).not.toBe(firstId);
  useCopilot.getState().receive(reply({ requestId: firstId, origin: "manual" }));
  expect(useCopilot.getState().manualResult).toBeNull();
});

it("does not enter reading mode or pin an unfinished response", () => {
  useCopilot.getState().setMode("reading");
  expect(useCopilot.getState().mode).toBe("compact");
  useCopilot.getState().pin(reply({ status: "failed", isComplete: false }));
  expect(useCopilot.getState().pinned).toBeNull();
});
