export type Appearance = "system" | "light" | "dark" | "contrast";
export type LocaleSetting = "system" | "de" | "en";
export type Density = "compact" | "comfortable";
export type NavigationMode = "compact" | "expanded";
export type BackgroundVariant = "calm" | "grid" | "terrain";

export interface ShellSettings {
  appearance: Appearance;
  locale: LocaleSetting;
  accentColor: string;
  density: Density;
  navigationMode: NavigationMode;
  backgroundVariant: BackgroundVariant;
  reducedMotion: boolean;
}

export const DEFAULT_SHELL_SETTINGS: ShellSettings = {
  appearance: "dark",
  locale: "system",
  accentColor: "#8b5cf6",
  density: "comfortable",
  navigationMode: "expanded",
  backgroundVariant: "calm",
  reducedMotion: false,
};
