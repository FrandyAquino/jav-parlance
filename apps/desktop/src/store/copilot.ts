import { create } from "zustand";
import type { Suggestion } from "@/bindings/Suggestion";
import { errorMessage, ipc } from "@/lib/ipc";

export type WindowMode = "compact" | "expanded" | "reading";
type CopilotTab = "live" | "manual";

interface CopilotView {
  mode: WindowMode;
  tab: CopilotTab;
  question: string;
  idea: string;
  replyLanguage: string;
  manualRequestId: string | null;
  manualResult: Suggestion | null;
  latestAutomatic: Suggestion | null;
  pinned: Suggestion | null;
  submitting: boolean;
  submissionError: string | null;
}

interface CopilotStore extends CopilotView {
  setMode: (mode: WindowMode) => void;
  setTab: (tab: CopilotTab) => void;
  setDraft: (patch: Partial<Pick<CopilotView, "question" | "idea" | "replyLanguage">>) => void;
  receive: (suggestion: Suggestion) => void;
  prepare: (suggestion: Suggestion) => void;
  pin: (suggestion: Suggestion) => void;
  unpin: () => void;
  ask: (userLanguage: string, profileId: string | null) => Promise<void>;
  cancel: () => Promise<void>;
}

export const initialCopilot: CopilotView = {
  mode: "compact",
  tab: "live",
  question: "",
  idea: "",
  replyLanguage: "en",
  manualRequestId: null,
  manualResult: null,
  latestAutomatic: null,
  pinned: null,
  submitting: false,
  submissionError: null,
};

export function isPending(result: Suggestion | null): boolean {
  return result?.status === "queued" || result?.status === "generating";
}

function acceptsUpdate(current: Suggestion | null, next: Suggestion): boolean {
  if (!current) return true;
  if (next.id !== current.id) return next.id > current.id;
  return isPending(current);
}

export const useCopilot = create<CopilotStore>((set, get) => ({
  ...initialCopilot,
  setMode: (mode) => {
    if (mode !== "reading" || get().pinned) set({ mode });
  },
  setTab: (tab) => set({ tab }),
  setDraft: (patch) => set({ ...patch, tab: "manual", submissionError: null }),
  receive: (suggestion) => {
    const current = get();
    if (suggestion.origin === "manual") {
      if (suggestion.requestId === current.manualRequestId && acceptsUpdate(current.manualResult, suggestion)) {
        set({ manualResult: suggestion });
      }
    } else if (acceptsUpdate(current.latestAutomatic, suggestion)) {
      set({ latestAutomatic: suggestion });
    }
  },
  prepare: (suggestion) =>
    set({
      tab: "manual",
      mode: "expanded",
      question: suggestion.question,
      idea: suggestion.idea,
      replyLanguage: suggestion.speakerLanguage ?? get().replyLanguage,
    }),
  pin: (suggestion) => {
    if (suggestion.status !== "complete") return;
    set({
      pinned: {
        ...suggestion,
        profile: suggestion.profile ? { ...suggestion.profile } : null,
      },
      mode: "reading",
    });
  },
  unpin: () => set({ pinned: null, mode: "expanded" }),
  ask: async (userLanguage, profileId) => {
    if (get().submitting) return;
    const { question, idea, replyLanguage } = get();
    if (!question.trim()) {
      set({ submissionError: "Write or paste a question first." });
      return;
    }
    const requestId = crypto.randomUUID();
    set({
      manualRequestId: requestId,
      manualResult: null,
      submissionError: null,
      submitting: true,
      tab: "manual",
    });
    try {
      await ipc.askCopilot({
        requestId,
        question: question.trim(),
        idea: idea.trim(),
        replyLanguage,
        userLanguage,
        profileId,
      });
    } catch (error) {
      if (get().manualRequestId === requestId) set({ submissionError: errorMessage(error) });
    } finally {
      if (get().manualRequestId === requestId) set({ submitting: false });
    }
  },
  cancel: async () => {
    const id = get().manualRequestId;
    if (!id) return;
    try {
      await ipc.cancelCopilot(id);
    } catch (error) {
      if (get().manualRequestId === id) set({ submissionError: errorMessage(error) });
    }
  },
}));
