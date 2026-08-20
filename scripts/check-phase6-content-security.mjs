import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REQUIRED_COMMANDS = [
  "phase6_content_snapshot",
  "phase6_check_content_updates",
  "phase6_modrinth_search",
  "phase6_modrinth_project",
  "phase6_install_modrinth",
  "phase6_set_content_enabled",
  "phase6_remove_content",
  "phase6_update_content",
  "phase6_add_local_file",
  "phase6_import_modrinth_pack",
  "phase6_export_profile",
  "phase6_import_profile",
];

const PUBLIC_RISK_FIELD = /\b(?:rawUrl|downloadUrl|artifactUrl|providerUrl|mirrorUrl|accessToken|refreshToken|sessionToken|deviceCode|device_code|password|secret)\b/gi;

function read(root, relative) {
  return fs.readFileSync(path.join(root, relative), "utf8");
}

function productRust(text) {
  const testModule = text.search(/\n#\[cfg\(test\)\]\s*\nmod tests\s*\{/);
  return testModule >= 0 ? text.slice(0, testModule) : text;
}

function phase6GeneratedSlice(text) {
  return [...text.matchAll(/^export interface Phase6\w+\s*\{[\s\S]*?^\}/gm)]
    .map((match) => match[0])
    .join("\n");
}

export function phase6PublicRiskFields(text) {
  return [...text.matchAll(PUBLIC_RISK_FIELD)].map((match) => match[0]);
}

export function phase6ContractSlice(contract) {
  const types = Object.fromEntries(
    Object.entries(contract.types ?? {}).filter(([name]) => name.startsWith("Phase6")),
  );
  const commands = (contract.commands ?? []).filter((command) =>
    String(command.name).startsWith("phase6_"));
  return JSON.stringify({ types, commands });
}

export function phase6CommandErrors(contract, rustLib) {
  const errors = [];
  if ((contract.version ?? 0) < 6) {
    errors.push("IPC-Vertrag ist älter als Version 6.");
  }
  const commands = new Set((contract.commands ?? []).map((command) => command.name));
  for (const command of REQUIRED_COMMANDS) {
    if (!commands.has(command)) {
      errors.push(`Phase-6-IPC-Vertrag fehlt: ${command}`);
    }
    if (!rustLib.includes(`ipc::${command}`)) {
      errors.push(`Phase-6-Command ist nicht im Tauri-Handler registriert: ${command}`);
    }
  }
  return errors;
}

export function inspectPhase6ContentSecurity(root = ".") {
  const errors = [];
  const rustLib = read(root, "src-tauri/src/lib.rs");
  const contract = JSON.parse(read(root, "contracts/ipc-contracts.json"));
  const generated = read(root, "src/lib/generated/ipc-contracts.ts");
  const frontendCommands = read(root, "src/lib/contentCommands.ts");
  const contentEditor = read(root, "src/components/content/ContentEditor.tsx");
  const contentModel = productRust(read(root, "src-tauri/src/content/model.rs"));
  const resolver = productRust(read(root, "src-tauri/src/content/resolver.rs"));
  const archive = productRust(read(root, "src-tauri/src/content/archive.rs"));
  const modrinthMod = productRust(read(root, "src-tauri/src/modrinth/mod.rs"));
  const modrinthModel = productRust(read(root, "src-tauri/src/modrinth/model.rs"));
  const modrinthProvider = productRust(read(root, "src-tauri/src/modrinth/provider.rs"));
  const modrinthValidation = productRust(read(root, "src-tauri/src/modrinth/validation.rs"));
  const download = productRust(read(root, "src-tauri/src/download/mod.rs"));
  const contentService = productRust(read(root, "src-tauri/src/content_service.rs"));
  const profileFormat = productRust(read(root, "src-tauri/src/profile_format.rs"));
  const projection = productRust(read(root, "src-tauri/src/content_projection.rs"));
  const minecraftService = productRust(read(root, "src-tauri/src/minecraft/service.rs"));

  errors.push(...phase6CommandErrors(contract, rustLib));

  for (const [label, text] of [
    ["Phase-6-IPC", phase6ContractSlice(contract)],
    ["generierter Phase-6-Vertrag", phase6GeneratedSlice(generated)],
    ["Frontend-Contentwrapper", frontendCommands],
    ["persistentes Contentmodell", contentModel],
    ["portables Profilformat", profileFormat],
  ]) {
    const fields = [...new Set(phase6PublicRiskFields(text))];
    if (fields.length) {
      errors.push(`${label} veröffentlicht URL-/Secret-Felder: ${fields.join(", ")}`);
    }
  }

  for (const [description, ok] of [
    [
      "Contentlock ist nicht vollständig kanonisch gehasht und validiert",
      contentModel.includes("ResolvedContentLockV1")
        && resolver.includes("canonical_content_lock_payload")
        && resolver.includes("content_lock_sha256")
        && resolver.includes("content_lock_hash_mismatch")
        && resolver.includes("content_lock_conflict")
        && resolver.includes("content_lock_target_collision"),
    ],
    [
      "Contentquellen enthalten keine URL-freie Provideridentität",
      contentModel.includes("enum ContentSourceV1")
        && contentModel.includes("project_id")
        && contentModel.includes("version_id")
        && !/\b(?:url|uri)\s*:/i.test(contentModel),
    ],
    [
      "Lokale Inhaltsarchive besitzen nicht alle Pfad-, Typ-, Kollisions- und Größen-Gates",
      archive.includes("normalize_relative_path")
        && archive.includes("collision_key")
        && archive.includes("content_archive_encrypted_entry_forbidden")
        && archive.includes("content_archive_special_entry_forbidden")
        && archive.includes("content_archive_compression_ratio_exceeded")
        && archive.includes("content_archive_file_directory_conflict")
        && archive.includes("validate_existing_chain"),
    ],
    [
      "Modrinth-Origins sind nicht exakt auf die offiziellen HTTPS-Hosts gebunden",
      modrinthMod.includes('MODRINTH_API_ORIGIN: &str = "https://api.modrinth.com"')
        && modrinthMod.includes('MODRINTH_CDN_ORIGIN: &str = "https://cdn.modrinth.com"')
        && modrinthValidation.includes('url.scheme() != "https"')
        && modrinthValidation.includes("url.host_str() != expected.host_str()")
        && modrinthValidation.includes("url.port_or_known_default() != Some(443)")
        && modrinthValidation.includes("!url.username().is_empty()")
        && modrinthValidation.includes("url.password().is_some()")
        && modrinthValidation.includes("forbid_query && url.query().is_some()"),
    ],
    [
      "Modrinth-Client folgt möglicherweise Redirects oder liest unbegrenzte Antworten",
      modrinthProvider.includes("reqwest::redirect::Policy::none()")
        && modrinthProvider.includes("MAX_SEARCH_RESPONSE_BYTES")
        && modrinthProvider.includes("MAX_PROJECT_RESPONSE_BYTES")
        && modrinthProvider.includes("MAX_VERSIONS_RESPONSE_BYTES")
        && modrinthProvider.includes("response.bytes_stream()"),
    ],
    [
      "Modrinth-Download-URLs könnten serialisiert oder ohne SHA-512 gebunden werden",
      modrinthModel.includes("#[serde(skip)]")
        && modrinthModel.includes("download_url: Url")
        && modrinthModel.includes("upstream_sha512")
        && contentService.includes("resolve_upstream_sha512")
        && download.includes("expected_sha512")
        && download.includes("actual_sha512")
        && download.includes("download_hash_mismatch"),
    ],
    [
      "Contentaktivierung umgeht Staging, Hashprüfung oder den verifizierten Cache",
      contentService.includes('resolve("staging-operations"')
        && contentService.includes("validate_local_content")
        && contentService.includes("activate_verified_copy")
        && contentService.includes("cleanup_staging"),
    ],
    [
      "MRPACK-Import bindet Index, Pfade und Downloads nicht fail-closed",
      contentService.includes("read_mrpack_index")
        && contentService.includes("validate_mrpack_files")
        && contentService.includes("content_modpack_index_size_invalid")
        && contentService.includes("path_ambiguous_separator")
        && contentService.includes("content_modpack_sha512_invalid"),
    ],
    [
      "Portables Profilformat ist nicht versioniert, atomar und archivgehärtet",
      profileFormat.includes('PROFILE_EXPORT_FORMAT: &str = "site.s9lab.profile-export"')
        && profileFormat.includes("deny_unknown_fields")
        && profileFormat.includes("sync_all")
        && profileFormat.includes("ensure_path_absent")
        && profileFormat.includes("fs::rename(&temporary_path, destination.absolute())")
        && profileFormat.includes("profile_export_archive_encrypted_entry_forbidden")
        && profileFormat.includes("profile_export_archive_symlink_forbidden")
        && profileFormat.includes("profile_export_archive_entry_collision")
        && profileFormat.includes("profile_export_archive_compression_ratio_exceeded")
        && profileFormat.includes("validate_existing_chain"),
    ],
    [
      "Startprojektion schützt fremde Dateien oder Rollback nicht vollständig",
      projection.includes("validate_resolved_content_lock")
        && projection.includes("validate_foreign_conflicts")
        && projection.includes("content_projection_foreign_target_conflict")
        && projection.includes("verify_desired_sources")
        && projection.includes("rollback_moves")
        && projection.includes("content_projection_rollback_failed")
        && projection.includes("collision_key")
        && minecraftService.includes("project_content_for_launch"),
    ],
    [
      "Browserfallback oder lokale Dateiauswahl meldet Desktop-Capabilities nicht fail-closed",
      frontendCommands.includes('"__TAURI_INTERNALS__" in window')
        && frontendCommands.includes("file.path")
        && frontendCommands.includes('state: "disabled"')
        && frontendCommands.includes("content_desktop_runtime_required")
        && contentEditor.includes("contentCommands.importModrinthPack")
        && contentEditor.includes("contentCommands.exportProfile")
        && contentEditor.includes("contentCommands.importProfile"),
    ],
  ]) {
    if (!ok) errors.push(description);
  }

  return errors;
}

const isMain = process.argv[1]
  && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  const errors = inspectPhase6ContentSecurity();
  if (errors.length) {
    console.error(errors.join("\n"));
    process.exit(1);
  }
  console.log(
    "Phase 6: IPC-, Lock-, Modrinth-, Archiv-, Profilformat- und Projektions-Gates erfolgreich.",
  );
}
