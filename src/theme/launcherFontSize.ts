import { uiStorage } from "../lib/uiStorage";
export const DEFAULT_LAUNCHER_FONT_SCALE = 1;
export const MIN_LAUNCHER_FONT_SCALE = 0.8;
export const MAX_LAUNCHER_FONT_SCALE = 1.6;
const STORAGE_KEY = "snine.launcher.fontScale";

function clamp(value: number) {
  return Math.min(MAX_LAUNCHER_FONT_SCALE, Math.max(MIN_LAUNCHER_FONT_SCALE, value));
}

export function normalizeLauncherFontScale(value: unknown) {
  const parsed = typeof value === "number" ? value : Number.parseFloat(String(value ?? ""));
  if (!Number.isFinite(parsed)) return DEFAULT_LAUNCHER_FONT_SCALE;
  return Math.round(clamp(parsed) * 100) / 100;
}

function setLauncherFontScale(scale: number) {
  document.documentElement.style.setProperty("--snine-font-scale", String(scale));
}

export function loadLauncherFontScale() {
  try {
    return normalizeLauncherFontScale(uiStorage.getItem(STORAGE_KEY));
  } catch {
    return DEFAULT_LAUNCHER_FONT_SCALE;
  }
}

export function applyLauncherFontScale(value = loadLauncherFontScale()) {
  const scale = normalizeLauncherFontScale(value);
  setLauncherFontScale(scale);
  return scale;
}

export function saveLauncherFontScale(value: number) {
  const scale = normalizeLauncherFontScale(value);
  try {
    uiStorage.setItem(STORAGE_KEY, String(scale));
  } catch {
    // Keep live preference even without persistence.
  }
  applyLauncherFontScale(scale);
  return scale;
}

export function resetLauncherFontScale() {
  try {
    uiStorage.removeItem(STORAGE_KEY);
  } catch {
    // Ignore storage errors and still restore the default.
  }
  applyLauncherFontScale(DEFAULT_LAUNCHER_FONT_SCALE);
  return DEFAULT_LAUNCHER_FONT_SCALE;
}
