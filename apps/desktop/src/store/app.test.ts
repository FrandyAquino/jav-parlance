import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SourceDescriptor } from "@/bindings/SourceDescriptor";
import { ipc } from "@/lib/ipc";
import { useApp } from "./app";

const system: SourceDescriptor = { id: "system:default", kind: { type: "systemOutput" }, name: "System", isDefault: true };
const microphone: SourceDescriptor = { id: "mic:default", kind: { type: "microphone" }, name: "Microphone", isDefault: true };

beforeEach(() => {
  vi.restoreAllMocks();
  useApp.setState({ sources: [system, microphone], selectedSourceId: system.id, microphoneId: null, downloads: {}, error: null });
});

describe("model downloads", () => {
  it("clears progress without an error when a download is cancelled", async () => {
    vi.spyOn(ipc, "listModels").mockResolvedValue([]);
    vi.spyOn(ipc, "downloadModel").mockImplementation(async (id, onProgress) => {
      onProgress({ id, downloadedBytes: 10, totalBytes: 100, done: false });
      throw { code: "cancelled", message: "Download cancelled" };
    });
    await useApp.getState().download("whisper-base-q8");
    expect(useApp.getState().error).toBeNull();
    expect(useApp.getState().downloads).toEqual({});
  });

  it("still reports an actual download failure", async () => {
    vi.spyOn(ipc, "listModels").mockResolvedValue([]);
    vi.spyOn(ipc, "downloadModel").mockRejectedValue({ code: "network", message: "Connection lost" });
    await useApp.getState().download("whisper-base-q8");
    expect(useApp.getState().error).toBe("Connection lost");
  });
});

describe("source selection", () => {
  it("ignores an older refresh that finishes after a newer microphone selection", async () => {
    let resolveOlder!: (sources: SourceDescriptor[]) => void;
    const older = new Promise<SourceDescriptor[]>((resolve) => { resolveOlder = resolve; });
    vi.spyOn(ipc, "listSources").mockReturnValueOnce(older).mockResolvedValueOnce([system, microphone]);
    const first = useApp.getState().refreshSources();
    await useApp.getState().refreshSources();
    useApp.getState().selectMicrophone(microphone.id);
    resolveOlder([system]);
    await first;
    expect(useApp.getState().microphoneId).toBe(microphone.id);
    expect(useApp.getState().sources).toEqual([system, microphone]);
  });

  it("clears the secondary microphone when a microphone becomes the primary source", () => {
    useApp.getState().selectMicrophone(microphone.id);
    useApp.getState().selectSource(microphone.id);
    expect(useApp.getState().selectedSourceId).toBe(microphone.id);
    expect(useApp.getState().microphoneId).toBeNull();
  });

  it("does not add a second microphone to microphone-only capture", () => {
    useApp.setState({ selectedSourceId: microphone.id });
    useApp.getState().selectMicrophone(microphone.id);
    expect(useApp.getState().microphoneId).toBeNull();
  });

  it("keeps the optional microphone when switching between non-microphone sources", () => {
    const app: SourceDescriptor = { id: "app:42", kind: { type: "application", pid: 42 }, name: "Meeting", isDefault: false };
    useApp.setState({ sources: [system, microphone, app] });
    useApp.getState().selectMicrophone(microphone.id);
    useApp.getState().selectSource(app.id);
    expect(useApp.getState().microphoneId).toBe(microphone.id);
  });

  it("clears an optional microphone that disappeared during source refresh", async () => {
    useApp.setState({ microphoneId: microphone.id });
    vi.spyOn(ipc, "listSources").mockResolvedValue([system]);
    await useApp.getState().refreshSources();
    expect(useApp.getState().microphoneId).toBeNull();
  });

  it("can fall back to a microphone-only device without opening it twice", async () => {
    useApp.setState({ microphoneId: microphone.id });
    vi.spyOn(ipc, "listSources").mockResolvedValue([microphone]);
    await useApp.getState().refreshSources();
    expect(useApp.getState().selectedSourceId).toBe(microphone.id);
    expect(useApp.getState().microphoneId).toBeNull();
  });
});
