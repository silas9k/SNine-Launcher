import { invoke } from "@tauri-apps/api/core";
import {
  PHASE5_INSTALL_PROFILE,
  PHASE5_LAUNCH_PROFILE,
  PHASE5_LAUNCH_STATUSES,
  PHASE5_PROFILE_RUNTIME_STATUS,
  PHASE5_REPAIR_PROFILE,
  PHASE5_RUNTIME_CATALOG,
  PHASE5_S9LAB_COMPONENT_CATALOG,
  PHASE5_SET_S9LAB_COMPONENT,
  PHASE5_STOP_LAUNCH,
  type Phase5ComponentCatalog,
  type Phase5ComponentSelection,
  type Phase5LaunchStatus,
  type Phase5OperationResult,
  type Phase5RuntimeCatalog,
  type Phase5RuntimeIntent,
  type Phase5RuntimeStatus,
} from "./generated/ipc-contracts";

export interface MinecraftLogChunk {
  text: string;
  nextOffset: number;
  fileSize: number;
}

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function desktopOnly() {
  return {
    code: "runtime_desktop_runtime_required",
    messageKey: "error.runtime_desktop_runtime_required",
    params: {},
  };
}

const browserCatalog: Phase5RuntimeCatalog = {
  minecraftVersions: [],
  fabricVersions: [],
  selectedMinecraftJavaMajor: null,
  neoforgeCapability: {
    capabilityId: "runtime.neoforge",
    state: "unconfigured",
    reasonCode: "runtime_desktop_runtime_required",
  },
  s9labComponentCapability: {
    capabilityId: "s9lab.components",
    state: "unconfigured",
    reasonCode: "component_provider_origin_unconfigured",
  },
};

export const runtimeCommands = {
  catalog: (minecraftVersion: string | null = null): Promise<Phase5RuntimeCatalog> =>
    isTauriRuntime()
      ? invoke<Phase5RuntimeCatalog>(PHASE5_RUNTIME_CATALOG, { minecraftVersion })
      : Promise.reject(desktopOnly()),

  componentCatalog: (intent: Phase5RuntimeIntent): Promise<Phase5ComponentCatalog> =>
    isTauriRuntime()
      ? invoke<Phase5ComponentCatalog>(PHASE5_S9LAB_COMPONENT_CATALOG, { intent })
      : Promise.resolve({
          capability: structuredClone(browserCatalog.s9labComponentCapability),
          entries: [],
        }),

  status: (profileId: string): Promise<Phase5RuntimeStatus> =>
    isTauriRuntime()
      ? invoke<Phase5RuntimeStatus>(PHASE5_PROFILE_RUNTIME_STATUS, { profileId })
      : Promise.reject(desktopOnly()),

  install: (
    profileId: string,
    intent: Phase5RuntimeIntent,
    component: Phase5ComponentSelection = { mode: "disabled" },
  ): Promise<Phase5OperationResult> =>
    isTauriRuntime()
      ? invoke<Phase5OperationResult>(PHASE5_INSTALL_PROFILE, {
          profileId,
          intent,
          component,
        })
      : Promise.reject(desktopOnly()),

  repair: (profileId: string): Promise<Phase5OperationResult> =>
    isTauriRuntime()
      ? invoke<Phase5OperationResult>(PHASE5_REPAIR_PROFILE, { profileId })
      : Promise.reject(desktopOnly()),

  launch: (profileId: string, memoryMb = 4096): Promise<Phase5LaunchStatus> =>
    isTauriRuntime()
      ? invoke<Phase5LaunchStatus>(PHASE5_LAUNCH_PROFILE, { profileId, memoryMb })
      : Promise.reject(desktopOnly()),

  stop: (launchId: string): Promise<Phase5LaunchStatus> =>
    isTauriRuntime()
      ? invoke<Phase5LaunchStatus>(PHASE5_STOP_LAUNCH, { launchId })
      : Promise.reject(desktopOnly()),

  launchStatuses: (): Promise<Phase5LaunchStatus[]> =>
    isTauriRuntime()
      ? invoke<Phase5LaunchStatus[]>(PHASE5_LAUNCH_STATUSES)
      : Promise.resolve([]),

  logRead: (profileId: string, launchId: string, offset = 0): Promise<MinecraftLogChunk> =>
    isTauriRuntime()
      ? invoke<MinecraftLogChunk>("phase5_minecraft_log_read", { profileId, launchId, offset })
      : Promise.resolve({ text: "", nextOffset: offset, fileSize: 0 }),

  logExport: (profileId: string, launchId: string): Promise<string> =>
    isTauriRuntime()
      ? invoke<string>("phase5_minecraft_log_export", { profileId, launchId })
      : Promise.reject(desktopOnly()),

  logOpen: async (profileId: string, launchId: string, accountName: string): Promise<void> => {
    if (!isTauriRuntime()) throw desktopOnly();
    const args = { profileId, launchId, accountName };
    try {
      await invoke<void>("phase5_minecraft_log_window_open", args);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error ?? "");
      if (!message.toLowerCase().includes("command") || !message.toLowerCase().includes("not found")) throw error;
      await invoke<void>("minecraft_log_window_open", args);
    }
  },

  setComponent: (
    profileId: string,
    selection: Phase5ComponentSelection,
  ): Promise<Phase5OperationResult> =>
    isTauriRuntime()
      ? invoke<Phase5OperationResult>(PHASE5_SET_S9LAB_COMPONENT, {
          profileId,
          selection,
        })
      : Promise.reject(desktopOnly()),
};
