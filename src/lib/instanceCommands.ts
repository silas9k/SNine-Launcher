import { invoke } from "@tauri-apps/api/core";
import type { Phase4Profile, Phase5LaunchStatus, Phase5RuntimeStatus } from "./generated/ipc-contracts";

export interface InstanceSettings {
  formatVersion: 1;
  icon: string;
  minRamMb: number;
  maxRamMb: number;
  jvmArguments: string[];
  width: number;
  height: number;
  fullscreen: boolean;
  customJavaExecutable: string | null;
  lastPlayedAtUnix: number | null;
  shareResourcepacks: boolean;
  shareWorlds: boolean;
  shareShaderpacks: boolean;
  shareOptions: boolean;
}

export const DEFAULT_INSTANCE_SETTINGS: InstanceSettings = {
  formatVersion: 1,
  icon: "grass-block",
  minRamMb: 512,
  maxRamMb: 4096,
  jvmArguments: [],
  width: 1280,
  height: 720,
  fullscreen: false,
  customJavaExecutable: null,
  lastPlayedAtUnix: null,
  shareResourcepacks: false,
  shareWorlds: false,
  shareShaderpacks: false,
  shareOptions: false,
};

export interface ProfileWorkspaceEntry {
  profile: Phase4Profile;
  runtime: Phase5RuntimeStatus;
  settings: InstanceSettings;
}

export interface ProfilesWorkspace {
  entries: ProfileWorkspaceEntry[];
  launches: Phase5LaunchStatus[];
}

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function desktopOnly() {
  return Promise.reject({ code: "runtime_desktop_runtime_required", messageKey: "error.runtime_desktop_runtime_required", params: {} });
}

export const instanceCommands = {
  workspace: (): Promise<ProfilesWorkspace> =>
    isTauriRuntime() ? invoke<ProfilesWorkspace>("phase5_profiles_workspace") : Promise.resolve({ entries: [], launches: [] }),
  settings: (profileId: string): Promise<InstanceSettings> =>
    isTauriRuntime() ? invoke("phase5_instance_settings", { profileId }) : Promise.resolve({ ...DEFAULT_INSTANCE_SETTINGS }),
  saveSettings: (profileId: string, settings: InstanceSettings): Promise<InstanceSettings> =>
    isTauriRuntime() ? invoke("phase5_save_instance_settings", { profileId, settings }) : desktopOnly(),
  rename: (profileId: string, displayName: string): Promise<Phase4Profile> =>
    isTauriRuntime() ? invoke("phase4_rename_profile", { profileId, displayName }) : desktopOnly(),
  launch: (profileId: string): Promise<Phase5LaunchStatus> =>
    isTauriRuntime() ? invoke("phase5_launch_instance", { profileId }) : desktopOnly(),
  openFolder: (profileId: string, folder: "game" | "mods" | "resourcepacks" | "worlds" | "shaderpacks" | "screenshots" | "logs"): Promise<void> =>
    isTauriRuntime() ? invoke("phase5_open_instance_folder", { profileId, folder }) : desktopOnly(),
};
