import fs from "node:fs";
import path from "node:path";
import ts from "typescript";
import { walk } from "./_walk.mjs";

const errors = [];
const visibleAttributes = new Set(["alt", "aria-label", "placeholder", "title"]);
for (const file of walk("src")) {
  if (!file.endsWith(".tsx")) continue;
  const sourceText = fs.readFileSync(file, "utf8");
  const source = ts.createSourceFile(file, sourceText, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  const visit = (node) => {
    if (ts.isJsxText(node) && /[A-Za-zÀ-ž]/.test(node.getText(source))) errors.push(`${file}:${source.getLineAndCharacterOfPosition(node.pos).line + 1}: sichtbarer Rohtext ${JSON.stringify(node.getText(source).trim())}`);
    if (ts.isJsxAttribute(node) && visibleAttributes.has(node.name.getText(source)) && node.initializer && ts.isStringLiteral(node.initializer) && node.initializer.text.length > 0) errors.push(`${file}:${source.getLineAndCharacterOfPosition(node.pos).line + 1}: sichtbares Attribut ${node.name.getText(source)} muss übersetzt werden`);
    ts.forEachChild(node, visit);
  };
  visit(source);
}
if (errors.length) { console.error(errors.join("\n")); process.exit(1); }
console.log("Keine unerlaubten sichtbaren Rohtexte in TSX gefunden.");
