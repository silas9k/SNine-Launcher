import fs from "node:fs";
import path from "node:path";
import { walk } from "./_walk.mjs";

const extensions = new Set([".ts", ".tsx", ".js", ".mjs", ".rs", ".json", ".toml", ".css", ".html", ".ps1", ".md"]);
const forbidden = ["Ã", "Â", "â€¦", "â€”", "â€“", "ðŸ", "�"];
const decoder = new TextDecoder("utf-8", { fatal: true });
const errors = [];
const selfPath = path.resolve("scripts/check-utf8.mjs");
const docsPath = path.resolve("docs");

for (const file of walk()) {
  const resolvedFile = path.resolve(file);
  if (resolvedFile === selfPath) continue;
  if (resolvedFile === docsPath || resolvedFile.startsWith(`${docsPath}${path.sep}`)) continue; // Dokumentation enthält historische Fehlerbeispiele.
  if (!extensions.has(path.extname(file).toLowerCase())) continue;
  let text;
  try {
    text = decoder.decode(fs.readFileSync(file));
  } catch {
    errors.push(`${file}: keine gültige UTF-8-Datei`);
    continue;
  }
  for (const marker of forbidden) {
    if (text.includes(marker)) errors.push(`${file}: verdächtige Zeichenfolge ${JSON.stringify(marker)}`);
  }
}

if (errors.length) {
  console.error(errors.join("\n"));
  process.exit(1);
}
console.log("UTF-8-Prüfung erfolgreich.");
