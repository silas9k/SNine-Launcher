import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REQUIRED_COMMANDS = [
  "phase5_runtime_catalog",
  "phase5_s9lab_component_catalog",
  "phase5_profile_runtime_status",
  "phase5_install_profile",
  "phase5_repair_profile",
  "phase5_launch_profile",
  "phase5_stop_launch",
  "phase5_launch_statuses",
  "phase5_set_s9lab_component",
];

const LEGACY_COMMANDS = [
  "bootstrap",
  "save_settings",
  "get_client_status",
  "install_client",
  "launch_client",
  "stop_client",
  "get_launch_status",
  "read_launcher_logs",
  "open_game_directory",
  "pending_design_import",
  "fetch_player_skin",
];

const LEGACY_MODULES = ["installer", "java", "launcher", "manifest"];
const SECRET_FIELD = /\b(?:accessToken|refreshToken|sessionToken|deviceCode|device_code|minecraftAccessToken|microsoftRefreshToken)\b/g;
const URL_FIELD = /(?:^|["{,\s])(?:rawUrl|downloadUrl|artifactUrl|providerUrl|mirrorUrl)["?]?\s*:/gi;

function read(root, relative) {
  return fs.readFileSync(path.join(root, relative), "utf8");
}

function productRust(text) {
  const testModule = text.search(/\n#\[cfg\(test\)\]\s*\nmod tests\s*\{/);
  return testModule >= 0 ? text.slice(0, testModule) : text;
}

function optionTag(text, value) {
  return text.match(new RegExp(`<option\\s+[^>]*value=["']${value}["'][^>]*>`, "i"))?.[0] ?? "";
}

function phase5ContractSlice(contract) {
  const types = Object.fromEntries(
    Object.entries(contract.types ?? {}).filter(([name]) => name.startsWith("Phase5")),
  );
  const commands = (contract.commands ?? []).filter((command) =>
    String(command.name).startsWith("phase5_"));
  return JSON.stringify({ types, commands });
}

export function inspectPhase5RuntimeSecurity(root = ".") {
  const errors = [];
  const rustLib = read(root, "src-tauri/src/lib.rs");
  const minecraftMod = read(root, "src-tauri/src/minecraft/mod.rs");
  const tauri = JSON.parse(read(root, "src-tauri/tauri.conf.json"));
  const service = productRust(read(root, "src-tauri/src/minecraft/service.rs"));
  const launch = productRust(read(root, "src-tauri/src/minecraft/profile_launch.rs"));
  const javaRuntime = productRust(read(root, "src-tauri/src/minecraft/java_runtime.rs"));
  const resolver = productRust(read(root, "src-tauri/src/minecraft/resolver.rs"));
  const componentProvider = productRust(read(root, "src-tauri/src/components/provider.rs"));
  const componentTrust = productRust(read(root, "src-tauri/src/components/trust.rs"));
  const componentJar = productRust(read(root, "src-tauri/src/components/jar.rs"));
  const componentModel = productRust(read(root, "src-tauri/src/runtime/component.rs"));
  const operations = productRust(read(root, "src-tauri/src/operations/engine.rs"));
  const contract = JSON.parse(read(root, "contracts/ipc-contracts.json"));
  const generated = read(root, "src/lib/generated/ipc-contracts.ts");
  const runtimeCommands = read(root, "src/lib/runtimeCommands.ts");
  const runtimePanel = read(root, "src/components/runtime/RuntimePanel.tsx");

  if (/\bmod\s+commands\s*;/.test(rustLib) || rustLib.includes("app::state::AppState")) {
    errors.push("Legacy-AppState oder globales commands-Modul ist wieder im aktiven Rust-Modulgraphen.");
  }
  for (const command of LEGACY_COMMANDS) {
    if (rustLib.includes(`commands::${command}`)) {
      errors.push(`alter globaler Command ist wieder registriert: ${command}`);
    }
  }
  for (const moduleName of LEGACY_MODULES) {
    if (new RegExp(`\\b(?:pub\\s+)?mod\\s+${moduleName}\\s*;`).test(minecraftMod)) {
      errors.push(`altes Minecraft-Modul ist wieder aktiv: minecraft::${moduleName}`);
    }
  }

  const resources = tauri.bundle?.resources;
  if (!Array.isArray(resources) || resources.length !== 0) {
    errors.push("Phase 5 darf keine Legacy-JAR-Ressourcen über Tauri bündeln.");
  }
  if (JSON.stringify(resources ?? []).match(/default-profile-mods|s9lab-client-bundled|\.jar/i)) {
    errors.push("Legacy- oder sonstige JAR-Ressource ist wieder im Tauri-Bundle aktiv.");
  }
  const activePhase5Rust = [
    service,
    launch,
    javaRuntime,
    resolver,
    componentProvider,
    componentTrust,
    componentJar,
    componentModel,
  ].join("\n");
  if (/default-profile-mods|s9lab-client-bundled/i.test(activePhase5Rust)) {
    errors.push("Aktiver Phase-5-Code referenziert eine alte gebündelte Client-/Mod-JAR.");
  }

  const commandNames = new Set((contract.commands ?? []).map((command) => command.name));
  for (const command of REQUIRED_COMMANDS) {
    if (!commandNames.has(command)) {
      errors.push(`Phase-5-IPC-Vertrag fehlt: ${command}`);
    }
    if (!rustLib.includes(`ipc::${command}`)) {
      errors.push(`Phase-5-Command ist nicht im Tauri-Handler registriert: ${command}`);
    }
  }
  if ((contract.version ?? 0) < 5) {
    errors.push("IPC-Vertrag ist älter als Version 5.");
  }
  for (const [label, text] of [
    ["Phase-5-IPC", phase5ContractSlice(contract)],
    ["generierter Phase-5-Vertrag", generated],
    ["Frontend-Runtimewrapper", runtimeCommands],
  ]) {
    const secrets = [...text.matchAll(SECRET_FIELD)].map((match) => match[0]);
    if (secrets.length) {
      errors.push(`${label} veröffentlicht geheime Felder: ${[...new Set(secrets)].join(", ")}`);
    }
  }
  if (URL_FIELD.test(phase5ContractSlice(contract))) {
    errors.push("Phase-5-IPC enthält eine importierbare Raw-/Download-/Provider-URL.");
  }

  for (const [description, ok] of [
    [
      "S9Lab-Origin ist nicht ausschließlich zur Buildzeit konfigurierbar",
      componentProvider.includes('option_env!("S9LAB_COMPONENT_PROVIDER_ORIGIN")')
        && !componentProvider.includes('std::env::var("S9LAB_COMPONENT_PROVIDER_ORIGIN")'),
    ],
    [
      "S9Lab-Vertrauenskette ist nicht ausschließlich zur Buildzeit konfigurierbar",
      componentProvider.includes('option_env!("S9LAB_COMPONENT_TRUST_KEYS_JSON")')
        && componentTrust.includes("ring::signature"),
    ],
    [
      "S9Lab-Provider erzwingt keine HTTPS-Root-Origin",
      componentProvider.includes('url.scheme() != "https"')
        && componentProvider.includes("url.port_or_known_default() != Some(443)")
        && componentProvider.includes("!matches!(url.path(), \"\" | \"/\")"),
    ],
    [
      "S9Lab-Provider folgt möglicherweise Redirects",
      componentProvider.includes("reqwest::redirect::Policy::none()"),
    ],
    [
      "signiertes Komponentenmanifest bindet nicht alle Pflichtfelder",
      [
        "component_id",
        "component_version",
        "minecraft_version",
        "loader.kind",
        "loader.loader_version",
        "size_bytes",
        "sha256",
        "relative_target",
      ].every((field) => componentModel.includes(`manifest.${field}`)),
    ],
    [
      "Komponenten-JAR wird nicht gegen Größe, Hash und JAR-Struktur geprüft",
      componentJar.includes("component_artifact_size_mismatch")
        && componentJar.includes("component_artifact_hash_mismatch")
        && componentJar.includes("validate_jar_entries"),
    ],
    [
      "Komponentenartefakt besitzt keinen Reparse-/Hardlink-Schutz",
      componentJar.includes("component_artifact_reparse_point_forbidden")
        && componentJar.includes("component_artifact_hardlink_forbidden"),
    ],
    [
      "Komponentenaktivierung nutzt nicht Operations-Staging und verifizierte Cachekopien",
      service.includes('"staging-operations"')
        && service.includes("activate_verified_copy")
        && operations.includes("secure_fs::copy_new"),
    ],
    [
      "Windows-Starts besitzen keine race-freie Job-Object-Prozessbaumkontrolle",
      launch.includes("CREATE_SUSPENDED")
        && launch.includes("CreateJobObjectW")
        && launch.includes("JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE")
        && launch.includes("AssignProcessToJobObject")
        && launch.includes("TerminateJobObject"),
    ],
  ]) {
    if (!ok) errors.push(description);
  }

  const productionComponentCode = [componentProvider, componentTrust, componentModel].join("\n");
  if (/from_seed|private[_ -]?key|secret[_ -]?key|Ed25519KeyPair/i.test(productionComponentCode)) {
    errors.push("Produktcode enthält Material oder Erzeugung für einen privaten Komponentenschlüssel.");
  }

  const browserNeo = runtimeCommands.match(
    /neoforgeCapability\s*:\s*\{[\s\S]*?state\s*:\s*"([^"]+)"/,
  )?.[1];
  const browserComponent = runtimeCommands.match(
    /s9labComponentCapability\s*:\s*\{[\s\S]*?state\s*:\s*"([^"]+)"/,
  )?.[1];
  if (browserNeo !== "unconfigured" || browserComponent !== "unconfigured") {
    errors.push("Browserfallback meldet eine nicht vorhandene Phase-5-Capability als verfügbar.");
  }
  if (!/componentCapability\?\.state\s*===\s*["']available["']/u.test(runtimePanel)
      || !/disabled\s*=\s*\{\s*!isComponentAvailable/u.test(runtimePanel)) {
    errors.push("UI aktiviert S9Lab-Komponenten nicht ausschließlich nach Rust-Capability.");
  }

  const neoforgeIncomplete = service.includes("runtime_neoforge_installation_pipeline_unavailable")
    || launch.includes("runtime_component_neoforge_launch_unsupported");
  if (neoforgeIncomplete) {
    if (!service.includes("neoforge_capability: CapabilityStatus::unconfigured")) {
      errors.push("NeoForge ist unvollständig, wird vom Rust-Kern aber nicht fail-closed gemeldet.");
    }
    if (!runtimePanel.includes('<option value="neoforge" disabled={!isNeoforgeAvailable}')) {
      errors.push("NeoForge ist unvollständig, bleibt in der UI aber auswählbar.");
    }
  }

  const managedJavaSupplyVerified =
    /managed[\s\S]{0,600}(?:sha256|signed|signature)/i.test(javaRuntime)
    && /(?:download|staging|activate)/i.test(javaRuntime);
  const managedOption = optionTag(runtimePanel, "managed");
  if (!managedJavaSupplyVerified && managedOption && !/\bdisabled\b/.test(managedOption)) {
    errors.push(
      "Managed Java wird auswählbar angeboten, obwohl kein hash-/signaturgebundener Beschaffungs- und Aktivierungspfad nachgewiesen ist.",
    );
  }

  if (!launch.includes("validate_existing_chain")
      || !launch.includes("runtime_revision_artifact_hash_mismatch")) {
    errors.push("Startpfad prüft aktive Revisionsartefakte nicht vollständig.");
  }
  if (!service.includes("ensure_minecraft_session")) {
    errors.push("Phase-5-Start ist nicht an eine geschützte Minecraft-Sitzung gebunden.");
  }

  return errors;
}

const isMain = process.argv[1]
  && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  const errors = inspectPhase5RuntimeSecurity();
  if (errors.length) {
    console.error(errors.join("\n"));
    process.exit(1);
  }
  console.log(
    "Phase 5: Legacy-Abschaltung, IPC-Geheimnisse, Provider-, Signatur-, JAR-, Pfad- und Capability-Gates erfolgreich.",
  );
}
