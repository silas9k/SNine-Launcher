import { invoke } from "@tauri-apps/api/core";

export interface LauncherCosmeticAsset {
  id: string;
  kind: string;
  name: string;
  textureDataUrl: string | null;
  model: unknown | null;
  definition: Record<string, unknown>;
}

export interface LauncherLiveSync {
  websocketUrl: string;
  sessionToken: string;
  playerUuid: string;
}


export interface LauncherLiveState {
  ok: boolean;
  online: boolean;
  equippedCosmetics: Record<string, string>;
  statusMessage: string;
}

export interface LauncherCosmeticSnapshot {
  ok: boolean;
  playerName: string;
  online: boolean;
  equipped: LauncherCosmeticAsset[];
  source: string;
  statusMessage: string;
  liveSync: LauncherLiveSync | null;
}

export interface LauncherSkinSnapshot {
  ok: boolean;
  playerName: string;
  textureDataUrl: string | null;
  model: "slim" | "classic";
  source: string;
  statusMessage: string;
}

const EMPTY_COSMETICS: LauncherCosmeticSnapshot = {
  ok: false,
  playerName: "SNine",
  online: false,
  equipped: [],
  source: "",
  statusMessage: "not_connected",
  liveSync: null,
};

const EMPTY_SKIN: LauncherSkinSnapshot = {
  ok: false,
  playerName: "SNine",
  textureDataUrl: null,
  model: "classic",
  source: "",
  statusMessage: "not_connected",
};

function hasTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

interface EmbeddedCosmeticItem {
  id: string;
  kind: string;
  name: string;
  texture?: string;
  model?: string;
  definition?: Record<string, unknown>;
}

interface OfflineLoadoutItem { id: string; kind: string; name?: string }
interface OfflineLoadoutFile {
  capturedAt?: string;
  players?: Record<string, { name?: string; equipped?: OfflineLoadoutItem[] }>;
}

const LEGACY_ASSET_ALIASES: Record<string, string> = {
  s9lab_cube_pet: "snine_cube_pet",
};

const SYNTHETIC_ASSETS: Record<string, Partial<LauncherCosmeticAsset>> = {
  s9lab_aurora_glint: {
    kind: "glint",
    name: "Aurora Glint",
    definition: { id: "s9lab_aurora_glint", type: "glint", rarity: "LEGENDARY", effect: "aurora" },
  },
  s9lab_back: {
    kind: "accessory",
    name: "Back",
    definition: { id: "s9lab_back", type: "accessory", legacy: true },
  },
};

let embeddedManifestPromise: Promise<Record<string, EmbeddedCosmeticItem>> | null = null;
let offlineLoadoutPromise: Promise<OfflineLoadoutFile> | null = null;

function isLauncherHiddenCosmetic(asset: LauncherCosmeticAsset): boolean {
  const kind = String(asset.kind || "").trim().toLowerCase();
  const id = String(asset.id || "").trim().toLowerCase();
  return kind === "glint" || id.includes("glint");
}


async function embeddedCosmetics(): Promise<Record<string, EmbeddedCosmeticItem>> {
  if (!embeddedManifestPromise) {
    embeddedManifestPromise = fetch("./snine-cosmetics/manifest.json", { cache: "force-cache" })
      .then(async (response) => {
        if (!response.ok) return {};
        const payload = await response.json() as { items?: Record<string, EmbeddedCosmeticItem> };
        return payload.items ?? {};
      })
      .catch(() => ({}));
  }
  return embeddedManifestPromise;
}

async function offlineLoadouts(): Promise<OfflineLoadoutFile> {
  if (!offlineLoadoutPromise) {
    offlineLoadoutPromise = fetch("./snine-cosmetics/offline-loadouts.json", { cache: "no-cache" })
      .then((response) => response.ok ? response.json() as Promise<OfflineLoadoutFile> : {})
      .catch(() => ({}));
  }
  return offlineLoadoutPromise;
}

export function compactMinecraftUuid(value: string): string {
  return value.replaceAll("-", "").trim().toLowerCase();
}

async function resolveEmbeddedAsset(asset: LauncherCosmeticAsset): Promise<LauncherCosmeticAsset> {
  const manifest = await embeddedCosmetics();
  const canonicalId = LEGACY_ASSET_ALIASES[asset.id] ?? asset.id;
  const fallback = manifest[canonicalId];
  const synthetic = SYNTHETIC_ASSETS[asset.id];

  if (!fallback && !synthetic) return asset;

  let model = asset.model;
  if (!model && fallback?.model) {
    model = await fetch(fallback.model, { cache: "force-cache" })
      .then((response) => response.ok ? response.json() : null)
      .catch(() => null);
  }

  const fallbackName = fallback?.name || synthetic?.name || asset.id;
  const fallbackKind = fallback?.kind || synthetic?.kind || "accessory";
  const originalKind = asset.kind?.trim().toLowerCase();
  const kind = !originalKind || originalKind === "unknown" || originalKind === "shoulder"
    ? fallbackKind
    : originalKind;

  return {
    ...asset,
    kind,
    name: asset.name && asset.name !== asset.id ? asset.name : fallbackName,
    textureDataUrl: asset.textureDataUrl || fallback?.texture || synthetic?.textureDataUrl || null,
    model,
    definition: {
      ...(fallback?.definition ?? {}),
      ...(synthetic?.definition ?? {}),
      ...(asset.definition ?? {}),
      sourceId: asset.id,
      canonicalAssetId: canonicalId,
    },
  };
}

async function enrichWithEmbeddedCosmetics(snapshot: LauncherCosmeticSnapshot): Promise<LauncherCosmeticSnapshot> {
  if (!snapshot.equipped.length) return snapshot;
  const resolved = await Promise.all(snapshot.equipped.map(resolveEmbeddedAsset));
  const equipped = resolved.filter((asset) => !isLauncherHiddenCosmetic(asset));
  return { ...snapshot, equipped };
}

function assetsFromEquippedMap(equipped: Record<string, string>): LauncherCosmeticAsset[] {
  return Object.entries(equipped)
    .filter(([kind, id]) => !["emote", "glint"].includes(kind.trim().toLowerCase()) && Boolean(id?.trim()) && !id.trim().toLowerCase().includes("glint"))
    .map(([kind, id]) => ({
      id: id.trim(),
      kind: kind.trim().toLowerCase(),
      name: id.trim(),
      textureDataUrl: null,
      model: null,
      definition: { id: id.trim(), type: kind.trim().toLowerCase(), livePush: true },
    }));
}

async function loadOfflineSnapshot(accountId: string, username: string): Promise<LauncherCosmeticSnapshot | null> {
  const data = await offlineLoadouts();
  const profile = data.players?.[compactMinecraftUuid(accountId)];
  if (!profile?.equipped?.length) return null;

  const equipped = await Promise.all(profile.equipped.map((item) => resolveEmbeddedAsset({
    id: item.id,
    kind: item.kind,
    name: item.name || item.id,
    textureDataUrl: null,
    model: null,
    definition: { id: item.id, type: item.kind, offlineSnapshot: true },
  })));

  return {
    ok: true,
    playerName: profile.name || username,
    online: false,
    equipped,
    source: "snine-offline-profile-snapshot",
    statusMessage: data.capturedAt ? `offline_snapshot_${data.capturedAt}` : "offline_snapshot",
    liveSync: null,
  };
}

export async function resolveSNineLiveCosmetics(
  equippedCosmetics: Record<string, string>,
  profileId?: string | null,
): Promise<LauncherCosmeticAsset[]> {
  const fallbackSnapshot: LauncherCosmeticSnapshot = {
    ...EMPTY_COSMETICS,
    ok: true,
    equipped: assetsFromEquippedMap(equippedCosmetics),
    source: "snine-websocket-live",
    statusMessage: "live_push_received",
  };

  if (!hasTauri()) {
    return (await enrichWithEmbeddedCosmetics(fallbackSnapshot)).equipped;
  }

  try {
    const assets = await invoke<LauncherCosmeticAsset[]>("snine_launcher_resolve_cosmetics", {
      equippedCosmetics,
      profileId: profileId || null,
    });
    return (await enrichWithEmbeddedCosmetics({ ...fallbackSnapshot, equipped: assets })).equipped;
  } catch {
    return (await enrichWithEmbeddedCosmetics(fallbackSnapshot)).equipped;
  }
}

export async function loadSNineLauncherCosmetics(
  accountId?: string | null,
  username?: string | null,
  profileId?: string | null,
): Promise<LauncherCosmeticSnapshot> {
  if (!accountId || !username || !hasTauri()) {
    return { ...EMPTY_COSMETICS, playerName: username || "SNine" };
  }

  try {
    const snapshot = await invoke<LauncherCosmeticSnapshot>("snine_launcher_cosmetics", {
      accountId,
      username,
      profileId: profileId || null,
    });
    const enriched = await enrichWithEmbeddedCosmetics({ ...snapshot, liveSync: snapshot.liveSync ?? null });
    if (enriched.equipped.length || enriched.online || enriched.liveSync) return enriched;
    return (await loadOfflineSnapshot(accountId, username)) ?? enriched;
  } catch (error) {
    const offline = await loadOfflineSnapshot(accountId, username);
    if (offline) return offline;
    return {
      ...EMPTY_COSMETICS,
      playerName: username,
      statusMessage: error instanceof Error ? error.message : String(error || "cosmetic_sync_failed"),
    };
  }
}

export async function loadSNinePlayerSkin(
  accountId?: string | null,
  username?: string | null,
): Promise<LauncherSkinSnapshot> {
  if (!accountId || !username || !hasTauri()) {
    return { ...EMPTY_SKIN, playerName: username || "SNine" };
  }
  try {
    const result = await invoke<LauncherSkinSnapshot>("snine_launcher_player_skin", { accountId, username });
    const normalized = {
      ...result,
      model: result.model === "slim" ? "slim" as const : "classic" as const,
    };
    if (!normalized.ok || !normalized.textureDataUrl) {
      console.warn("[SNine Launcher] Minecraft skin fallback", normalized.source, normalized.statusMessage);
    } else {
      console.info("[SNine Launcher] Minecraft skin loaded", normalized.source, normalized.playerName, normalized.model);
    }
    return normalized;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error || "skin_sync_failed");
    console.warn("[SNine Launcher] Minecraft skin IPC failed", message);
    return {
      ...EMPTY_SKIN,
      playerName: username,
      statusMessage: message,
    };
  }
}

export async function importSNinePlayerSkin(reference: string): Promise<LauncherSkinSnapshot> {
  if (!hasTauri()) {
    return {
      ...EMPTY_SKIN,
      ok: true,
      playerName: reference,
      textureDataUrl: `https://mc-heads.net/skin/${encodeURIComponent(reference)}`,
      source: "browser-skin-proxy",
      statusMessage: "browser_preview",
    };
  }
  return invoke<LauncherSkinSnapshot>("snine_launcher_import_skin", { reference });
}


export async function pollSNineLauncherLiveState(live: LauncherLiveSync): Promise<LauncherLiveState> {
  if (!hasTauri()) {
    return { ok: false, online: false, equippedCosmetics: {}, statusMessage: "desktop_runtime_required" };
  }
  return invoke<LauncherLiveState>("snine_launcher_live_state", {
    sessionToken: live.sessionToken,
    playerUuid: live.playerUuid,
  });
}
