import { resolveAccentPalette } from "./accent";
import type { Appearance, ShellSettings } from "./types";

export function resolveAppearance(appearance: Appearance, prefersDark: boolean): Exclude<Appearance, "system"> {
  return appearance === "system" ? (prefersDark ? "dark" : "light") : appearance;
}

export function applyShellTheme(settings: ShellSettings, root: HTMLElement = document.documentElement): void {
  const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const systemReduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const palette = resolveAccentPalette(settings.accentColor);
  root.dataset.theme = resolveAppearance(settings.appearance, prefersDark);
  root.dataset.density = settings.density;
  root.dataset.navigation = settings.navigationMode;
  root.dataset.background = settings.backgroundVariant;
  root.dataset.reducedMotion = String(settings.reducedMotion || systemReduced);
  root.style.setProperty("--color-accent", palette.accent);
  root.style.setProperty("--color-on-accent", palette.onAccent);
  root.style.setProperty("--color-accent-hover", palette.hover);
  root.style.setProperty("--color-accent-pressed", palette.pressed);
  root.style.setProperty("--color-focus", palette.focus);
}
