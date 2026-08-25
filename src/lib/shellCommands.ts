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
  const parse = (value: unknown): TypedIpcError | null => {
    if (!value || typeof value !== "object") return null;
    const candidate = value as Partial<TypedIpcError>;
    return typeof candidate.code === "string"
      && typeof candidate.messageKey === "string"
      && candidate.params != null
      && typeof candidate.params === "object"
      ? { code: candidate.code, messageKey: candidate.messageKey, params: candidate.params }
      : null;
  };

  const direct = parse(error);
  if (direct) return direct;

  // Tauri may reject an invoke with the serialized Rust error as a string
  // (or as Error.message, depending on the WebView runtime). Decode both
  // representations so callers never lose the actionable safe error code.
  const serialized = typeof error === "string"
    ? error
    : error instanceof Error
      ? error.message
      : null;
  if (!serialized) return null;
  try {
    return parse(JSON.parse(serialized));
  } catch {
    return null;
  }
}
