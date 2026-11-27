import { create } from "zustand";
import type { ProjectProfile } from "@/bindings/ProjectProfile";
import type { ProjectProfiles } from "@/bindings/ProjectProfiles";
import type { SaveProjectProfileRequest } from "@/bindings/SaveProjectProfileRequest";
import { errorMessage, ipc } from "@/lib/ipc";

interface ProfilesStore {
  profiles: ProjectProfile[];
  activeProfileId: string | null;
  busy: boolean;
  loaded: boolean;
  error: string | null;
  load: () => Promise<void>;
  save: (request: SaveProjectProfileRequest) => Promise<boolean>;
  remove: (id: string) => Promise<boolean>;
  select: (id: string | null) => Promise<boolean>;
}

export const useProfiles = create<ProfilesStore>((set, get) => {
  async function update(action: () => Promise<ProjectProfiles>): Promise<boolean> {
    if (get().busy) return false;
    set({ busy: true, error: null });
    try {
      set({ ...(await action()), loaded: true });
      return true;
    } catch (error) {
      set({ error: errorMessage(error) });
      return false;
    } finally {
      set({ busy: false });
    }
  }
  return {
    profiles: [],
    activeProfileId: null,
    busy: false,
    loaded: false,
    error: null,
    load: async () => {
      await update(ipc.listProjectProfiles);
    },
    save: (request) => update(() => ipc.saveProjectProfile(request)),
    remove: (id) => update(() => ipc.deleteProjectProfile(id)),
    select: (id) => update(() => ipc.setActiveProjectProfile(id)),
  };
});
