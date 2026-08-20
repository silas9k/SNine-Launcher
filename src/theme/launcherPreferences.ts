import { invoke } from "@tauri-apps/api/core";

export interface LauncherPreferences {
  closeOnLaunch: boolean;
  showPreviewCosmetics: boolean;
  previewAnimations: boolean;
  discordRpc: boolean;
}

export const DEFAULT_LAUNCHER_PREFERENCES: LauncherPreferences = {
  closeOnLaunch: false,
  showPreviewCosmetics: true,
  previewAnimations: true,
  discordRpc: true,
};

const STORAGE_KEY = "snine.launcher.preferences.v1";
const CHANGE_EVENT = "snine-launcher-preferences-changed";

export function loadLauncherPreferences(): LauncherPreferences {
  if (typeof window === "undefined") return { ...DEFAULT_LAUNCHER_PREFERENCES };
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) || "{}") as Partial<LauncherPreferences>;
    return { ...DEFAULT_LAUNCHER_PREFERENCES, ...saved };
  } catch {
    return { ...DEFAULT_LAUNCHER_PREFERENCES };
  }
}

export function saveLauncherPreferences(preferences: LauncherPreferences): LauncherPreferences {
  const saved = { ...DEFAULT_LAUNCHER_PREFERENCES, ...preferences };
  localStorage.setItem(STORAGE_KEY, JSON.stringify(saved));
  window.dispatchEvent(new CustomEvent(CHANGE_EVENT, { detail: saved }));
  return saved;
}

export function resetLauncherPreferences(): LauncherPreferences {
  localStorage.removeItem(STORAGE_KEY);
  const defaults = { ...DEFAULT_LAUNCHER_PREFERENCES };
  window.dispatchEvent(new CustomEvent(CHANGE_EVENT, { detail: defaults }));
  return defaults;
}

export function subscribeLauncherPreferences(listener: (preferences: LauncherPreferences) => void): () => void {
  const onChange = (event: Event) => {
    listener((event as CustomEvent<LauncherPreferences>).detail ?? loadLauncherPreferences());
  };
  window.addEventListener(CHANGE_EVENT, onChange);
  return () => window.removeEventListener(CHANGE_EVENT, onChange);
}

export async function applyDiscordRpcPreference(enabled: boolean): Promise<void> {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
  await invoke("discord_rpc_set_enabled", { enabled });
}
