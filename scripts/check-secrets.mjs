import fs from "node:fs";
import path from "node:path";
import { walk } from "./_walk.mjs";

const privateExtensions = new Set([".pem", ".p12", ".pfx"]);
const privateNamePatterns = [/\.key$/i, /private[-_. ]?key/i, /signing[-_. ]?key/i];
const privateMarkers = [
  "BEGIN PRIVATE KEY",
  "BEGIN RSA PRIVATE KEY",
  "BEGIN OPENSSH PRIVATE KEY",
  "minisign secret key",
  "untrusted comment: minisign encrypted secret key",
];
const errors = [];
const selfPath = path.resolve("scripts/check-secrets.mjs");

for (const file of walk()) {
  if (path.resolve(file) === selfPath) continue;
  const base = path.basename(file);
  const ext = path.extname(file).toLowerCase();
  if (privateExtensions.has(ext) || privateNamePatterns.some((pattern) => pattern.test(base))) {
    if (!base.endsWith(".key.pub")) errors.push(`${file}: potenzielles privates Schlüsselmaterial`);
  }
  const stat = fs.statSync(file);
  if (stat.size > 2_000_000) continue;
  let text;
  try { text = fs.readFileSync(file, "utf8"); } catch { continue; }
  for (const marker of privateMarkers) {
    if (text.includes(marker)) errors.push(`${file}: privater Schlüsselmarker gefunden`);
  }
}

if (errors.length) {
  console.error([...new Set(errors)].join("\n"));
  process.exit(1);
}
console.log("Secret-Prüfung erfolgreich.");
