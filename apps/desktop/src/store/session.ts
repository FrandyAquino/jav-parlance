import { create } from "zustand";
import type { AudioLevel } from "@/bindings/AudioLevel";
import type { Notice } from "@/bindings/Notice";
import type { SessionEvent } from "@/bindings/SessionEvent";
import type { SessionState } from "@/bindings/SessionState";
import type { Suggestion } from "@/bindings/Suggestion";
import type { TranscriptSegment } from "@/bindings/TranscriptSegment";
import type { Translation } from "@/bindings/Translation";
import { without } from "@/lib/record";

/** Keeps long meetings from growing the renderer's memory without bound. */
export const MAX_ROWS = 1500;
const MAX_SUGGESTIONS = 8;
const MAX_NOTICES = 3;

export interface TranscriptRow {
  segment: TranscriptSegment;
  translation: Translation | null;
}

export interface ActiveNotice extends Notice {
  id: number;
}

export interface SessionView {
  state: SessionState;
  level: AudioLevel;
  /** Wall-clock ms when the current run started; null while idle. */
  startedAt: number | null;
  order: number[];
  rows: Record<number, TranscriptRow>;
  suggestions: Suggestion[];
  notices: ActiveNotice[];
  detectedLanguage: string | null;
  nextNoticeId: number;
}

export const SILENT: AudioLevel = { rmsDb: -100, peakDb: -100 };

export const initialSession: SessionView = {
  state: { state: "idle" },
  level: SILENT,
  startedAt: null,
  order: [],
  rows: {},
  suggestions: [],
  notices: [],
  detectedLanguage: null,
  nextNoticeId: 1,
};

function applyState(view: SessionView, state: SessionState, now: number): SessionView {
  switch (state.state) {
    case "starting":
      return { ...initialSession, state, notices: view.notices, nextNoticeId: view.nextNoticeId };
    case "running":
      return { ...view, state, startedAt: view.startedAt ?? now };
    case "idle":
    case "failed":
      return { ...view, state, level: SILENT, startedAt: null };
    default:
      return { ...view, state };
  }
}

function applySegment(view: SessionView, segment: TranscriptSegment): SessionView {
  const existing = view.rows[segment.id];
  if (existing && existing.segment.revision >= segment.revision) return view;

  const isEmptyFinal = segment.stability === "final" && segment.stableText.trim() === "";
  if (isEmptyFinal) {
    return { ...view, rows: without(view.rows, [segment.id]), order: view.order.filter((id) => id !== segment.id) };
  }

  const rows = { ...view.rows, [segment.id]: { segment, translation: existing?.translation ?? null } };
  if (existing) return { ...view, rows };

  const order = [...view.order, segment.id].sort((a, b) => a - b);
  const evicted = order.splice(0, Math.max(0, order.length - MAX_ROWS));
  return { ...view, rows: evicted.length > 0 ? without(rows, evicted) : rows, order };
}

/** Older source revisions, and partial text racing a final, never replace what is shown. */
function isStale(current: Translation, incoming: Translation): boolean {
  if (current.sourceRevision > incoming.sourceRevision) return true;
  return current.isFinal && !incoming.isFinal && current.sourceRevision >= incoming.sourceRevision;
}

function applyTranslation(view: SessionView, translation: Translation): SessionView {
  const row = view.rows[translation.segmentId];
  if (!row) return view;
  const current = row.translation;
  if (current && isStale(current, translation)) return view;
  return { ...view, rows: { ...view.rows, [translation.segmentId]: { ...row, translation } } };
}

function applySuggestion(view: SessionView, suggestion: Suggestion): SessionView {
  const others = view.suggestions.filter((s) => s.id !== suggestion.id);
  return { ...view, suggestions: [suggestion, ...others].slice(0, MAX_SUGGESTIONS) };
}

export function reduceSession(view: SessionView, event: SessionEvent, now = Date.now()): SessionView {
  switch (event.type) {
    case "stateChanged":
      return applyState(view, event.state, now);
    case "level":
      return { ...view, level: event.level };
    case "transcript":
      return applySegment(view, event.segment);
    case "translation":
      return applyTranslation(view, event.translation);
    case "suggestion":
      return applySuggestion(view, event.suggestion);
    case "languageDetected":
      return { ...view, detectedLanguage: event.language };
    case "notice": {
      const notice = { ...event.notice, id: view.nextNoticeId };
      return { ...view, notices: [...view.notices, notice].slice(-MAX_NOTICES), nextNoticeId: view.nextNoticeId + 1 };
    }
  }
}

export const useSession = create<SessionView>(() => initialSession);

export function dispatchSessionEvent(event: SessionEvent): void {
  useSession.setState((view) => reduceSession(view, event));
}

export function dismissNotice(id: number): void {
  useSession.setState((view) => ({ ...view, notices: view.notices.filter((n) => n.id !== id) }));
}

export function isActive(state: SessionState): boolean {
  return state.state === "starting" || state.state === "running" || state.state === "paused";
}
