import fs from "node:fs";
import path from "node:path";

const packageVersion = JSON.parse(fs.readFileSync("package.json", "utf8")).version;
const tauriVersion = JSON.parse(fs.readFileSync("src-tauri/tauri.conf.json", "utf8")).version;
const cargo = fs.readFileSync("src-tauri/Cargo.toml", "utf8");
const cargoVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const errors = [];

if (!packageVersion || packageVersion !== tauriVersion || packageVersion !== cargoVersion) {
  errors.push(`Versionskonflikt: package=${packageVersion}, tauri=${tauriVersion}, cargo=${cargoVersion}`);
}

const rustRoot = "src-tauri/src";
const rustFiles = fs.readdirSync(rustRoot, { recursive: true })
  .filter((relative) => String(relative).endsWith(".rs"))
  .map((relative) => path.join(rustRoot, String(relative)));
for (const file of rustFiles) {
  const text = fs.readFileSync(file, "utf8");
  if (/S9Lab-Launcher\/\d/.test(text)) errors.push(`${file}: fest codierte User-Agent-Version.`);
  if (/MINECRAFT_LAUNCHER_VERSION",\s*"\d/.test(text)) errors.push(`${file}: fest codierte Launcher-Version.`);
}

if (errors.length) {
  console.error(errors.join("\n"));
  process.exit(1);
}
console.log(`Version ${packageVersion} ist synchron.`);
