import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const blockedDirectories = new Set([
  ".git", ".idea", ".vscode", "artifacts", "coverage", "dist", "gen", "node_modules",
  "playwright-report", "screenshots", "target", "test-results",
]);
const blockedExtensions = new Set([
  ".bak", ".backup", ".dmp", ".exe", ".key", ".log", ".msi", ".old", ".orig",
  ".p12", ".patch", ".pem", ".pfx", ".rej", ".save", ".swo", ".swp", ".temp", ".tmp",
]);
const blockedNames = new Set(["thumbs.db", ".ds_store"]);

export function inspectSourceTree(root, options = {}) {
  const errors = [];
  let fileCount = 0;
  let totalBytes = 0;
  const visit = (current) => {
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const full = path.join(current, entry.name);
      const relative = path.relative(root, full).replaceAll(path.sep, "/");
      const normalizedName = entry.name.toLowerCase();
      if (entry.isSymbolicLink()) {
        errors.push(`${relative}: symbolische Verknüpfungen sind im Quellpaket nicht erlaubt`);
        continue;
      }
      if (entry.isDirectory()) {
        if (normalizedName === ".git" && relative.toLowerCase() === ".git" && options.allowRootVcsMetadata) {
          continue;
        }
        if (blockedDirectories.has(normalizedName)) {
          errors.push(`${relative}: unerlaubtes Build-, Test- oder Metadatenverzeichnis`);
        } else {
          visit(full);
        }
        continue;
      }
      if (!entry.isFile()) continue;
      if (normalizedName === ".git") {
        if (relative.toLowerCase() === ".git" && options.allowRootVcsMetadata) continue;
        errors.push(`${relative}: unerlaubte Versionskontroll-Metadaten`);
        continue;
      }
      fileCount += 1;
      totalBytes += fs.statSync(full).size;
      const extension = path.extname(entry.name).toLowerCase();
      if (blockedNames.has(normalizedName) || blockedExtensions.has(extension)) {
        errors.push(`${relative}: unerlaubtes Artefakt oder privates Material`);
      }
      if (/~$|^#.*#$|^\.~lock\.|\.(?:bak|backup|old|orig|patch|rej|save|swo|swp|temp|tmp)$/iu.test(entry.name)) {
        errors.push(`${relative}: Backup- oder Patchrest ist nicht erlaubt`);
      }
    }
  };
  visit(root);
  return { errors, fileCount, totalBytes };
}

function run() {
  const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
  const projectRoot = path.resolve(scriptDirectory, "..");
  const allowedArguments = new Set(["--allow-root-vcs-metadata"]);
  const unknownArguments = process.argv.slice(2).filter((argument) => !allowedArguments.has(argument));
  if (unknownArguments.length) {
    console.error(`Unbekannte Argumente: ${unknownArguments.join(", ")}`);
    process.exit(2);
  }
  const result = inspectSourceTree(projectRoot, {
    allowRootVcsMetadata: process.argv.includes("--allow-root-vcs-metadata"),
  });
  if (result.errors.length) {
    console.error([...new Set(result.errors)].join("\n"));
    process.exitCode = 1;
  } else {
    console.log(`Quellpaket-Sauberkeit erfolgreich: ${result.fileCount} Dateien, ${result.totalBytes} Bytes.`);
  }
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) run();
