import { create } from "zustand";
import type { AppInfo } from "@/bindings/AppInfo";
import type { AppSettings } from "@/bindings/AppSettings";
import type { ModelDescriptor } from "@/bindings/ModelDescriptor";
import type { ModelProgress } from "@/bindings/ModelProgress";
import type { SourceDescriptor } from "@/bindings/SourceDescriptor";
import { errorMessage, ipc, isIpcError } from "@/lib/ipc";
import { without } from "@/lib/record";

interface AppStore {
  info: AppInfo | null;
  settings: AppSettings | null;
  sources: SourceDescriptor[];
  selectedSourceId: string | null;
  microphoneId: string | null;
  models: ModelDescriptor[];
  downloads: Record<string, ModelProgress>;
  error: string | null;
  load: () => Promise<void>;
  refreshSources: () => Promise<void>;
  refreshModels: () => Promise<void>;
  saveSettings: (patch: Partial<AppSettings>) => Promise<void>;
  selectSource: (id: string) => void;
  selectMicrophone: (id: string | null) => void;
  download: (id: string) => Promise<void>;
  cancelDownload: (id: string) => Promise<void>;
}

function defaultSource(sources: SourceDescriptor[]): string | null {
  const preferred = sources.find((s) => s.kind.type === "systemOutput" && s.isDefault) ?? sources[0];
  return preferred?.id ?? null;
}

function secondaryMicrophone(
  sources: SourceDescriptor[],
  primaryId: string | null,
  microphoneId: string | null,
): string | null {
  if (sources.some((s) => s.id === primaryId && s.kind.type === "microphone")) return null;
  return sources.some((s) => s.id === microphoneId && s.kind.type === "microphone") ? microphoneId : null;
}

let sourceRefreshGeneration = 0;

export const useApp = create<AppStore>((set, get) => ({
  info: null,
  settings: null,
  sources: [],
  selectedSourceId: null,
  microphoneId: null,
  models: [],
  downloads: {},
  error: null,

  load: async () => {
    try {
      const [info, settings, models] = await Promise.all([ipc.appInfo(), ipc.getSettings(), ipc.listModels()]);
      set({ info, settings, models, error: null });
      await get().refreshSources();
    } catch (error) {
      set({ error: errorMessage(error) });
    }
  },

  refreshSources: async () => {
    const generation = ++sourceRefreshGeneration;
    try {
      const sources = await ipc.listSources();
      if (generation !== sourceRefreshGeneration) return;
      const current = get().selectedSourceId;
      const stillExists = sources.some((s) => s.id === current);
      const selectedSourceId = stillExists ? current : defaultSource(sources);
      set({
        sources,
        selectedSourceId,
        microphoneId: secondaryMicrophone(sources, selectedSourceId, get().microphoneId),
      });
    } catch (error) {
      if (generation === sourceRefreshGeneration) set({ error: errorMessage(error) });
    }
  },

  refreshModels: async () => set({ models: await ipc.listModels() }),

  saveSettings: async (patch) => {
    const current = get().settings;
    if (!current) return;
    try {
      set({ settings: await ipc.updateSettings({ ...current, ...patch }), error: null });
    } catch (error) {
      set({ error: errorMessage(error) });
    }
  },

  selectSource: (id) =>
    set((s) => ({
      selectedSourceId: id,
      microphoneId: secondaryMicrophone(s.sources, id, s.microphoneId),
    })),
  selectMicrophone: (id) => set((s) => ({ microphoneId: secondaryMicrophone(s.sources, s.selectedSourceId, id) })),

  download: async (id) => {
    const track = (progress: ModelProgress) => set((s) => ({ downloads: { ...s.downloads, [id]: progress } }));
    try {
      await get().refreshModels();
      await ipc.downloadModel(id, track);
    } catch (error) {
      if (!isIpcError(error) || error.code !== "cancelled") set({ error: errorMessage(error) });
    } finally {
      set((s) => ({ downloads: without(s.downloads, [id]) }));
      await get().refreshModels();
    }
  },

  cancelDownload: (id) => ipc.cancelDownload(id),
}));

/** Models the core needs before a session can start. */
export function missingRequiredModels(models: ModelDescriptor[], settings: AppSettings | null): ModelDescriptor[] {
  if (!settings) return [];
  const required = new Set([settings.speechModel, "silero-vad-5.1.2"]);
  return models.filter((m) => required.has(m.id) && m.status !== "installed");
}
