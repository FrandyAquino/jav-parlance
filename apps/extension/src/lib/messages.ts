import { isRecord } from "./guards";

/** Internal messages between the popup, service worker and offscreen document. */

export type CaptureStatus =
  | { state: "idle" }
  | { state: "starting" }
  | { state: "capturing"; tabTitle: string }
  | { state: "error"; reason: string };

/** Popup to service worker; answered with the resulting `CaptureStatus`. */
export type PopupRequest = { target: "background"; kind: "start" | "stop" | "status" };

/** Service worker broadcast so an open popup can follow state changes. */
export type StatusUpdate = { target: "popup"; kind: "status"; status: CaptureStatus };

/** Service worker to offscreen document. */
export type OffscreenCommand = { target: "offscreen"; kind: "start"; streamId: string };

/** Offscreen document to service worker, over the audio port. */
export type CaptureEvent =
  { kind: "started" } | { kind: "frame"; pcm16Base64: string } | { kind: "failed"; reason: string } | { kind: "ended" };

export function isPopupRequest(value: unknown): value is PopupRequest {
  return hasTarget(value, "background") && ["start", "stop", "status"].includes(String(value["kind"]));
}

export function isStatusUpdate(value: unknown): value is StatusUpdate {
  return hasTarget(value, "popup") && value["kind"] === "status" && isRecord(value["status"]);
}

export function isOffscreenCommand(value: unknown): value is OffscreenCommand {
  return hasTarget(value, "offscreen") && value["kind"] === "start" && typeof value["streamId"] === "string";
}

export function isCaptureEvent(value: unknown): value is CaptureEvent {
  if (!isRecord(value)) return false;
  switch (value["kind"]) {
    case "started":
    case "ended":
      return true;
    case "frame":
      return typeof value["pcm16Base64"] === "string";
    case "failed":
      return typeof value["reason"] === "string";
    default:
      return false;
  }
}

function hasTarget(value: unknown, target: string): value is Record<string, unknown> {
  return isRecord(value) && value["target"] === target;
}
