import { loadSNinePlayerSkin, type LauncherSkinSnapshot } from "./snineClientBridge";

const READY_TTL_MS = 5 * 60_000;
const FAILURE_TTL_MS = 15_000;

interface SkinCacheEntry {
  expiresAt: number;
  promise: Promise<LauncherSkinSnapshot>;
}

const skinCache = new Map<string, SkinCacheEntry>();
const avatarCache = new Map<string, Promise<string>>();

function normalizedAccountKey(accountId?: string | null, username?: string | null): string {
  const id = (accountId ?? "").replaceAll("-", "").trim().toLowerCase();
  const name = (username ?? "").trim().toLowerCase();
  return `${id}:${name}`;
}

function fingerprint(value: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

export function loadMinecraftSkin(
  accountId?: string | null,
  username?: string | null,
  force = false,
): Promise<LauncherSkinSnapshot> {
  const key = normalizedAccountKey(accountId, username);
  const now = Date.now();
  const cached = skinCache.get(key);
  if (!force && cached && cached.expiresAt > now) return cached.promise;

  const promise = loadSNinePlayerSkin(accountId, username).then((snapshot) => {
    const current = skinCache.get(key);
    if (current?.promise === promise) {
      current.expiresAt = Date.now() + (snapshot.ok && snapshot.textureDataUrl ? READY_TTL_MS : FAILURE_TTL_MS);
    }
    return snapshot;
  });
  skinCache.set(key, { expiresAt: now + FAILURE_TTL_MS, promise });
  return promise;
}

export function invalidateMinecraftSkin(accountId?: string | null): void {
  const compactId = (accountId ?? "").replaceAll("-", "").trim().toLowerCase();
  for (const key of skinCache.keys()) {
    if (!compactId || key.startsWith(`${compactId}:`)) skinCache.delete(key);
  }
  for (const key of avatarCache.keys()) {
    if (!compactId || key.startsWith(`${compactId}:`)) avatarCache.delete(key);
  }
}

function loadImage(source: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("minecraft_skin_image_decode_failed"));
    image.src = source;
  });
}

export async function composeMinecraftHead(textureDataUrl: string, size = 64): Promise<string> {
  const image = await loadImage(textureDataUrl);
  if (image.naturalWidth < 64 || image.naturalHeight < 32) {
    throw new Error("minecraft_skin_dimensions_invalid");
  }
  const safeSize = Math.max(16, Math.min(256, Math.round(size)));
  const canvas = document.createElement("canvas");
  canvas.width = safeSize;
  canvas.height = safeSize;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("minecraft_avatar_canvas_unavailable");
  context.imageSmoothingEnabled = false;
  context.clearRect(0, 0, safeSize, safeSize);
  context.drawImage(image, 8, 8, 8, 8, 0, 0, safeSize, safeSize);
  context.drawImage(image, 40, 8, 8, 8, 0, 0, safeSize, safeSize);
  return canvas.toDataURL("image/png");
}

export function minecraftHeadFromSnapshot(
  accountId: string,
  snapshot: LauncherSkinSnapshot,
  size = 64,
): Promise<string> {
  if (!snapshot.textureDataUrl) return Promise.reject(new Error("minecraft_skin_missing"));
  const compactId = accountId.replaceAll("-", "").trim().toLowerCase();
  const key = `${compactId}:${fingerprint(snapshot.textureDataUrl)}:${snapshot.model}:${size}:overlay`;
  const cached = avatarCache.get(key);
  if (cached) return cached;
  const promise = composeMinecraftHead(snapshot.textureDataUrl, size).catch((error) => {
    avatarCache.delete(key);
    throw error;
  });
  avatarCache.set(key, promise);
  return promise;
}

export function resetMinecraftSkinCachesForTests(): void {
  skinCache.clear();
  avatarCache.clear();
}
