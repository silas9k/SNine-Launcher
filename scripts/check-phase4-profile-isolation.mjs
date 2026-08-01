import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const PHASE4_MIGRATION_SHA256 = "129419b811bc8bdc909c50b77a713ed01e00f4397897175ba75ec4a201d184fd";

const REQUIRED_MUTABLE_DIRECTORIES = [
  "instance/mods",
  "instance/config",
  "instance/saves",
  "instance/resourcepacks",
  "instance/shaderpacks",
  "instance/datapacks",
];

export function forbiddenCopyMechanisms(text) {
  const patterns = [
    ["hardlink", /\b(?:hard_link|hardlink)\s*\(/gi],
    ["reflink", /\b(?:reflink|clonefile)\s*\(/gi],
  ];
  return patterns.flatMap(([name, pattern]) => [...text.matchAll(pattern)].map(() => name));
}

export function permanentQuarantineDeletion(text) {
  const product = text.split("#[cfg(test)]", 1)[0];
  return /(?:remove_file|remove_dir_all|delete_quarantined|purge_quarantined)\s*\(/i.test(product);
}

export function latestSchemaVersion(text) {
  const match = text.match(/LATEST_SCHEMA_VERSION:\s*i64\s*=\s*(\d+)\b/);
  return match ? Number.parseInt(match[1], 10) : null;
}

export function phase4MigrationIsCanonical(text) {
  const match = text.match(
    /Migration\s*\{\s*version:\s*5,\s*name:\s*"phase4_profile_lifecycle_and_cache_quarantine",\s*sql:\s*r#"([\s\S]*?)"#,\s*\},/,
  );
  if (!match) return false;
  const normalizedSql = match[1].replaceAll("\r\n", "\n").trim();
  return crypto.createHash("sha256").update(normalizedSql).digest("hex") === PHASE4_MIGRATION_SHA256;
}

export function inspectPhase4ProfileIsolation(root = ".") {
  const errors = [];
  const read = (relative) => fs.readFileSync(path.join(root, relative), "utf8");
  const migrations = read("src-tauri/src/storage/migrations.rs");
  const profiles = read("src-tauri/src/profiles/service.rs");
  const profileModel = read("src-tauri/src/profiles/model.rs");
  const operations = read("src-tauri/src/operations/engine.rs");
  const operationModel = read("src-tauri/src/operations/model.rs");
  const cache = read("src-tauri/src/cache/mod.rs");
  const paths = read("src-tauri/src/app/paths.rs");
  const home = read("src/pages/HomePage.tsx");
  const contract = JSON.parse(read("contracts/ipc-contracts.json"));

  for (const [description, ok] of [
    ["SQLite-Schema ist älter als v5", (latestSchemaVersion(migrations) ?? 0) >= 5],
    ["bestehende Phase-4-Migration 5 wurde verändert", phase4MigrationIsCanonical(migrations)],
    ["Profil-Metadatenmigration fehlt", migrations.includes("profile_metadata")],
    ["Profil-Lineage-Migration fehlt", migrations.includes("profile_lineage")],
    ["Cache-Quarantänemigration fehlt", migrations.includes("cache_quarantine")],
    ["dauerhafte Cache-Löschung ist nicht ausdrücklich unkonfiguriert", migrations.includes("deletion_policy TEXT NOT NULL CHECK (deletion_policy = 'unconfigured')")],
    ["versioniertes Profilmanifest fehlt", profileModel.includes("ProfileManifestV1") && profileModel.includes("format_version")],
    ["versionierter Profillock fehlt", profileModel.includes("ProfileLockV1") && profileModel.includes("manifest_sha256")],
    ["profilbezogene Manifest-/Lock-Aktivierung fehlt", operations.includes("plan_profile_revision")],
    ["Rollback neuer Profile fehlt", operationModel.includes("cleanup_profile_on_rollback") && operations.includes("detach_and_delete_incomplete_profile")],
    ["verifizierte normale Kopie fehlt", profiles.includes("secure_fs::copy_new")],
    ["Cache-Quarantänewurzel fehlt", paths.includes("cache_quarantine") && paths.includes("cache/quarantine/sha256")],
    ["GC nutzt kein wiederholtes Referenz-Revalidieren", (cache.match(/discover_references\(\)/g) ?? []).length >= 3],
    ["prozessweite Cache-Mutationssperre fehlt", cache.includes("Arc<Mutex<()>>") && cache.includes("cache_mutation_lock_poisoned")],
    ["GC-Löschrichtlinie ist nicht unkonfiguriert", cache.includes('deletion_policy: "unconfigured"')],
    ["aktive Phase-4-Profile sind nicht in die Startseite integriert", home.includes("profileCommands.list()") && home.includes("lifecycleState === \"active\"")],
  ]) {
    if (!ok) errors.push(description);
  }

  for (const directory of REQUIRED_MUTABLE_DIRECTORIES) {
    if (!profiles.includes(`"${directory}"`)) {
      errors.push(`isoliertes veränderliches Profilverzeichnis fehlt: ${directory}`);
    }
  }
  for (const mechanism of forbiddenCopyMechanisms(profiles)) {
    errors.push(`unerlaubter ${mechanism} für veränderliche Profildaten`);
  }
  if (permanentQuarantineDeletion(cache)) {
    errors.push("dauerhafte Cache-Löschung ist vor Konfiguration der Sicherheitsfrist verboten");
  }

  const requiredCommands = [
    "phase4_list_profiles",
    "phase4_create_profile",
    "phase4_duplicate_profile",
    "phase4_archive_profile",
    "phase4_trash_profile",
    "phase4_restore_profile",
    "phase4_cache_gc_preview",
    "phase4_quarantine_unreferenced_cache",
  ];
  const commands = new Set(contract.commands.map((command) => command.name));
  for (const command of requiredCommands) {
    if (!commands.has(command)) errors.push(`versionierter Phase-4-IPC-Befehl fehlt: ${command}`);
  }
  return errors;
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  const errors = inspectPhase4ProfileIsolation();
  if (errors.length) {
    console.error(errors.join("\n"));
    process.exit(1);
  }
  console.log("Phase 4: Profilisolation, Revisionen, Quarantäne und fail-closed GC-Gates erfolgreich.");
}
