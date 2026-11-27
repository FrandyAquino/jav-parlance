import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AUDIO_PORT_NAME } from "./lib/constants";
import type { CaptureStatus } from "./lib/messages";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function listeners<T extends (...args: never[]) => unknown>() {
  const callbacks = new Set<T>();
  return {
    addListener: (callback: T) => callbacks.add(callback),
    removeListener: (callback: T) => callbacks.delete(callback),
    emit: (...args: Parameters<T>) => {
      for (const callback of [...callbacks]) callback(...args);
    },
  };
}

function nativeHost() {
  const onMessage = listeners<(message: unknown) => void>();
  const onDisconnect = listeners<() => void>();
  return {
    onMessage,
    onDisconnect,
    disconnect: vi.fn(),
    postMessage: vi.fn((message: { type: string }) => {
      if (message.type === "hello")
        onMessage.emit({ type: "ready", protocolVersion: 1 });
    }),
  };
}

function browser() {
  const onMessage =
    listeners<
      (
        message: unknown,
        sender: chrome.runtime.MessageSender,
        respond: (status: CaptureStatus) => void,
      ) => boolean
    >();
  const onConnect = listeners<(port: unknown) => void>();
  const hosts: ReturnType<typeof nativeHost>[] = [];
  let documentOpen = false;
  const statusUpdates: CaptureStatus[] = [];
  const api = {
    runtime: {
      id: "test-extension",
      onMessage,
      onConnect,
      ContextType: { OFFSCREEN_DOCUMENT: "OFFSCREEN_DOCUMENT" },
      getURL: (path: string) => `chrome-extension://test-extension/${path}`,
      getContexts: vi.fn(async () => (documentOpen ? [{}] : [])),
      connectNative: vi.fn(() => {
        const host = nativeHost();
        hosts.push(host);
        return host;
      }),
      sendMessage: vi.fn(
        async (message: { target: string; status?: CaptureStatus }) => {
          if (message.status) statusUpdates.push(message.status);
        },
      ),
    },
    tabs: { query: vi.fn(async () => [{ id: 1, title: "Meeting" }]) },
    tabCapture: { getMediaStreamId: vi.fn(async () => "stream-id") },
    offscreen: {
      Reason: { USER_MEDIA: "USER_MEDIA" },
      createDocument: vi.fn(async () => {
        documentOpen = true;
      }),
      closeDocument: vi.fn(async () => {
        documentOpen = false;
      }),
    },
  };
  return {
    api,
    host: (index: number) => {
      const host = hosts[index];
      if (!host) throw new Error(`Native host ${index} was not connected`);
      return host;
    },
    statusUpdates,
    isOpen: () => documentOpen,
    open: () => {
      documentOpen = true;
    },
    close: () => {
      documentOpen = false;
    },
    audioPort: () => {
      const port = {
        name: AUDIO_PORT_NAME,
        sender: { url: api.runtime.getURL("offscreen.html") },
        onMessage: listeners<(message: unknown) => void>(),
        disconnect: vi.fn(),
      };
      onConnect.emit(port);
      return port;
    },
    request: (kind: "start" | "stop" | "status") =>
      new Promise<CaptureStatus>((resolve) => {
        onMessage.emit({ target: "background", kind }, {}, resolve);
      }),
    starts: () =>
      api.runtime.sendMessage.mock.calls.filter(
        ([message]) => message.target === "offscreen",
      ),
  };
}

// Drain promise continuations without time-based races or timer sleeps.
async function settle() {
  for (let turn = 0; turn < 30; turn++) await Promise.resolve();
}

describe("background capture lifecycle", () => {
  let chromeMock: ReturnType<typeof browser>;

  beforeEach(async () => {
    vi.resetModules();
    chromeMock = browser();
    vi.stubGlobal("chrome", chromeMock.api);
    await import("./background");
  });

  afterEach(() => vi.unstubAllGlobals());

  it("does not create or start after disconnect during the document lookup", async () => {
    const lookup = deferred<object[]>();
    chromeMock.api.runtime.getContexts.mockImplementationOnce(
      () => lookup.promise,
    );
    const start = chromeMock.request("start");
    await settle();
    expect(chromeMock.api.runtime.getContexts).toHaveBeenCalledTimes(1);
    chromeMock.host(0).onDisconnect.emit();
    await settle();
    lookup.resolve([]);
    await start;
    await settle();

    expect(chromeMock.starts()).toHaveLength(0);
    expect(chromeMock.api.offscreen.createDocument).not.toHaveBeenCalled();
    expect(chromeMock.isOpen()).toBe(false);
    expect(chromeMock.statusUpdates.at(-1)?.state).toBe("error");
  });

  it("closes a late-created document without starting after host disconnect", async () => {
    const creation = deferred<undefined>();
    chromeMock.api.offscreen.createDocument.mockImplementationOnce(async () => {
      await creation.promise;
      chromeMock.open();
    });
    const start = chromeMock.request("start");
    await settle();
    expect(chromeMock.api.offscreen.createDocument).toHaveBeenCalledTimes(1);
    chromeMock.host(0).onDisconnect.emit();
    await settle();
    creation.resolve(undefined);
    await start;
    await settle();

    expect(chromeMock.starts()).toHaveLength(0);
    expect(chromeMock.isOpen()).toBe(false);
    expect(chromeMock.statusUpdates.at(-1)?.state).toBe("error");
  });

  it("starts and stops normally and permits immediate retry", async () => {
    await chromeMock.request("start");
    chromeMock.audioPort().onMessage.emit({ kind: "started" });
    await settle();
    expect(chromeMock.statusUpdates.at(-1)).toEqual({
      state: "capturing",
      tabTitle: "Meeting",
    });
    expect(chromeMock.starts()).toHaveLength(1);
    expect(chromeMock.isOpen()).toBe(true);
    expect(await chromeMock.request("stop")).toEqual({ state: "idle" });
    expect(chromeMock.isOpen()).toBe(false);
    expect(chromeMock.host(0).postMessage).toHaveBeenCalledWith({
      type: "end",
    });

    await chromeMock.request("start");
    expect(chromeMock.starts()).toHaveLength(2);
    expect(chromeMock.isOpen()).toBe(true);
    await chromeMock.request("stop");
    expect(chromeMock.isOpen()).toBe(false);
  });

  it("does not let delayed cleanup overwrite or close a newer start", async () => {
    await chromeMock.request("start");
    const closing = deferred<undefined>();
    chromeMock.api.offscreen.closeDocument.mockImplementationOnce(async () => {
      await closing.promise;
      chromeMock.close();
    });
    const stop = chromeMock.request("stop");
    await settle();
    const retry = chromeMock.request("start");
    await settle();
    const lateAudio = chromeMock.audioPort();
    closing.resolve(undefined);
    await Promise.all([stop, retry]);
    await settle();

    expect(chromeMock.starts()).toHaveLength(2);
    expect(chromeMock.isOpen()).toBe(true);
    expect(chromeMock.statusUpdates.at(-1)?.state).toBe("starting");
    expect(lateAudio.disconnect).toHaveBeenCalled();
    lateAudio.onMessage.emit({ kind: "ended" });
    chromeMock.host(0).onDisconnect.emit();
    await settle();
    expect(chromeMock.isOpen()).toBe(true);
    await chromeMock.request("stop");
  });

  it("ignores late capture events from a document belonging to the previous session", async () => {
    await chromeMock.request("start");
    const oldAudio = chromeMock.audioPort();
    await chromeMock.request("stop");
    await chromeMock.request("start");
    const currentAudio = chromeMock.audioPort();
    currentAudio.onMessage.emit({ kind: "started" });
    oldAudio.onMessage.emit({ kind: "ended" });
    await settle();

    expect(chromeMock.isOpen()).toBe(true);
    expect(chromeMock.statusUpdates.at(-1)?.state).toBe("capturing");
    expect(chromeMock.host(1).disconnect).not.toHaveBeenCalled();
    await chromeMock.request("stop");
  });
});
