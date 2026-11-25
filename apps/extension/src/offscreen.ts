import type { CaptureProcessorOptions } from "./capture-processor";
import { bytesToBase64 } from "./lib/base64";
import { AUDIO_PORT_NAME, PROCESSOR_NAME } from "./lib/constants";
import { type CaptureEvent, isOffscreenCommand } from "./lib/messages";

// Chrome-specific constraints for consuming a tabCapture stream id.
interface TabAudioConstraints {
  mandatory: { chromeMediaSource: "tab"; chromeMediaSourceId: string };
}

chrome.runtime.onMessage.addListener((message: unknown, _sender, sendResponse) => {
  if (!isOffscreenCommand(message)) return false;
  void startCapture(message.streamId);
  // Acknowledge receipt; the outcome arrives over the audio port.
  sendResponse(true);
  return false;
});

async function startCapture(streamId: string): Promise<void> {
  const port = chrome.runtime.connect({ name: AUDIO_PORT_NAME });
  const send = (event: CaptureEvent) => port.postMessage(event);
  try {
    const stream = await openTabStream(streamId);
    stream.getAudioTracks().forEach((track) => track.addEventListener("ended", () => send({ kind: "ended" })));
    const node = await buildGraph(stream);
    node.port.onmessage = (event: MessageEvent<ArrayBuffer>) => {
      send({ kind: "frame", pcm16Base64: bytesToBase64(new Uint8Array(event.data)) });
    };
    send({ kind: "started" });
  } catch (error) {
    send({ kind: "failed", reason: `Could not capture this tab: ${describe(error)}` });
  }
}

function openTabStream(streamId: string): Promise<MediaStream> {
  const audio: TabAudioConstraints = { mandatory: { chromeMediaSource: "tab", chromeMediaSourceId: streamId } };
  return navigator.mediaDevices.getUserMedia({ audio: audio as MediaTrackConstraints, video: false });
}

async function buildGraph(stream: MediaStream): Promise<AudioWorkletNode> {
  const context = new AudioContext();
  await context.audioWorklet.addModule(chrome.runtime.getURL("capture-processor.js"));
  const source = context.createMediaStreamSource(stream);
  // Capturing a tab mutes it; routing the stream back out keeps it audible.
  source.connect(context.destination);
  const processorOptions: CaptureProcessorOptions = { inputSampleRate: context.sampleRate };
  // No outputs makes Chrome pull the node even though nothing consumes it.
  const node = new AudioWorkletNode(context, PROCESSOR_NAME, {
    numberOfInputs: 1,
    numberOfOutputs: 0,
    processorOptions,
  });
  source.connect(node);
  return node;
}

function describe(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
