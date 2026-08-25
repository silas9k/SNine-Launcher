import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const publicSecretIdentifier = /\b(?:accessToken|refreshToken|deviceCode|device_code|sessionToken)\b/g;
const schemaSecretColumn = /\b(?:access_token|refresh_token|device_code|password|secret)\b/gi;

export function publicSecretIdentifiers(text) {
  return [...text.matchAll(publicSecretIdentifier)].map((match) => match[0]);
}

export function secretSchemaColumns(text) {
  return [...text.matchAll(schemaSecretColumn)].map((match) => match[0]);
}

export function inspectPhase3Auth(root = ".") {
  const errors = [];
  const contract = fs.readFileSync(path.join(root, "contracts/ipc-contracts.json"), "utf8");
  const generated = fs.readFileSync(path.join(root, "src/lib/generated/ipc-contracts.ts"), "utf8");
  for (const [name, text] of [["IPC-Vertrag", contract], ["generierter Frontend-Vertrag", generated]]) {
    for (const identifier of publicSecretIdentifiers(text)) {
      errors.push(`${name}: geheimes Feld ${identifier} ist öffentlich`);
    }
  }

  for (const directory of ["src", "src/pages", "src/lib"]) {
    const absolute = path.join(root, directory);
    if (!fs.existsSync(absolute)) continue;
    for (const entry of fs.readdirSync(absolute, { withFileTypes: true })) {
      if (!entry.isFile() || !/\.(?:ts|tsx)$/.test(entry.name)) continue;
      const file = path.join(absolute, entry.name);
      const text = fs.readFileSync(file, "utf8");
      for (const identifier of publicSecretIdentifiers(text)) {
        errors.push(`${path.relative(root, file)}: geheimes Frontend-Feld ${identifier}`);
      }
    }
  }

  const migrations = fs.readFileSync(path.join(root, "src-tauri/src/storage/migrations.rs"), "utf8");
  for (const column of secretSchemaColumns(migrations)) {
    errors.push(`SQLite-Migration: geheime Spalte ${column}`);
  }

  const microsoft = fs.readFileSync(path.join(root, "src-tauri/src/auth/microsoft.rs"), "utf8");
  const store = fs.readFileSync(path.join(root, "src-tauri/src/auth/store.rs"), "utf8");
  const service = fs.readFileSync(path.join(root, "src-tauri/src/auth/service.rs"), "utf8");
  const launcher = fs.readFileSync(path.join(root, "src-tauri/src/lib.rs"), "utf8");
  const logging = fs.readFileSync(path.join(root, "src-tauri/src/logging.rs"), "utf8");
  for (const [description, ok] of [
    ["Besitzprüfung fehlt", microsoft.includes("/entitlements/mcstore")],
    ["OS-Schlüsselspeicher fehlt", store.includes("keyring::Entry") && store.includes("vault_ref")],
    ["Offline-Richtlinie ist nicht fail-closed", service.includes("OfflinePolicyStatus::unconfigured")],
    ["alter Device-Code-IPC-Handler ist noch registriert", !launcher.includes("commands::start_microsoft_login")],
    ["Log-Redaktion fehlt", logging.includes("redact_sensitive") && logging.includes("[REDACTED]")],
  ]) {
    if (!ok) errors.push(description);
  }
  return errors;
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  const errors = inspectPhase3Auth();
  if (errors.length) {
    console.error(errors.join("\n"));
    process.exit(1);
  }
  console.log("Phase-3-Authentifizierung: Token-, Besitz-, Vault-, Offline- und Log-Gates erfolgreich.");
}
