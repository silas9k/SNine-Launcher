import { uiStorage } from "../lib/uiStorage";
import { invoke } from "@tauri-apps/api/core";

export type LauncherFontChoice = "minecraft" | "launcher";

export const DEFAULT_LAUNCHER_FONT: LauncherFontChoice = "minecraft";
const STORAGE_KEY = "snine.launcher.font";
const FONT_CACHE_KEY = "snine.launcher.minecraftFontDataUrl.v1";
const FONT_STYLE_ID = "snine-minecraft-font-face";

const minecraftFamily = '"SNine Minecraft", "Minecraft", "Minecraftia", ui-monospace, monospace';
const launcherFamily = 'Inter, "Segoe UI Variable", "Segoe UI", system-ui, sans-serif';

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function normalizeLauncherFont(value: string | null | undefined): LauncherFontChoice {
  return value === "launcher" ? "launcher" : "minecraft";
}

export function loadLauncherFont(): LauncherFontChoice {
  try {
    return normalizeLauncherFont(uiStorage.getItem(STORAGE_KEY));
  } catch {
    return DEFAULT_LAUNCHER_FONT;
  }
}

function setFontFamily(choice: LauncherFontChoice) {
  document.documentElement.style.setProperty(
    "--snine-font-family",
    choice === "minecraft" ? minecraftFamily : launcherFamily,
  );
}

async function ensureMinecraftFontFace() {
  if (document.getElementById(FONT_STYLE_ID)) return;

  let dataUrl = "";
  try {
    dataUrl = uiStorage.getItem(FONT_CACHE_KEY) ?? "";
  } catch {
    // Local cache is optional.
  }

  if (!dataUrl && isTauriRuntime()) {
    try {
      dataUrl = await invoke<string>("launcher_minecraft_font_data_url");
      if (dataUrl.startsWith("data:font/ttf;base64,")) {
        try {
          uiStorage.setItem(FONT_CACHE_KEY, dataUrl);
        } catch {
          // The exact font still works for this session if uiStorage is blocked.
        }
      }
    } catch (error) {
      console.warn("[SNine Launcher] Minecraft font could not be loaded", error);
      return;
    }
  }

  if (!dataUrl.startsWith("data:font/ttf;base64,")) return;
  const style = document.createElement("style");
  style.id = FONT_STYLE_ID;
  style.textContent = `@font-face{font-family:"SNine Minecraft";src:url("${dataUrl}") format("truetype");font-style:normal;font-weight:400 900;font-display:swap;}`;
  document.head.appendChild(style);
}

export async function applyLauncherFont(value = loadLauncherFont()) {
  const choice = normalizeLauncherFont(value);
  setFontFamily(choice);
  if (choice === "minecraft") await ensureMinecraftFontFace();
  return choice;
}

export function saveLauncherFont(value: LauncherFontChoice) {
  const choice = normalizeLauncherFont(value);
  try {
    uiStorage.setItem(STORAGE_KEY, choice);
  } catch {
    // Keep the live selection even when persistence is unavailable.
  }
  void applyLauncherFont(choice);
  return choice;
}

export function resetLauncherFont() {
  try {
    uiStorage.removeItem(STORAGE_KEY);
  } catch {
    // Ignore storage failures and still restore the default.
  }
  void applyLauncherFont(DEFAULT_LAUNCHER_FONT);
  return DEFAULT_LAUNCHER_FONT;
}
