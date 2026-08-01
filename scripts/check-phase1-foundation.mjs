import fs from "node:fs";

const read = (file) => fs.readFileSync(file, "utf8");
const errors = [];
const lib = read("src-tauri/src/lib.rs");
for (const moduleName of ["platform", "storage", "operations", "security", "download", "cache", "foundation", "ipc"]) {
  if (!new RegExp(`^(?:pub )?mod ${moduleName};$`, "m").test(lib)) {
    errors.push(`Phase-1-Modul fehlt in lib.rs: ${moduleName}`);
  }
}

const paths = read("src-tauri/src/app/paths.rs");
for (const directory of ["data", "profiles", "cache", "staging", "operations", "migration", "backups", "logs", "launcher"]) {
  if (!paths.includes(`"${directory}"`)) errors.push(`Kontrollierter Pfad fehlt: ${directory}`);
}
for (const required of [
  "pub fn ensure_root(&self)",
  "pub fn create_registered_layout(&self, registry: &PathRegistry)",
  "validate_existing_chain(&anchor, &self.root)",
  "secure_fs::create_directories_within",
]) {
  if (!paths.includes(required)) errors.push(`Sicherer Pfad-Bootstrap fehlt: ${required}`);
}

const states = [
  "planned",
  "staging",
  "verifying",
  "ready-to-commit",
  "committing",
  "validating",
  "completed",
  "rolling-back",
  "rolled-back",
  "failed",
];
const model = read("src-tauri/src/operations/model.rs");
const migrations = read("src-tauri/src/storage/migrations.rs");
for (const state of states) {
  if (!model.includes(`"${state}"`)) errors.push(`Operationszustand fehlt im Modell: ${state}`);
  if (!migrations.includes(`'${state}'`)) errors.push(`Operationszustand fehlt im SQLite-Schema: ${state}`);
}

for (const file of [
  "src-tauri/src/platform/mod.rs",
  "src-tauri/src/storage/mod.rs",
  "src-tauri/src/operations/engine.rs",
  "src-tauri/src/security/paths.rs",
  "src-tauri/src/download/mod.rs",
  "src-tauri/src/cache/mod.rs",
]) {
  if (!fs.existsSync(file)) errors.push(`Phase-1-Kerndatei fehlt: ${file}`);
}

const productionOperations = read("src-tauri/src/operations/engine.rs")
  .split(/\r?\n#\[cfg\(test\)\]\r?\nmod tests/u)[0];
if (productionOperations.includes("FailAt")) {
  errors.push("Failure-Injection darf nicht im produktiven Operationspfad instanziiert werden.");
}
for (const [required, present] of [
  ["self.validate_plan_paths(plan)?", productionOperations.includes("self.validate_plan_paths(plan)?") || productionOperations.includes("self.validate_plan_paths(&plan)?")],
  ["staging_derived_paths(plan)", productionOperations.includes("staging_derived_paths(plan)")],
  ["profile_derived_paths(plan)", productionOperations.includes("profile_derived_paths(plan)")],
]) {
  if (!present) errors.push(`Operations-Pfadbudget wird nicht vor dem Commit geprüft: ${required}`);
}
const operationTests = read("src-tauri/src/operations/mod.rs");
for (const required of [
  "generated_profile_and_staging_paths_fit_the_documented_budget",
  "operation_plan_preflight_enforces_the_real_root_budget_before_journaling",
  "assert_eq!(profile_id_length, 40",
  "revision_id_length, 36,",
  "operation_id_length, 35,",
  "target_file_length, 29,",
  "assert_eq!(profile_length, 117",
  "assert_eq!(staging_length, 74",
  "required_plan_absolute_utf16",
  "path preflight must not create an operation row",
  "path preflight must not create a journal row",
  "path preflight must not create an operation staging directory",
]) {
  if (!operationTests.includes(required)) {
    errors.push(`Operations-Pfadbudget-Test fehlt: ${required}`);
  }
}

const cacheSource = read("src-tauri/src/cache/mod.rs").split("#[cfg(test)]")[0];
if (/\bhard_link\s*\(/.test(cacheSource)) {
  errors.push("Cache-Foundation darf keine Hardlinks erzeugen.");
}

const pathSecurity = read("src-tauri/src/security/paths.rs");
for (const required of [
  "anchor: PathBuf",
  "validate_existing_chain(&self.anchor, &absolute)",
  "pub const LEGACY_SAFE_MAX_ABSOLUTE_UTF16: usize = 247",
  "pub struct PathLengthBudget",
  "availableRelativeLength",
  "path_hardlink_forbidden",
  "path_reparse_point_forbidden",
  "path_alternate_data_stream",
  "path_windows_reserved_name",
  "path_collision",
  "accepts_path_at_the_available_relative_boundary",
  "rejects_path_one_unit_beyond_the_available_relative_boundary",
  "absolute_path_budget_accounts_for_the_registered_root_length",
  "create hardlink test fixture",
  "hardlink fixture was not created",
  "classifies_verified_windows_junctions_with_the_stable_reparse_error",
  "rejects_windows_directory_junctions_after_verified_fixture_creation",
  "junction fixture creation failed",
]) {
  if (!pathSecurity.includes(required)) errors.push(`Pfadsicherheitsmerkmal fehlt: ${required}`);
}

const reparseClassificationIndex = pathSecurity.indexOf("if is_reparse_point(&metadata)");
const symlinkClassificationIndex = pathSecurity.indexOf("if metadata.file_type().is_symlink()");
if (reparseClassificationIndex < 0 || symlinkClassificationIndex < 0 || reparseClassificationIndex > symlinkClassificationIndex) {
  errors.push("Windows-Reparse-Points müssen vor der allgemeinen Symlink-Klassifizierung geprüft werden.");
}

if (pathSecurity.includes("if status.is_ok_and")) {
  errors.push("Junction-Test darf fehlgeschlagene Fixture-Erstellung nicht überspringen.");
}
if (!pathSecurity.includes('Command::new("powershell.exe")')) {
  errors.push("Windows-Junction-Test muss PowerShell explizit und überprüfbar aufrufen.");
}
for (const required of [
  "New-Item -ItemType Junction",
  "S9LAB_JUNCTION_LINK",
  "S9LAB_JUNCTION_TARGET",
  "New-Item -ItemType Junction -Path $link -Target $target",
  "read created junction metadata",
  "PowerShell did not create the junction",
  "junction target does not exist",
  "created link has wrong LinkType",
  "removing the junction must not remove target content",
]) {
  if (!pathSecurity.includes(required)) errors.push(`Junction-Fixture-Prüfung fehlt: ${required}`);
}

const secureFs = read("src-tauri/src/security/fs.rs");
if (!secureFs.includes("pub fn open_new_file(path: &SecurePath)")) {
  errors.push("Zentraler sicherer Create-New-Dateizugriff fehlt.");
}
const download = read("src-tauri/src/download/mod.rs");
if (!download.includes("secure_fs::open_new_file(&partial)")) {
  errors.push("Download-Teil-Dateien müssen über den sicheren Dateizugriff erstellt werden.");
}
for (const required of ["download_https_required", "download_port_not_allowed", "download_domain_not_allowed", "download_size_mismatch", "download_hash_mismatch"]) {
  if (!download.includes(required)) errors.push(`Download-Sicherheitsfehler fehlt: ${required}`);
}

const schema = migrations.toLowerCase();
for (const secretColumn of [" access_token ", " refresh_token ", " password ", " secret "]) {
  if (schema.includes(secretColumn)) errors.push(`Geheimnisfeld im SQLite-Schema gefunden: ${secretColumn.trim()}`);
}


const storageSource = read("src-tauri/src/storage/mod.rs");
for (const required of [
  "pub(crate) fn initialize(database_path: &SecurePath)",
  "secure_fs::create_parent_directories(database_path)",
]) {
  if (!storageSource.includes(required)) errors.push(`Kontrollierte SQLite-Initialisierung fehlt: ${required}`);
}
const storageLines = storageSource.split(/\r?\n/);
for (let index = 0; index + 1 < storageLines.length; index += 1) {
  if (/[A-Za-z0-9]\\\s*$/.test(storageLines[index]) && /^\s*[A-Za-z0-9]/.test(storageLines[index + 1])) {
    errors.push(`SQLite-SQL enthält in Zeile ${index + 1} eine Rust-Zeilenfortsetzung ohne trennendes Leerzeichen.`);
  }
}

const windowsWorkflowPath = ".github/workflows/phase1-windows-verification.yml";
if (!fs.existsSync(windowsWorkflowPath)) {
  errors.push("Windows-CI-Workflow für Phase 1 fehlt.");
} else {
  const workflow = read(windowsWorkflowPath);
  for (const required of [
    "permissions:\n  contents: read",
    "npm test",
    "npm run build",
    "cargo fmt --all -- --check",
    "cargo check --locked",
    "cargo clippy --locked --all-targets -- -D warnings",
    "cargo test --locked",
    "crash_recovery_never_leaves_a_mixed_revision",
    "rejects_existing_hardlinks",
    "classifies_verified_windows_junctions_with_the_stable_reparse_error",
    "rejects_windows_directory_junctions_after_verified_fixture_creation",
    "accepts_path_at_the_available_relative_boundary",
    "rejects_path_one_unit_beyond_the_available_relative_boundary",
    "operation_plan_preflight_enforces_the_real_root_budget_before_journaling",
    "phase1_transaction_demo",
  ]) {
    if (!workflow.includes(required)) errors.push(`Windows-CI-Schritt fehlt: ${required}`);
  }
  for (const forbidden of ["upload-artifact", "release-action", "contents: write", "SIGNING_PRIVATE_KEY"]) {
    if (workflow.includes(forbidden)) errors.push(`Windows-CI enthält verbotene Veröffentlichung/Signierung: ${forbidden}`);
  }
}

if (errors.length) {
  console.error([...new Set(errors)].join("\n"));
  process.exit(1);
}
console.log("Phase-1-Foundation statisch erfolgreich geprüft.");
