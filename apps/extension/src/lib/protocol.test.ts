import { describe, expect, it } from "vitest";
import { MAX_MESSAGE_BYTES } from "./constants";
import { disconnectReason, handshakeFailureReason } from "./errors";
import { isCaptureEvent, isOffscreenCommand, isPopupRequest } from "./messages";
import { statusView } from "./status-view";
import { END_MESSAGE, audioMessage, helloMessage, parseHostMessage } from "./wire";

const EXTENSION_ID = "abcdefghijklmnopabcdefghijklmnop";

describe("wire messages", () => {
  it("serialises in the shape the Rust host deserialises", () => {
    expect(JSON.parse(JSON.stringify(helloMessage("Standup")))).toEqual({
      type: "hello",
      protocolVersion: 1,
      tabTitle: "Standup",
    });
    expect(JSON.parse(JSON.stringify(audioMessage(3, "AAA=")))).toEqual({
      type: "audio",
      sequence: 3,
      sampleRate: 16_000,
      pcm16Base64: "AAA=",
    });
    expect(JSON.stringify(END_MESSAGE)).toBe('{"type":"end"}');
  });

  it("keeps a 100 ms frame far below the native messaging cap", () => {
    const frame = audioMessage(Number.MAX_SAFE_INTEGER, "A".repeat(Math.ceil(3_200 / 3) * 4));
    expect(JSON.stringify(frame).length).toBeLessThan(MAX_MESSAGE_BYTES / 100);
  });
});

describe("parseHostMessage", () => {
  it("accepts every host message", () => {
    expect(parseHostMessage({ type: "ready", protocolVersion: 1 })).toEqual({ type: "ready", protocolVersion: 1 });
    expect(parseHostMessage({ type: "unavailable", reason: "closed" })).toEqual({
      type: "unavailable",
      reason: "closed",
    });
    expect(parseHostMessage({ type: "error", message: "bad" })).toEqual({ type: "error", message: "bad" });
  });

  it("rejects unknown or malformed messages", () => {
    for (const value of [null, "ready", { type: "ready" }, { type: "unavailable", reason: 1 }, { type: "shell" }]) {
      expect(parseHostMessage(value)).toBeNull();
    }
  });
});

describe("error reasons", () => {
  it("passes the host's unavailable reason through", () => {
    expect(handshakeFailureReason({ type: "unavailable", reason: "Open Parlance on this computer." })).toBe(
      "Open Parlance on this computer.",
    );
  });

  it("explains protocol mismatches", () => {
    expect(handshakeFailureReason({ type: "ready", protocolVersion: 2 })).toContain("Update both");
  });

  it("tells the user how to install a missing host", () => {
    expect(disconnectReason("Specified native messaging host not found.", EXTENSION_ID)).toContain(
      `parlance-native-host --install ${EXTENSION_ID}`,
    );
  });

  it("tells the user how to allow this extension", () => {
    expect(disconnectReason("Access to the specified native messaging host is forbidden.", EXTENSION_ID)).toContain(
      "does not allow this extension",
    );
  });

  it("falls back to a generic reason", () => {
    expect(disconnectReason(undefined, EXTENSION_ID)).toContain("Check that Parlance is open");
  });
});

describe("internal message guards", () => {
  it("recognises popup requests by target and kind", () => {
    expect(isPopupRequest({ target: "background", kind: "start" })).toBe(true);
    expect(isPopupRequest({ target: "background", kind: "launch" })).toBe(false);
    expect(isPopupRequest({ target: "offscreen", kind: "start" })).toBe(false);
  });

  it("requires a stream id for offscreen commands", () => {
    expect(isOffscreenCommand({ target: "offscreen", kind: "start", streamId: "id" })).toBe(true);
    expect(isOffscreenCommand({ target: "offscreen", kind: "start" })).toBe(false);
  });

  it("validates capture event payloads", () => {
    expect(isCaptureEvent({ kind: "frame", pcm16Base64: "AA==" })).toBe(true);
    expect(isCaptureEvent({ kind: "frame" })).toBe(false);
    expect(isCaptureEvent({ kind: "failed", reason: "denied" })).toBe(true);
    expect(isCaptureEvent({ kind: "ended" })).toBe(true);
    expect(isCaptureEvent("ended")).toBe(false);
  });
});

describe("statusView", () => {
  it("offers stop only while capturing", () => {
    expect(statusView({ state: "capturing", tabTitle: "Sync" })).toMatchObject({
      buttonLabel: "Stop capture",
      active: true,
    });
    expect(statusView({ state: "idle" })).toMatchObject({ buttonLabel: "Start capture", active: false });
  });

  it("disables the button while starting", () => {
    expect(statusView({ state: "starting" }).busy).toBe(true);
  });

  it("surfaces error reasons verbatim", () => {
    expect(statusView({ state: "error", reason: "Open Parlance" }).message).toBe("Open Parlance");
  });
});
