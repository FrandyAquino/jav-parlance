import { AUDIO_PORT_NAME, NATIVE_HOST_NAME, PROTOCOL_VERSION } from "./lib/constants";
import { describeError, disconnectReason, handshakeFailureReason } from "./lib/errors";
import {
  type CaptureEvent,
  type CaptureStatus,
  type OffscreenCommand,
  type PopupRequest,
  type StatusUpdate,
  isCaptureEvent,
  isPopupRequest,
} from "./lib/messages";
import { END_MESSAGE, audioMessage, helloMessage, parseHostMessage } from "./lib/wire";

const OFFSCREEN_PATH = "offscreen.html";
const FALLBACK_TAB_TITLE = "Browser tab";

interface Session {
  host: chrome.runtime.Port;
  tabTitle: string;
  sequence: number;
}

let status: CaptureStatus = { state: "idle" };
let session: Session | null = null;
// Bumped by every start/stop so a superseded start bails out after each await.
let generation = 0;

chrome.runtime.onMessage.addListener((message: unknown, _sender, sendResponse) => {
  if (!isPopupRequest(message)) return false;
  void handlePopupRequest(message).then(sendResponse);
  return true;
});

chrome.runtime.onConnect.addListener((port) => {
  if (port.name !== AUDIO_PORT_NAME || port.sender?.url !== chrome.runtime.getURL(OFFSCREEN_PATH)) {
    port.disconnect();
    return;
  }
  port.onMessage.addListener((event: unknown) => {
    if (isCaptureEvent(event)) void handleCaptureEvent(event);
  });
});

async function handlePopupRequest(request: PopupRequest): Promise<CaptureStatus> {
  if (request.kind === "start") await startCapture();
  if (request.kind === "stop") await stopCapture({ state: "idle" });
  return status;
}

async function handleCaptureEvent(event: CaptureEvent): Promise<void> {
  switch (event.kind) {
    case "started":
      if (session) setStatus({ state: "capturing", tabTitle: session.tabTitle });
      return;
    case "frame":
      if (session) session.host.postMessage(audioMessage(session.sequence++, event.pcm16Base64));
      return;
    case "failed":
      return stopCapture({ state: "error", reason: event.reason });
    case "ended":
      return stopCapture({ state: "error", reason: "The tab stopped sharing audio." });
  }
}

/** Must run from the popup's user gesture: tabCapture is granted through activeTab. */
async function startCapture(): Promise<void> {
  if (status.state === "starting" || status.state === "capturing") return;
  const attempt = ++generation;
  setStatus({ state: "starting" });
  try {
    const tab = await activeTab();
    const streamId = await chrome.tabCapture.getMediaStreamId({ targetTabId: tab.id });
    const tabTitle = tab.title || FALLBACK_TAB_TITLE;
    const host = await connectHost(tabTitle);
    if (attempt !== generation) {
      host.disconnect();
      return;
    }
    session = { host, tabTitle, sequence: 0 };
    watchHost(host);
    await resetOffscreenDocument();
    const command: OffscreenCommand = { target: "offscreen", kind: "start", streamId };
    await chrome.runtime.sendMessage(command);
  } catch (error) {
    if (attempt === generation) await stopCapture({ state: "error", reason: describeError(error) });
  }
}

async function stopCapture(next: CaptureStatus): Promise<void> {
  generation++;
  const current = session;
  session = null;
  if (current) closeHost(current.host);
  await closeOffscreenDocument();
  setStatus(next);
}

async function activeTab(): Promise<chrome.tabs.Tab & { id: number }> {
  const [tab] = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  if (tab?.id === undefined) throw new Error("There is no active tab to capture.");
  return { ...tab, id: tab.id };
}

/** Resolves once the host confirms the desktop app accepted the stream. */
function connectHost(tabTitle: string): Promise<chrome.runtime.Port> {
  return new Promise((resolve, reject) => {
    const host = chrome.runtime.connectNative(NATIVE_HOST_NAME);
    const detach = () => {
      host.onMessage.removeListener(onReply);
      host.onDisconnect.removeListener(onDisconnect);
    };
    const onReply = (raw: unknown) => {
      detach();
      const reply = parseHostMessage(raw);
      if (reply?.type === "ready" && reply.protocolVersion === PROTOCOL_VERSION) {
        resolve(host);
        return;
      }
      host.disconnect();
      reject(new Error(handshakeFailureReason(reply)));
    };
    const onDisconnect = () => {
      detach();
      reject(new Error(disconnectReason(chrome.runtime.lastError?.message, chrome.runtime.id)));
    };
    host.onMessage.addListener(onReply);
    host.onDisconnect.addListener(onDisconnect);
    host.postMessage(helloMessage(tabTitle));
  });
}

function watchHost(host: chrome.runtime.Port): void {
  const stopIfCurrent = (reason: string) => {
    if (session?.host === host) void stopCapture({ state: "error", reason });
  };
  host.onMessage.addListener((raw: unknown) => {
    const message = parseHostMessage(raw);
    if (message?.type === "unavailable") stopIfCurrent(message.reason);
    if (message?.type === "error") stopIfCurrent(handshakeFailureReason(message));
  });
  host.onDisconnect.addListener(() => {
    stopIfCurrent(disconnectReason(chrome.runtime.lastError?.message, chrome.runtime.id));
  });
}

function closeHost(host: chrome.runtime.Port): void {
  try {
    host.postMessage(END_MESSAGE);
  } catch {
    // The host already disconnected; there is nobody to tell.
  }
  host.disconnect();
}

async function resetOffscreenDocument(): Promise<void> {
  await closeOffscreenDocument();
  await chrome.offscreen.createDocument({
    url: OFFSCREEN_PATH,
    reasons: [chrome.offscreen.Reason.USER_MEDIA],
    justification: "Process the captured tab audio and send it to the Parlance desktop app.",
  });
}

async function closeOffscreenDocument(): Promise<void> {
  const contexts = await chrome.runtime.getContexts({
    contextTypes: [chrome.runtime.ContextType.OFFSCREEN_DOCUMENT],
  });
  if (contexts.length > 0) await chrome.offscreen.closeDocument();
}

function setStatus(next: CaptureStatus): void {
  status = next;
  const update: StatusUpdate = { target: "popup", kind: "status", status };
  // Rejects when no popup is open, which is the common case.
  chrome.runtime.sendMessage(update).catch(() => undefined);
}
