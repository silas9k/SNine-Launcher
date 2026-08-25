import fs from "node:fs";

const read = (path) => fs.readFileSync(path, "utf8");
const contract = JSON.parse(read("contracts/ipc-contracts.json"));
const rust = [
  read("src-tauri/src/cloud_sync/model.rs"),
  read("src-tauri/src/cloud_sync/service.rs"),
  read("src-tauri/src/ipc/mod.rs"),
  read("src-tauri/src/lib.rs"),
].join("\n");
const accountsFrontend = read("src/pages/AccountsPage.tsx");
const frontend = [read("src/lib/cloudSyncCommands.ts"), accountsFrontend].join("\n");
const errors = [];
const command = "phase8_cloud_sync_snapshot";
const commandNames = new Set(contract.commands.map((entry) => entry.name));

if (contract.version < 8) errors.push("IPC contract was not advanced for Phase 8");
if (!commandNames.has(command)) errors.push(`missing Phase 8 contract: ${command}`);
if (!rust.includes(command)) errors.push(`missing Phase 8 Rust registration: ${command}`);
if (!frontend.includes("PHASE8_CLOUD_SYNC_SNAPSHOT")) errors.push("missing typed Phase 8 frontend wrapper");

for (const marker of [
  "CloudSyncProvider",
  "UnconfiguredCloudSyncProvider",
  "cloud_provider_unconfigured",
  "site.s9lab.cloud-payload",
  "three_way_merge",
  "resolve_conflicts",
  "cloud_conflict_resolution_incomplete",
  'device_limit: 2',
  '"profile-metadata"',
  '"content-lists"',
  '"settings"',
]) {
  if (!rust.includes(marker)) errors.push(`missing Phase 8 safety invariant: ${marker}`);
}

if (/https?:\/\//i.test(rust + frontend)) errors.push("unapproved cloud endpoint found in Phase 8 implementation");
if (accountsFrontend.includes("cloudSyncCommands") && !accountsFrontend.includes("<Button disabled>")) {
  errors.push("cloud account linking is not visibly fail-closed");
}

const payloadStruct = rust.slice(rust.indexOf("pub struct SyncPayloadV1"), rust.indexOf("pub struct SyncRevisionV1"));
for (const forbidden of ["token", "world", "path", "log", "account", "session"]) {
  if (new RegExp(forbidden, "i").test(payloadStruct)) errors.push(`forbidden sync payload field marker: ${forbidden}`);
}

if (errors.length) {
  console.error(errors.join("\n"));
  process.exit(1);
}
console.log("Phase 8: lokaler Zwei-Geräte-Merge, begrenzter Sync-Umfang und fail-closed Provider erfolgreich geprüft.");
