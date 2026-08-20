import fs from "node:fs";
import path from "node:path";
import { walk } from "./_walk.mjs";

const errors = [];
const legacy = ["styles.css", "launcher-theme.css", "enhancements.css", "atlas.css", "premium-overhaul.css", "s9lab-next.css", "minecraft-client-ui.css"];
for (const file of legacy) if (fs.existsSync(path.join("src", file))) errors.push(`Paralleles Legacy-Stylesheet vorhanden: src/${file}`);
const styleFiles = walk("src/styles").filter((file) => file.endsWith(".css"));
if (styleFiles.length !== 1 || !styleFiles[0].endsWith("index.css")) {
  errors.push("Das kompakte Designsystem muss in src/styles/index.css vereinheitlicht bleiben.");
}
const stylesheet = fs.readFileSync("src/styles/index.css", "utf8");
for (const token of ["--bg:", "--panel:", "--border:", "--text:", "--accent:", "--radius-sm:", "--control:", "--transition:"]) {
  if (!stylesheet.includes(token)) errors.push(`Zentrales Design-Token fehlt: ${token}`);
}

const tauri = JSON.parse(fs.readFileSync("src-tauri/tauri.conf.json", "utf8"));
const mainWindow = tauri.app?.windows?.find((window) => window.label === "main");
if (mainWindow?.minWidth !== 900 || mainWindow?.minHeight !== 580) errors.push("Das Hauptfenster muss eine Mindestgröße von 900 × 580 Pixeln verwenden.");
const forbiddenProductAreas = ["nav.friends", "nav.shop", "nav.coins", "secretReward"];
const messages = fs.readFileSync("src/i18n/messages.ts", "utf8");
for (const marker of forbiddenProductAreas) if (messages.includes(marker)) errors.push(`Nicht freigegebener Produktbereich gefunden: ${marker}`);

const main = fs.readFileSync("src/main.tsx", "utf8");
if (!main.includes('"./styles/index.css"')) errors.push("main.tsx muss ausschließlich das Phase-2-Stylesheet importieren.");
for (const file of walk("src")) {
  if (!/\.(css|ts|tsx)$/.test(file)) continue;
  const text = fs.readFileSync(file, "utf8");
  if (/https?:\/\//i.test(text) && (file.endsWith(".css") || /font/i.test(text))) errors.push(`${file}: externer Laufzeit- oder Fontverweis gefunden.`);
}
if (!stylesheet.includes('[data-reduced-motion="true"]')) errors.push("Benutzerdefinierter Reduced-Motion-Schalter fehlt.");
if (!stylesheet.includes("prefers-reduced-motion")) errors.push("prefers-reduced-motion wird nicht berücksichtigt.");
if (!stylesheet.includes("@media(max-width:760px)")) errors.push("Responsive Kompaktfenster-Grundlage fehlt.");
if (errors.length) { console.error(errors.join("\n")); process.exit(1); }
console.log("Semantisches Designsystem und lokale Laufzeitressourcen erfolgreich geprüft.");
