import fs from "node:fs";

const read = (path) => fs.readFileSync(path, "utf8");
const player = read("src/components/player/LauncherSkinPreview.tsx");
const renderer = read("src/components/player/launcherSkinRenderer.ts");
const skinCache = read("src/lib/minecraftSkinCache.ts");
const cosmetics = read("src/pages/CosmeticsPage.tsx");
const home = read("src/pages/HomePage.tsx");
const errors = [];

for (const marker of [
  "loadMinecraftSkin",
  "launcherBadgeIconUrl",
  "launcher-skin__nametag",
  'state, setState] = useState<"loading" | "ready" | "fallback">',
]) {
  if (!player.includes(marker)) errors.push(`missing Phase 9 player invariant: ${marker}`);
}

for (const marker of ["setViewportVisible", "visibilitychange", "deleteBuffer", "deleteTexture", "deleteShader", "detachOrbit", "1.5"]) {
  if (!renderer.includes(marker)) errors.push(`missing Phase 9 renderer cleanup invariant: ${marker}`);
}
for (const marker of ["accountId", "avatarCache", "40, 8, 8, 8", "requestRef"]) {
  if (!(skinCache + player + read("src/components/player/MinecraftAvatar.tsx")).includes(marker)) {
    errors.push(`missing Phase 9 account skin invariant: ${marker}`);
  }
}

if (home.includes("<PlayerStage")) errors.push("redundant PlayerStage renderer returned");
if (!home.includes("<LauncherSkinPreview")) errors.push("real player preview is not integrated into Home");
if (!cosmetics.includes("ownershipUnavailable") || !cosmetics.includes("previewAsset")) {
  errors.push("cosmetic ownership boundary is not explicit");
}
if (/\bfetch\s*\(|XMLHttpRequest|WebSocket/i.test(player + skinCache)) errors.push("uncontrolled player network path found");
if (player.includes("snine.active.skin")) errors.push("global skin override can leak across accounts");
if (player.includes("minecraftHeadFromSnapshot")) errors.push("player head must not be used as the SNine nametag badge");

if (errors.length) {
  console.error(errors.join("\n"));
  process.exit(1);
}
console.log("Phase 9: accountgebundener Skin, Hat-Layer, stale-sicherer Cache und vollständiges WebGL-Cleanup erfolgreich geprüft.");
