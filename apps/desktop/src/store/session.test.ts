import { describe, expect, it } from "vitest";
import type { TranscriptSegment } from "@/bindings/TranscriptSegment";
import { initialSession, MAX_ROWS, reduceSession, type SessionView } from "./session";

function segment(id: number, revision: number, stableText: string, final = false): TranscriptSegment {
  return {
    id,
    revision,
    stableText,
    unstableText: final ? "" : "…",
    language: "en",
    startMs: id * 1000,
    endMs: id * 1000 + 900,
    stability: final ? "final" : "partial",
  };
}

function apply(view: SessionView, ...segments: TranscriptSegment[]): SessionView {
  return segments.reduce((v, s) => reduceSession(v, { type: "transcript", segment: s }), view);
}

describe("reduceSession", () => {
  it("replaces segments in place by revision and ignores stale updates", () => {
    const view = apply(initialSession, segment(1, 0, "hel"), segment(1, 2, "hello"), segment(1, 1, "stale"));
    expect(view.order).toEqual([1]);
    expect(view.rows[1]?.segment.stableText).toBe("hello");
  });

  it("orders rows by segment id even if they arrive out of order", () => {
    const view = apply(initialSession, segment(2, 0, "b"), segment(1, 0, "a"));
    expect(view.order).toEqual([1, 2]);
  });

  it("removes rows whose final text is empty", () => {
    const view = apply(initialSession, segment(1, 0, "uh"), segment(1, 1, "", true));
    expect(view.order).toEqual([]);
    expect(view.rows[1]).toBeUndefined();
  });

  it("keeps translations aligned to their segment and newest source revision", () => {
    let view = apply(initialSession, segment(1, 0, "hola"));
    const translation = { segmentId: 1, targetLanguage: "en", text: "hello", isFinal: false };
    view = reduceSession(view, { type: "translation", translation: { ...translation, sourceRevision: 2 } });
    view = reduceSession(view, { type: "translation", translation: { ...translation, sourceRevision: 1, text: "old" } });
    view = apply(view, segment(1, 3, "hola mundo"));
    expect(view.rows[1]?.translation?.text).toBe("hello");
  });

  it("bounds memory for long meetings", () => {
    const segments = Array.from({ length: MAX_ROWS + 10 }, (_, i) => segment(i + 1, 0, `s${i}`, true));
    const view = apply(initialSession, ...segments);
    expect(view.order).toHaveLength(MAX_ROWS);
    expect(view.order[0]).toBe(11);
    expect(Object.keys(view.rows)).toHaveLength(MAX_ROWS);
  });

  it("clears the previous transcript when a new session starts and tracks run time", () => {
    let view = apply(initialSession, segment(1, 0, "old"));
    view = reduceSession(view, { type: "stateChanged", state: { state: "starting" } }, 0);
    expect(view.order).toEqual([]);
    view = reduceSession(view, { type: "stateChanged", state: { state: "running" } }, 5000);
    expect(view.startedAt).toBe(5000);
    view = reduceSession(view, { type: "stateChanged", state: { state: "idle" } }, 9000);
    expect(view.startedAt).toBeNull();
  });

  it("keeps only the latest notices", () => {
    let view = initialSession;
    for (let i = 0; i < 5; i += 1) {
      view = reduceSession(view, { type: "notice", notice: { level: "info", message: `n${i}` } });
    }
    expect(view.notices.map((n) => n.message)).toEqual(["n2", "n3", "n4"]);
  });
});
