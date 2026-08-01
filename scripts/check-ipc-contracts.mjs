import fs from "node:fs";

const wrapperFiles = [
  ...fs.readdirSync("src/lib")
    .filter((file) => file.endsWith("Commands.ts"))
    .map((file) => `src/lib/${file}`),
];
const ts = [...new Set(wrapperFiles)].map((file) => fs.readFileSync(file, "utf8")).join("\n");
const generated = fs.readFileSync("src/lib/generated/ipc-contracts.ts", "utf8");
const rust = fs.readFileSync("src-tauri/src/lib.rs", "utf8");
const contract = JSON.parse(fs.readFileSync("contracts/ipc-contracts.json", "utf8"));

const constants = new Map(
  [...generated.matchAll(/export const ([A-Z0-9_]+) = "([a-z0-9_]+)" as const;/g)]
    .map((match) => [match[1], match[2]]),
);
const invoked = new Set(
  [...ts.matchAll(/invoke(?:<[^>]+>)?\(\s*(?:"([a-z0-9_]+)"|([A-Z0-9_]+))/g)]
    .map((match) => match[1] ?? constants.get(match[2]))
    .filter(Boolean),
);
const handled = new Set([
  ...[...rust.matchAll(/commands::([a-z0-9_]+)/g)].map((match) => match[1]),
  ...[...rust.matchAll(/ipc::([a-z0-9_]+)/g)].map((match) => match[1]),
]);
const errors = [];

for (const command of invoked) {
  if (!handled.has(command)) errors.push(`Frontend-Command ohne Rust-Handler: ${command}`);
}
for (const command of contract.commands) {
  if (!invoked.has(command.name)) errors.push(`Shared Contract ohne Frontend-Wrapper: ${command.name}`);
  if (!handled.has(command.name)) errors.push(`Shared Contract ohne Rust-Handler: ${command.name}`);
}
for (const command of handled) {
  if (!invoked.has(command) && !command.startsWith("window_")) {
    console.warn(`Hinweis: Rust-Command ohne zentralen Frontend-Wrapper: ${command}`);
  }
}

if (errors.length) {
  console.error(errors.join("\n"));
  process.exit(1);
}
console.log(`${invoked.size} IPC-Verträge erfolgreich geprüft; ${contract.commands.length} gemeinsam typisiert.`);
