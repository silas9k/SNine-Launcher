import fs from "node:fs";

const read = (path) => fs.readFileSync(path, "utf8");
const player = read("src/components/player/PlayerStage.tsx");
const cosmetics = read("src/pages/CosmeticsPage.tsx");
const home = read("src/pages/HomePage.tsx");
const browserHarness = read("scripts/run-phase2-browser-tests.mjs");
const errors = [];

for (const marker of [
  'import("skinview3d")',
  "LOCAL_SKIN",
  "LOCAL_CAPE",
  "enableZoom = false",
  "enablePan = false",
  '"elytra"',
  'model: "auto-detect"',
  "setOuterLayerVisible",
  "resetCameraPose",
  "WebGLRenderingContext",
  'state, setState] = useState<"loading" | "ready" | "fallback">',
]) {
  if (!player.includes(marker)) errors.push(`missing Phase 9 player invariant: ${marker}`);
}

if (!home.includes("<PlayerStage />")) errors.push("integrated player is missing from the home layout");
if (!browserHarness.includes(".player-render-anchor")) errors.push("responsive browser gate does not cover the real player stage");
if (!cosmetics.includes("ownershipUnavailable") || !cosmetics.includes("previewAsset")) {
  errors.push("cosmetic ownership boundary is not explicit");
}
const networkSurface = (player + cosmetics).replaceAll('http://www.w3.org/2000/svg', "");
if (/https?:\/\//i.test(networkSurface)) errors.push("remote player or cosmetic asset URL found");
if (/\bfetch\s*\(|XMLHttpRequest|WebSocket/i.test(player + cosmetics)) errors.push("uncontrolled player network path found");
if (/enableZoom\s*=\s*true/i.test(player)) errors.push("player zoom was enabled");

if (errors.length) {
  console.error(errors.join("\n"));
  process.exit(1);
}
console.log("Phase 9: lokale Assets, kein Zoom, vollständige Steuerung und ehrliche Cosmetic-Grenze erfolgreich geprüft.");
