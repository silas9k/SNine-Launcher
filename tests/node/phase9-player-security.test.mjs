import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { spawnSync } from "node:child_process";

test("phase9 project passes the integrated player guard", () => {
  const result = spawnSync(process.execPath, ["scripts/check-phase9-player-security.mjs"], { encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr || result.stdout);
});

test("phase9 player uses only local preview assets and disables zoom", () => {
  const source = fs.readFileSync("src/components/player/PlayerStage.tsx", "utf8");
  assert.match(source, /data:image\/svg\+xml/);
  assert.match(source, /enableZoom = false/);
  assert.doesNotMatch(source.replaceAll('http://www.w3.org/2000/svg', ""), /https?:\/\/|fetch\s*\(|WebSocket/i);
});

test("phase9 keeps unverified cosmetic ownership visibly separate", () => {
  const source = fs.readFileSync("src/pages/CosmeticsPage.tsx", "utf8");
  assert.match(source, /ownershipUnavailable/);
  assert.match(source, /localPreview/);
  assert.doesNotMatch(source, /owned\s*[:=]\s*true/i);
});
