import fs from "node:fs";
import path from "node:path";

const ignored = new Set([".git", ".idea", ".vscode", "node_modules", "dist", "target", "gen", "phase0-reports"]);

export function walk(root = ".") {
  const files = [];
  const visit = (current) => {
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      if (ignored.has(entry.name)) continue;
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) visit(full);
      else if (entry.isFile()) files.push(full);
    }
  };
  visit(root);
  return files;
}
