// Mirrors `parlance_protocol::native_messaging` constants, which ts-rs does not export.
export const PROTOCOL_VERSION = 1;
export const BRIDGE_SAMPLE_RATE = 16_000;
export const MAX_MESSAGE_BYTES = 1024 * 1024;

export const NATIVE_HOST_NAME = "app.parlance.native_host";
export const PROCESSOR_NAME = "parlance-tab-capture";
export const AUDIO_PORT_NAME = "parlance-audio";

/** 100 ms of bridge-rate audio per native message. */
export const FRAME_SAMPLES = BRIDGE_SAMPLE_RATE / 10;
