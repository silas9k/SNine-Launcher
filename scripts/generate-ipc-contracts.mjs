import fs from "node:fs";

const contract = JSON.parse(fs.readFileSync("contracts/ipc-contracts.json", "utf8"));
const lines = [
  "// Generated from contracts/ipc-contracts.json. Do not edit manually.",
  `export const IPC_CONTRACT_VERSION = ${contract.version} as const;`,
  "",
  "export interface TypedIpcError {",
  "  code: string;",
  "  messageKey: string;",
  "  params: Record<string, string>;",
  "}",
  "",
];

for (const [typeName, fields] of Object.entries(contract.types)) {
  lines.push(`export interface ${typeName} {`);
  for (const [field, type] of Object.entries(fields)) lines.push(`  ${field}: ${type};`);
  lines.push("}", "");
}

for (const command of contract.commands) {
  const constantName = command.name.toUpperCase();
  lines.push(`export const ${constantName} = "${command.name}" as const;`, "");
}

const output = `${lines.join("\n")}\n`;
const target = "src/lib/generated/ipc-contracts.ts";
if (process.argv.includes("--check")) {
  const current = fs.existsSync(target)
    ? fs.readFileSync(target, "utf8").replace(/\r\n?/g, "\n")
    : "";
  if (current !== output) {
    console.error(`${target} ist nicht aktuell. Führe npm run generate:ipc aus.`);
    process.exit(1);
  }
  console.log("Generierte IPC-Verträge sind aktuell.");
} else {
  fs.mkdirSync("src/lib/generated", { recursive: true });
  fs.writeFileSync(target, output, "utf8");
  console.log(`${target} aktualisiert.`);
}
