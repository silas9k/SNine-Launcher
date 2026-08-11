import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { spawnSync } from "node:child_process";

test("phase8 project passes the cloud synchronization guard", () => {
  const result = spawnSync(process.execPath, ["scripts/check-phase8-cloud-security.mjs"], { encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr || result.stdout);
});

test("phase8 public contract exposes a summary without secrets or arbitrary paths", () => {
  const contract = fs.readFileSync("contracts/ipc-contracts.json", "utf8");
  const phase8 = contract.slice(contract.indexOf('"Phase8CloudSyncSnapshot"'));
  assert.match(phase8, /providerState/);
  assert.match(phase8, /localRevision/);
  assert.match(phase8, /deviceLimit/);
  assert.doesNotMatch(phase8, /accessToken|refreshToken|sessionToken|worldPath|filePath|endpoint|url/i);
});

test("phase8 has no invented production endpoint or browser fallback connection", () => {
  const source = [
    fs.readFileSync("src-tauri/src/cloud_sync/service.rs", "utf8"),
    fs.readFileSync("src/lib/cloudSyncCommands.ts", "utf8"),
  ].join("\n");
  assert.doesNotMatch(source, /https?:\/\//i);
  assert.match(source, /cloud_provider_unconfigured/);
});
