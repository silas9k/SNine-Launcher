import fs from "node:fs";

const source = fs.readFileSync("src/i18n/messages.ts", "utf8");
const block = (name, end) => source.slice(source.indexOf(`export const ${name}`), source.indexOf(end));
const keys = (text) => [...text.matchAll(/^\s*"([^"]+)"\s*:/gm)].map((match) => match[1]);
const germanBlock = block("de", "export type TranslationKey");
const englishBlock = source.slice(source.indexOf("export const en"));
const german = keys(germanBlock);
const english = keys(englishBlock);
const errors = [];
for (const key of german) if (!english.includes(key)) errors.push(`Englische Übersetzung fehlt: ${key}`);
for (const key of english) if (!german.includes(key)) errors.push(`Deutsche Übersetzung fehlt: ${key}`);
for (const duplicate of german.filter((key, index) => german.indexOf(key) !== index)) errors.push(`Doppelter deutscher Schlüssel: ${duplicate}`);
for (const duplicate of english.filter((key, index) => english.indexOf(key) !== index)) errors.push(`Doppelter englischer Schlüssel: ${duplicate}`);
const placeholders = (value) => [...value.matchAll(/\{([a-zA-Z0-9_]+)\}/g)].map((match) => match[1]).sort().join(",");
const entries = (text) => new Map([...text.matchAll(/^\s*"([^"]+)"\s*:\s*"((?:[^"\\]|\\.)*)"/gm)].map((match) => [match[1], match[2]]));
const deEntries = entries(germanBlock);
const enEntries = entries(englishBlock);
for (const [key, value] of deEntries) if (enEntries.has(key) && placeholders(value) !== placeholders(enEntries.get(key))) errors.push(`Interpolationsparameter unterscheiden sich: ${key}`);
for (const marker of ["fuer", "ueber", "oeffnen", "schliessen", "waehlen", "zurueck", "groesse"]) {
  if (new RegExp(`\\b${marker}\\b`, "i").test(germanBlock)) errors.push(`Verdächtige ASCII-Umlautersetzung in Deutsch: ${marker}`);
}
if (errors.length) { console.error(errors.join("\n")); process.exit(1); }
console.log(`${german.length} deutsche und englische Übersetzungsschlüssel sind vollständig und parameterkompatibel.`);
