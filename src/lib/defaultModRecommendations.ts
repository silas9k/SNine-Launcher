export type LoaderKind = "vanilla" | "fabric" | "neoforge";

export interface DefaultModRecommendation {
  projectId: string;
  title: string;
  reason: string;
}

export const SNINE_STANDARD_MODRINTH_PROJECTS = [
  "P7dR8mSH", // Fabric API
  "AANobbMI", // Sodium
  "gvQqBUqZ", // Lithium
  "uXXizFIs", // FerriteCore
  "mOgUt4GM", // Mod Menu
  "5ZwdcRci", // ImmediatelyFast
  "NNAgCjsB", // Entity Culling
] as const;

export const RECOMMENDED_SAFE_DEFAULTS: Record<LoaderKind, DefaultModRecommendation[]> = {
  fabric: [
    { projectId: "AANobbMI", title: "Sodium", reason: "Critical performance and rendering stability" },
    { projectId: "P7dR8mSH", title: "Fabric API", reason: "Required compatibility layer for Fabric mods" },
  ],
  vanilla: [
    { projectId: "AANobbMI", title: "Sodium", reason: "Performance-oriented baseline for stable gameplay" },
  ],
  neoforge: [
    { projectId: "o7cnmI0k", title: "Embeddium", reason: "NeoForge-compatible optimization baseline" },
  ],
};

export function recommendedSafeDefaultsForLoader(loader: LoaderKind | null | undefined): DefaultModRecommendation[] {
  if (!loader) return [];
  return RECOMMENDED_SAFE_DEFAULTS[loader] ?? [];
}
