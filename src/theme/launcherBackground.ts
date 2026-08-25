export const DEFAULT_LAUNCHER_BACKGROUND = "#272727";
const STORAGE_KEY = "snine.launcher.backgroundColor";

export function normalizeLauncherBackground(value: string | null | undefined): string {
  const candidate = String(value ?? "").trim();
  return /^#[0-9a-fA-F]{6}$/.test(candidate) ? candidate.toLowerCase() : DEFAULT_LAUNCHER_BACKGROUND;
}

export function loadLauncherBackground(): string {
  try {
    return normalizeLauncherBackground(window.localStorage.getItem(STORAGE_KEY));
  } catch {
    return DEFAULT_LAUNCHER_BACKGROUND;
  }
}

export function applyLauncherBackground(value = loadLauncherBackground()): string {
  const normalized = normalizeLauncherBackground(value);
  document.documentElement.style.setProperty("--snine-background", normalized);
  return normalized;
}

export function saveLauncherBackground(value: string): string {
  const normalized = normalizeLauncherBackground(value);
  try {
    window.localStorage.setItem(STORAGE_KEY, normalized);
  } catch {
    // A blocked localStorage must not make the settings page unusable.
  }
  return applyLauncherBackground(normalized);
}

export function resetLauncherBackground(): string {
  try {
    window.localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Ignore storage failures and still reset the active CSS variable.
  }
  return applyLauncherBackground(DEFAULT_LAUNCHER_BACKGROUND);
}
