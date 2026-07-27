import { invoke } from "@tauri-apps/api/core";
import {
  PHASE2_SAVE_SHELL_SETTINGS,
  PHASE2_SHELL_BOOTSTRAP,
  type Phase2ShellBootstrap,
  type ShellSettings as GeneratedShellSettings,
  type TypedIpcError,
} from "./generated/ipc-contracts";
import { DEFAULT_SHELL_SETTINGS, type ShellSettings } from "../theme/types";

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function fromGenerated(settings: GeneratedShellSettings): ShellSettings {
  return settings;
}

function toGenerated(settings: ShellSettings): GeneratedShellSettings {
  return settings;
}

let browserSettings: ShellSettings = { ...DEFAULT_SHELL_SETTINGS };

export async function loadShellSettings(): Promise<ShellSettings> {
  if (!isTauriRuntime()) return { ...browserSettings };
  const result = await invoke<Phase2ShellBootstrap>(PHASE2_SHELL_BOOTSTRAP);
  return fromGenerated(result.settings);
}

export async function saveShellSettings(settings: ShellSettings): Promise<ShellSettings> {
  if (!isTauriRuntime()) {
    browserSettings = { ...settings };
    return { ...browserSettings };
  }
  const result = await invoke<Phase2ShellBootstrap>(PHASE2_SAVE_SHELL_SETTINGS, {
    settings: toGenerated(settings),
  });
  return fromGenerated(result.settings);
}

export function typedIpcError(error: unknown): TypedIpcError | null {
  if (!error || typeof error !== "object") return null;
  const candidate = error as Partial<TypedIpcError>;
  return typeof candidate.code === "string" && typeof candidate.messageKey === "string" && candidate.params != null
    ? { code: candidate.code, messageKey: candidate.messageKey, params: candidate.params }
    : null;
}
