import { create } from "zustand";

interface WorkspaceState {
  selectedProfileId: string | null;
  selectProfile: (profileId: string | null) => void;
  reconcileProfiles: (profileIds: string[]) => void;
}

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  selectedProfileId: null,
  selectProfile: (selectedProfileId) => set({ selectedProfileId }),
  reconcileProfiles: (profileIds) => {
    const current = get().selectedProfileId;
    set({
      selectedProfileId: current && profileIds.includes(current)
        ? current
        : profileIds[0] ?? null,
    });
  },
}));
