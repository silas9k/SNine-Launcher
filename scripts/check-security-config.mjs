import fs from "node:fs";
import path from "node:path";

const errors = [];
const read = (file) => fs.readFileSync(file, "utf8");
const tauri = JSON.parse(read("src-tauri/tauri.conf.json"));
const tauriWindows = JSON.parse(read("src-tauri/tauri.windows.conf.json"));
const csp = tauri.app?.security?.csp ?? "";

const updater = tauri.plugins?.updater;

if (!updater) {
  errors.push("Tauri-Updater ist nicht konfiguriert.");
} else {
  const endpoints = updater.endpoints;

  if (
    !Array.isArray(endpoints)
    || endpoints.length === 0
    || endpoints.some((endpoint) => typeof endpoint !== "string" || !endpoint.startsWith("https://"))
  ) {
    errors.push("Tauri-Updater muss ausschließlich HTTPS-Endpunkte verwenden.");
  }

  if (typeof updater.pubkey !== "string" || !updater.pubkey.trim()) {
    errors.push("Tauri-Updater Public Key fehlt.");
  }
}

if (tauri.bundle?.createUpdaterArtifacts !== true) {
  errors.push("Updater-Artefakte müssen für Release-Builds aktiviert sein.");
}
if (csp.includes("31.70.89.55") || csp.includes(["http", "://31."].join(""))) errors.push("Unsicherer produktiver HTTP-Endpunkt in CSP.");
for (const match of csp.matchAll(/http:\/\/[^\s;]+/g)) {
  const url = match[0];
  if (url !== ["http", "://ipc.localhost"].join("")) errors.push(`Nicht erlaubte HTTP-Quelle in CSP: ${url}`);
}

// Die Windows-Plattformkonfiguration wird von Tauri nach der Hauptkonfiguration
// eingespielt und ist für die Phase-0-Bundleauswahl maßgeblich.
if (tauri.bundle?.targets !== undefined) {
  errors.push("Bundle-Ziele dürfen in Phase 0 nicht gleichzeitig in tauri.conf.json und tauri.windows.conf.json definiert sein.");
}
const windowsTargets = tauriWindows.bundle?.targets;
if (!Array.isArray(windowsTargets) || windowsTargets.length !== 1 || windowsTargets[0] !== "nsis") {
  errors.push("Phase 0 muss in tauri.windows.conf.json eindeutig nur das NSIS-Bundle konfigurieren.");
}

const forbiddenFiles = [
  "src-tauri/backend-integration.json",
  "src-tauri/client-update.json",
  "src/updater-config.ts",
];
for (const file of forbiddenFiles) if (fs.existsSync(file)) errors.push(`${file} darf im Phase-0-Stand nicht aktiv sein.`);

const rustLib = read("src-tauri/src/lib.rs");
if (rustLib.includes("tauri_plugin_updater") || rustLib.includes("tauri_plugin_process")) errors.push("Updater-/Relaunch-Plugin ist in Rust aktiv.");

const cargoToml = read("src-tauri/Cargo.toml");
if (cargoToml.includes("tauri-plugin-updater") || cargoToml.includes("tauri-plugin-process")) {
  errors.push("Updater-/Process-Plugins sind weiterhin direkte Rust-Abhängigkeiten.");
}
const cargoLock = read("src-tauri/Cargo.lock");
const rootPackage = cargoLock.match(/\[\[package\]\]\nname = "s9lab-launcher"[\s\S]*?\n\]/)?.[0] ?? "";
if (rootPackage.includes("tauri-plugin-updater") || rootPackage.includes("tauri-plugin-process")) {
  errors.push("Cargo.lock führt Updater-/Process-Plugins weiterhin als direkte Launcher-Abhängigkeit.");
}

const packageJson = JSON.parse(read("package.json"));
for (const dependency of ["@tauri-apps/plugin-updater", "@tauri-apps/plugin-process"]) {
  if (packageJson.dependencies?.[dependency] || packageJson.devDependencies?.[dependency]) errors.push(`${dependency} ist im Frontend weiter aktiv.`);
}

const activeSource = ["src", "src-tauri/src"];
for (const root of activeSource) {
  const stack = [root];
  while (stack.length) {
    const current = stack.pop();
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) stack.push(full);
      else if (/\.(ts|tsx|rs|json|css)$/.test(entry.name)) {
        const text = read(full);
        if (text.includes("31.70.89.55")) errors.push(`${full}: alte Backend-IP gefunden.`);
        if (text.includes("allow_insecure_http")) errors.push(`${full}: unsicherer HTTP-Schalter gefunden.`);
        if ((entry.name.endsWith(".ts") || entry.name.endsWith(".tsx")) && /\b(?:localStorage|sessionStorage)\b/.test(text)) {
          errors.push(`${full}: persistente Browser-Speicherung ist für Produktdaten nicht erlaubt.`);
        }
        if (entry.name.endsWith(".css") && /@import\s+(?:url\()?['"]?https?:\/\//i.test(text)) {
          errors.push(`${full}: externer CSS-Laufzeitimport gefunden.`);
        }
      }
    }
  }
}

if (errors.length) {
  console.error(errors.join("\n"));
  process.exit(1);
}
console.log("Sicherheitskonfiguration erfolgreich geprüft.");
