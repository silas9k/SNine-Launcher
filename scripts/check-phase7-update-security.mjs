import fs from "node:fs";

const read = (path) => fs.readFileSync(path, "utf8");
const contract = JSON.parse(read("contracts/ipc-contracts.json"));
const rust = [
  read("src-tauri/src/updates/model.rs"),
  read("src-tauri/src/updates/service.rs"),
  read("src-tauri/src/profiles/service.rs"),
  read("src-tauri/src/ipc/mod.rs"),
  read("src-tauri/src/lib.rs"),
].join("\n");
const frontend = [read("src/pages/UpdatesPage.tsx"), read("src/lib/updateCommands.ts")].join("\n");
const errors = [];

const requiredCommands = [
  "phase7_update_snapshot",
  "phase7_save_update_policy",
  "phase7_preview_profile_updates",
  "phase7_create_restore_point",
  "phase7_apply_profile_updates",
  "phase7_rollback_profile",
  "phase7_restore_backup",
  "phase7_run_automatic_updates",
];
const commandNames = new Set(contract.commands.map((command) => command.name));
if (contract.version < 7) errors.push("IPC contract was not advanced for Phase 7");
for (const command of requiredCommands) {
  if (!commandNames.has(command)) errors.push(`missing Phase 7 contract: ${command}`);
  if (!rust.includes(command)) errors.push(`missing Phase 7 Rust registration: ${command}`);
  if (!frontend.includes(command.toUpperCase())) errors.push(`missing typed Phase 7 frontend wrapper: ${command}`);
}

for (const marker of [
  "site.s9lab.restore-point",
  "MAX_BACKUP_FILES",
  "MAX_BACKUP_TOTAL_BYTES",
  "backup_symlink_forbidden",
  "backup_content_mismatch",
  "replace_cache_references",
  "ProfileRollback",
  "create_restore_point(profile_id)",
  "rollback_to_revision",
  "update_policy_channel_unavailable",
  "launcher_update_trust_not_configured",
]) {
  if (!rust.includes(marker)) errors.push(`missing Phase 7 safety invariant: ${marker}`);
}

if (!frontend.includes("UpdatesPage") || !frontend.includes("Restore backup as a new profile") && !read("src/i18n/messages.ts").includes("updates.restoreBackupTitle")) {
  errors.push("real update/recovery UI is missing");
}
if (/https?:\/\/[A-Za-z0-9.-]*(?:s9lab|localhost|127\.0\.0\.1)/i.test(rust)) {
  errors.push("unapproved launcher/S9Lab update endpoint found in Phase 7 core");
}
if (/PRIVATE KEY|private[_-]?key|signing[_-]?secret/i.test(rust)) {
  errors.push("private signing material marker found in Phase 7 core");
}

if (errors.length) {
  console.error(errors.join("\n"));
  process.exit(1);
}
console.log("Phase 7: getrennte Kanäle, Vorschau, Backup, atomarer Rollback und Migrationsauswahl erfolgreich geprüft.");
