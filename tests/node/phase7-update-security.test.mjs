import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { spawnSync } from "node:child_process";

test("phase7 project passes the complete update and recovery guard", () => {
  const result = spawnSync(process.execPath, ["scripts/check-phase7-update-security.mjs"], { encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr || result.stdout);
});

test("phase7 public contracts expose selections but no raw updater URL or signing secret", () => {
  const contract = fs.readFileSync("contracts/ipc-contracts.json", "utf8");
  const phase7 = contract.slice(contract.indexOf('"Phase7UpdatePolicy"'));
  assert.match(phase7, /includeAccount/);
  assert.match(phase7, /includeSettings/);
  assert.match(phase7, /includeFiles/);
  assert.doesNotMatch(phase7, /privateKey|signingSecret|downloadUrl|updateUrl|token/i);
});

test("blocked production channels cannot be switched to automatic through IPC", () => {
  const service = fs.readFileSync("src-tauri/src/updates/service.rs", "utf8");
  assert.match(service, /policy\.launcher == UpdateMode::Automatic/);
  assert.match(service, /policy\.s9lab_component == UpdateMode::Automatic/);
  assert.match(service, /update_policy_channel_unavailable/);
});
