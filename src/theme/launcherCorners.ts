export const DEFAULT_LAUNCHER_CORNER_RADIUS = 10;
export const MIN_LAUNCHER_CORNER_RADIUS = 0;
export const MAX_LAUNCHER_CORNER_RADIUS = 24;
const STORAGE_KEY = "snine.launcher.cornerRadius";

function clamp(value: number) {
  return Math.min(MAX_LAUNCHER_CORNER_RADIUS, Math.max(MIN_LAUNCHER_CORNER_RADIUS, value));
}

export function normalizeLauncherCornerRadius(value: unknown) {
  const parsed = typeof value === "number" ? value : Number.parseFloat(String(value ?? ""));
  if (!Number.isFinite(parsed)) return DEFAULT_LAUNCHER_CORNER_RADIUS;
  return Math.round(clamp(parsed));
}

function setLauncherCornerRadius(radius: number) {
  document.documentElement.style.setProperty("--snine-corner-radius", `${radius}px`);
}

export function loadLauncherCornerRadius() {
  try {
    return normalizeLauncherCornerRadius(window.localStorage.getItem(STORAGE_KEY));
  } catch {
    return DEFAULT_LAUNCHER_CORNER_RADIUS;
  }
}

export function applyLauncherCornerRadius(value = loadLauncherCornerRadius()) {
  const radius = normalizeLauncherCornerRadius(value);
  setLauncherCornerRadius(radius);
  return radius;
}

export function saveLauncherCornerRadius(value: number) {
  const radius = normalizeLauncherCornerRadius(value);
  try {
    window.localStorage.setItem(STORAGE_KEY, String(radius));
  } catch {
    // Keep live preference even without persistence.
  }
  applyLauncherCornerRadius(radius);
  return radius;
}

export function resetLauncherCornerRadius() {
  try {
    window.localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Ignore storage errors and still restore the default.
  }
  applyLauncherCornerRadius(DEFAULT_LAUNCHER_CORNER_RADIUS);
  return DEFAULT_LAUNCHER_CORNER_RADIUS;
}
