import fs from "node:fs";
import path from "node:path";
import { walk } from "./_walk.mjs";

const errors = [];
const legacy = ["styles.css", "launcher-theme.css", "enhancements.css", "atlas.css", "premium-overhaul.css", "s9lab-next.css", "minecraft-client-ui.css"];
for (const file of legacy) if (fs.existsSync(path.join("src", file))) errors.push(`Paralleles Legacy-Stylesheet vorhanden: src/${file}`);
for (const file of walk("src/styles")) {
  if (!file.endsWith(".css") || file.endsWith("tokens.css")) continue;
  const text = fs.readFileSync(file, "utf8");
  if (/#[0-9a-fA-F]{3,8}\b/.test(text)) errors.push(`${file}: unkontrollierter Hex-Farbwert außerhalb tokens.css`);
  if (/\b(?:rgb|rgba|hsl|hsla)\s*\(/i.test(text)) errors.push(`${file}: unkontrollierter Farbfunktionswert außerhalb tokens.css`);
}

const tauri = JSON.parse(fs.readFileSync("src-tauri/tauri.conf.json", "utf8"));
const mainWindow = tauri.app?.windows?.find((window) => window.label === "main");
if (mainWindow?.minWidth !== 900 || mainWindow?.minHeight !== 600) errors.push("Das Hauptfenster muss eine Mindestgröße von 900 × 600 Pixeln verwenden.");
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
const layout = fs.readFileSync("src/styles/layout.css", "utf8");
const base = fs.readFileSync("src/styles/base.css", "utf8");
if (!base.includes('[data-reduced-motion="true"]')) errors.push("Benutzerdefinierter Reduced-Motion-Schalter fehlt.");
if (!base.includes("prefers-reduced-motion")) errors.push("prefers-reduced-motion wird nicht berücksichtigt.");
if (!layout.includes("@media (max-width: 960px)")) errors.push("Responsive 900-Pixel-Grundlage fehlt.");
if (errors.length) { console.error(errors.join("\n")); process.exit(1); }
console.log("Semantisches Designsystem und lokale Laufzeitressourcen erfolgreich geprüft.");
