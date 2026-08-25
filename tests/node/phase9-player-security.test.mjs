import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { spawnSync } from "node:child_process";

test("phase9 project passes the integrated player guard", () => {
  const result = spawnSync(process.execPath, ["scripts/check-phase9-player-security.mjs"], { encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr || result.stdout);
});

test("phase9 player uses the active-account cache and disposes WebGL resources", () => {
  const preview = fs.readFileSync("src/components/player/LauncherSkinPreview.tsx", "utf8");
  const renderer = fs.readFileSync("src/components/player/launcherSkinRenderer.ts", "utf8");
  const cache = fs.readFileSync("src/lib/minecraftSkinCache.ts", "utf8");
  assert.match(preview, /loadMinecraftSkin\(accountId, playerName/);
  assert.match(preview, /launcher-skin__nametag/);
  assert.match(preview, /launcherBadgeIconUrl/);
  assert.doesNotMatch(preview, /minecraftHeadFromSnapshot/);
  assert.doesNotMatch(preview, /snine\.active\.skin/);
  assert.match(cache, /40, 8, 8, 8/);
  assert.match(renderer, /deleteBuffer/);
  assert.match(renderer, /deleteTexture/);
  assert.match(renderer, /removeEventListener/);
  assert.match(renderer, /visibilitychange/);
});

test("phase9 keeps unverified cosmetic ownership visibly separate", () => {
  const source = fs.readFileSync("src/pages/CosmeticsPage.tsx", "utf8");
  assert.match(source, /ownershipUnavailable/);
  assert.match(source, /localPreview/);
  assert.doesNotMatch(source, /owned\s*[:=]\s*true/i);
});
