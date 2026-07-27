import fs from "node:fs";
import path from "node:path";

const root = "src-tauri/src";
const rustFiles = [];
const walk = (dir) => {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full);
    else if (entry.isFile() && entry.name.endsWith(".rs")) rustFiles.push(full);
  }
};
walk(root);

const errors = [];
for (const file of rustFiles) {
  const source = fs.readFileSync(file, "utf8");
  for (const match of source.matchAll(/include_bytes!\("([^"]+)"\)/g)) {
    const included = path.normalize(path.join(path.dirname(file), match[1]));
    if (!fs.existsSync(included)) errors.push(`${file}: include_bytes-Ziel fehlt: ${match[1]}`);
  }

  const fileName = path.basename(file);
  if (fileName === "mod.rs" || fileName === "lib.rs" || fileName === "main.rs") {
    for (const match of source.matchAll(/^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([a-zA-Z0-9_]+)\s*;/gm)) {
      const name = match[1];
      const base = path.dirname(file);
      if (!fs.existsSync(path.join(base, `${name}.rs`)) && !fs.existsSync(path.join(base, name, "mod.rs"))) {
        errors.push(`${file}: Moduldatei fehlt für ${name}`);
      }
    }
  }
}

const joined = rustFiles.map((file) => fs.readFileSync(file, "utf8")).join("\n");
for (const forbidden of ["client_update::", "mod client_update;", "rewards::", "mod rewards;"]) {
  if (joined.includes(forbidden)) errors.push(`Entfernter unsicherer Modulpfad weiterhin vorhanden: ${forbidden}`);
}

const lib = fs.readFileSync("src-tauri/src/lib.rs", "utf8");
for (const forbiddenTestCommand of ["execute_with_injector", "execute_controlled_with_injector", "FailAt"]) {
  if (lib.includes(forbiddenTestCommand)) {
    errors.push(`Test-/Failure-Injection darf nicht im Produktions-IPC registriert sein: ${forbiddenTestCommand}`);
  }
}

if (errors.length) {
  console.error([...new Set(errors)].join("\n"));
  process.exit(1);
}
console.log(`${rustFiles.length} Rust-Dateien statisch geprüft.`);
