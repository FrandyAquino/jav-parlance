import type { ExtensionMessage } from "../../../desktop/src/bindings/ExtensionMessage";
import type { HostMessage } from "../../../desktop/src/bindings/HostMessage";
import { BRIDGE_SAMPLE_RATE, PROTOCOL_VERSION } from "./constants";
import { isRecord } from "./guards";

export type { HostMessage };

type AudioBinding = Extract<ExtensionMessage, { type: "audio" }>;

/**
 * ts-rs maps Rust `u64` to `bigint`, which JSON (and so native messaging)
 * cannot carry; a JS number is exact for any realistic frame count.
 */
export type AudioMessage = Omit<AudioBinding, "sequence"> & { sequence: number };
export type WireExtensionMessage = Exclude<ExtensionMessage, AudioBinding> | AudioMessage;

export function helloMessage(tabTitle: string): WireExtensionMessage {
  return { type: "hello", protocolVersion: PROTOCOL_VERSION, tabTitle: tabTitle };
}

export function audioMessage(sequence: number, pcm16Base64: string): WireExtensionMessage {
  return { type: "audio", sequence, sampleRate: BRIDGE_SAMPLE_RATE, pcm16Base64: pcm16Base64 };
}

export const END_MESSAGE: WireExtensionMessage = { type: "end" };

/** Narrows untrusted host output to a known message, or `null`. */
export function parseHostMessage(value: unknown): HostMessage | null {
  if (!isRecord(value)) return null;
  switch (value["type"]) {
    case "ready":
      return typeof value["protocolVersion"] === "number"
        ? { type: "ready", protocolVersion: value["protocolVersion"] }
        : null;
    case "unavailable":
      return typeof value["reason"] === "string" ? { type: "unavailable", reason: value["reason"] } : null;
    case "error":
      return typeof value["message"] === "string" ? { type: "error", message: value["message"] } : null;
    default:
      return null;
  }
}
