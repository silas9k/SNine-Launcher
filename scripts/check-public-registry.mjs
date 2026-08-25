import fs from "node:fs";
import { isIP } from "node:net";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { walk } from "./_walk.mjs";

export const PUBLIC_REGISTRY = "https://registry.npmjs.org/";

const urlPattern = /\b(?:git\+)?https?:\/\/[^\s"'<>`\\]+/giu;
const protocolRelativeUrlPattern = /(?<!:)\/\/(?:[a-z0-9-]+\.)+[a-z0-9-]+(?:\/[^\s"'<>`\\]*)?/giu;
const reservedInternalLabels = new Set([
  "corp", "example", "home", "internal", "invalid", "intranet", "lan", "local", "localhost", "private", "test",
]);

function isPrivateAddress(hostname) {
  const normalized = hostname.replace(/^\[|\]$/g, "").toLowerCase();
  const version = isIP(normalized);
  if (version === 4) {
    const [first, second] = normalized.split(".").map(Number);
    return first === 0
      || first === 10
      || first === 127
      || (first === 169 && second === 254)
      || (first === 172 && second >= 16 && second <= 31)
      || (first === 192 && second === 168);
  }
  if (version === 6) {
    return normalized === "::1"
      || normalized.startsWith("fc")
      || normalized.startsWith("fd")
      || /^fe[89ab]/u.test(normalized);
  }
  if (normalized === "localhost" || normalized.endsWith(".localhost")) return true;
  const labels = normalized.split(".");
  return labels.length < 2 || labels.some((label) => reservedInternalLabels.has(label));
}

function parseUrl(value) {
  try {
    return new URL(value.replace(/^git\+/u, ""));
  } catch {
    return null;
  }
}

function isApprovedShippedUrl(sourceName, parsed) {
  const approvedBySource = new Map([
    ["src-tauri/tauri.conf.json", [
      ["http", "://127.0.0.1:1420/"].join(""),
      ["http", "://ipc.localhost/"].join(""),
    ]],
    ["src-tauri/src/download/mod.rs", [
      ["https", "://example.invalid/x"].join(""),
    ]],
  ]);
  return approvedBySource.get(sourceName)?.includes(parsed.href) ?? false;
}

export function validateRegistryUrl(value, context = "registry") {
  const errors = [];
  const parsed = parseUrl(value);
  if (!parsed) return [`${context}: ungültige Registry-URL`];
  if (parsed.username || parsed.password) errors.push(`${context}: eingebettete Zugangsdaten sind verboten`);
  if (parsed.protocol !== "https:") errors.push(`${context}: Registry-URL muss HTTPS verwenden`);
  if (parsed.port) errors.push(`${context}: ein abweichender Registry-Port ist verboten`);
  if (parsed.search) errors.push(`${context}: Query-Parameter sind in Registry-URLs verboten`);
  if (parsed.hash) errors.push(`${context}: Fragmente sind in Registry-URLs verboten`);
  if (parsed.hostname !== "registry.npmjs.org") {
    errors.push(`${context}: nur ${PUBLIC_REGISTRY} ist als npm-Registry freigegeben`);
  }
  if (isPrivateAddress(parsed.hostname)) errors.push(`${context}: lokale oder private Registry-Adresse ist verboten`);
  return errors;
}

export function validateRegistryConfigValue(value, context = "registry") {
  const raw = String(value).trim();
  const errors = validateRegistryUrl(raw, context);
  const parsed = parseUrl(raw);
  if (!parsed || raw.startsWith("//") || raw !== PUBLIC_REGISTRY
    || parsed.pathname !== "/" || parsed.search || parsed.hash || parsed.port) {
    errors.push(`${context}: Registry-Konfiguration muss exakt ${PUBLIC_REGISTRY} entsprechen`);
  }
  return [...new Set(errors)];
}

function unquoteScalar(value) {
  let result = String(value).trim().replace(/\s+#.*$/u, "").trim();
  if (result.length >= 2 && ((result.startsWith('"') && result.endsWith('"'))
    || (result.startsWith("'") && result.endsWith("'")))) {
    result = result.slice(1, -1);
  }
  return result.trim();
}

function assignmentValue(match) {
  return match?.groups?.double ?? match?.groups?.single ?? match?.groups?.bare ?? "";
}

function inspectShellRegistryText(sourceName, source) {
  const errors = [];
  const environmentPattern = /(?:^|[\s;{(])(?:export\s+|set\s+)?(?:\$env:)?(?<key>NPM_CONFIG_REGISTRY)\s*=\s*(?:"(?<double>[^"]*)"|'(?<single>[^']*)'|(?<bare>[^\s;})]+))/gimu;
  for (const match of source.matchAll(environmentPattern)) {
    errors.push(...validateRegistryConfigValue(unquoteScalar(assignmentValue(match)), `${sourceName}: ${match.groups?.key ?? "NPM_CONFIG_REGISTRY"}`));
  }
  const cmdSetPattern = /(?:^|[\r\n&|()\s])set\s+"(?<key>NPM_CONFIG_REGISTRY)\s*=\s*(?<double>[^"]*)"/gimu;
  for (const match of source.matchAll(cmdSetPattern)) {
    errors.push(...validateRegistryConfigValue(unquoteScalar(assignmentValue(match)), `${sourceName}: ${match.groups?.key ?? "NPM_CONFIG_REGISTRY"}`));
  }
  const cliPattern = /(?:^|[\s"'`])--registry(?:\s*=\s*|\s+)(?:"(?<double>[^"]*)"|'(?<single>[^']*)'|(?<bare>[^\s"'`;]+))/gimu;
  for (const match of source.matchAll(cliPattern)) {
    errors.push(...validateRegistryConfigValue(unquoteScalar(assignmentValue(match)), `${sourceName}: --registry`));
  }
  return errors;
}

function inspectJsonRegistryFields(sourceName, source) {
  const errors = [];
  if (!sourceName.toLowerCase().endsWith(".json")) return errors;
  let value;
  try { value = JSON.parse(source); } catch { return errors; }
  if (value?.scripts && typeof value.scripts === "object" && !Array.isArray(value.scripts)) {
    for (const [scriptName, script] of Object.entries(value.scripts)) {
      if (typeof script === "string") {
        errors.push(...inspectShellRegistryText(`${sourceName}: $.scripts.${scriptName}`, script));
      }
    }
  }
  const visit = (node, location) => {
    if (Array.isArray(node)) {
      node.forEach((entry, index) => visit(entry, `${location}[${index}]`));
      return;
    }
    if (!node || typeof node !== "object") return;
    for (const [key, entry] of Object.entries(node)) {
      const next = `${location}.${key}`;
      const normalized = key.toLowerCase().replaceAll("-", "_");
      if (normalized === "registry" || normalized === "npm_config_registry") {
        errors.push(...validateRegistryConfigValue(entry, `${sourceName}: ${next}`));
      }
      visit(entry, next);
    }
  };
  visit(value, "$ ".trim());
  return errors;
}

function findRegistryAssignments(sourceName, source) {
  const errors = inspectJsonRegistryFields(sourceName, source);
  const extension = path.extname(sourceName).toLowerCase();
  const lines = source.split(/\r?\n/u);
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#") || trimmed.startsWith("//")) continue;
    const matches = [];
    if (extension === ".npmrc") {
      matches.push(trimmed.match(/^(?<key>(?:@[a-z0-9._~-]+:)?registry)\s*=\s*(?<value>.*)$/iu));
    }
    if (extension === ".yaml" || extension === ".yml") {
      matches.push(trimmed.match(/^["']?(?<key>NPM_CONFIG_REGISTRY|registry-url|registry)["']?\s*:\s*(?<value>.*)$/iu));
    }
    for (const match of matches.filter(Boolean)) {
      errors.push(...validateRegistryConfigValue(unquoteScalar(match.groups?.value ?? ""), `${sourceName}:${index + 1}: ${match.groups?.key ?? "registry"}`));
    }
  }
  errors.push(...inspectShellRegistryText(sourceName, source));
  if (extension === ".yaml" || extension === ".yml") {
    const yamlFlowPattern = /(?:^|[{,\s])(?<key>NPM_CONFIG_REGISTRY)\s*:\s*(?:"(?<double>[^"]*)"|'(?<single>[^']*)'|(?<bare>[^,}\s]+))/gimu;
    for (const match of source.matchAll(yamlFlowPattern)) {
      errors.push(...validateRegistryConfigValue(unquoteScalar(assignmentValue(match)), `${sourceName}: ${match.groups?.key ?? "NPM_CONFIG_REGISTRY"}`));
    }
  }
  return errors;
}

export function canonicalTarballUrl(packageLocation, version) {
  const marker = "node_modules/";
  const markerIndex = packageLocation.lastIndexOf(marker);
  if (markerIndex < 0 || typeof version !== "string" || version.length === 0) {
    throw new Error(`Lockfile-Eintrag kann nicht normalisiert werden: ${packageLocation}`);
  }
  const packageName = packageLocation.slice(markerIndex + marker.length);
  const tarballName = packageName.split("/").at(-1);
  if (!packageName || !tarballName) throw new Error(`Ungültiger Paketname im Lockfile: ${packageLocation}`);
  return `${PUBLIC_REGISTRY}${packageName}/-/${tarballName}-${version}.tgz`;
}

export function normalizeLockObject(lock) {
  if (!lock || lock.lockfileVersion !== 3 || typeof lock.packages !== "object") {
    throw new Error("Nur npm-package-lock.json im Lockfile-Format 3 wird unterstützt.");
  }
  let changed = 0;
  for (const [location, metadata] of Object.entries(lock.packages)) {
    if (!metadata || typeof metadata.resolved !== "string") continue;
    const canonical = canonicalTarballUrl(location, metadata.version);
    if (metadata.resolved !== canonical) {
      metadata.resolved = canonical;
      changed += 1;
    }
  }
  return changed;
}

function looksLikeRegistryReference(parsed, source, start) {
  const context = source.slice(Math.max(0, start - 80), start + 180).toLowerCase();
  return parsed.hostname.includes("registry")
    || parsed.pathname.endsWith(".tgz")
    || parsed.pathname.includes("/-/")
    || /(?:artifactory|nexus|verdaccio|repository)/iu.test(`${parsed.hostname}${parsed.pathname}`)
    || /\/(?:api\/npm|artifactory|nexus|repository|verdaccio)(?:\/|$)/iu.test(parsed.pathname)
    || /(?:--)?registry\s*(?:=|:)/u.test(context);
}

export function findTextViolations(sourceName, source) {
  const errors = findRegistryAssignments(sourceName, source);
  for (const match of source.matchAll(urlPattern)) {
    const raw = match[0].replace(/[),.;]+$/u, "");
    const parsed = parseUrl(raw);
    if (!parsed) continue;
    const label = `${sourceName}: ${raw}`;
    if (parsed.username || parsed.password) errors.push(`${label}: eingebettete Zugangsdaten sind verboten`);
    if (isPrivateAddress(parsed.hostname) && !isApprovedShippedUrl(sourceName, parsed)) {
      errors.push(`${label}: lokale, private oder interne Adresse ist verboten`);
    }
    if (looksLikeRegistryReference(parsed, source, match.index ?? 0)) {
      errors.push(...validateRegistryUrl(raw, label));
    }
  }
  for (const match of source.matchAll(protocolRelativeUrlPattern)) {
    const raw = match[0].replace(/[),.;]+$/u, "");
    const parsed = parseUrl(`https:${raw}`);
    if (!parsed) continue;
    if (looksLikeRegistryReference(parsed, source, match.index ?? 0)) {
      errors.push(`${sourceName}: ${raw}: protocol-relative Registry- oder interne URL ist verboten`);
    }
  }
  return errors;
}

export function inspectLockObject(lock) {
  const errors = [];
  if (!lock || lock.lockfileVersion !== 3 || typeof lock.packages !== "object") {
    return ["package-lock.json: erwartetes npm-Lockfile-Format 3 fehlt"];
  }
  for (const [location, metadata] of Object.entries(lock.packages)) {
    if (!metadata || typeof metadata.resolved !== "string") continue;
    errors.push(...validateRegistryUrl(metadata.resolved, `package-lock.json:${location}`));
    const expected = canonicalTarballUrl(location, metadata.version);
    if (metadata.resolved !== expected) {
      errors.push(`package-lock.json:${location}: resolved-URL entspricht nicht dem kanonischen öffentlichen Tarball-Pfad`);
    }
  }
  return errors;
}

function readSmallText(file) {
  if (fs.statSync(file).size > 2_000_000) return null;
  const buffer = fs.readFileSync(file);
  if (buffer.includes(0)) return null;
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(buffer);
  } catch {
    return null;
  }
}

export function inspectProject(projectRoot) {
  const lockPath = path.join(projectRoot, "package-lock.json");
  const lock = JSON.parse(fs.readFileSync(lockPath, "utf8"));
  const errors = inspectLockObject(lock);
  for (const file of walk(projectRoot)) {
    const source = readSmallText(file);
    if (source === null) continue;
    const relative = path.relative(projectRoot, file).replaceAll(path.sep, "/");
    errors.push(...findTextViolations(relative, source));
  }
  return [...new Set(errors)];
}

function run() {
  const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
  const projectRoot = path.resolve(scriptDirectory, "..");
  const lockPath = path.join(projectRoot, "package-lock.json");
  if (process.argv.includes("--normalize-lock")) {
    const lock = JSON.parse(fs.readFileSync(lockPath, "utf8"));
    const changed = normalizeLockObject(lock);
    fs.writeFileSync(lockPath, `${JSON.stringify(lock, null, 2)}\n`);
    console.log(`Lockfile kontrolliert normalisiert: ${changed} resolved-URLs geändert.`);
  }
  const errors = inspectProject(projectRoot);
  if (errors.length) {
    console.error(errors.join("\n"));
    process.exitCode = 1;
    return;
  }
  console.log(`Registry-Prüfung erfolgreich: ausschließlich ${PUBLIC_REGISTRY}`);
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) run();
