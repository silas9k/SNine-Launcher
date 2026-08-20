import fs from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseDocument } from "./vendor/yaml.mjs";

const registryGuard = "node scripts/check-public-registry.mjs";
const sourceGuard = "node scripts/check-source-cleanliness.mjs --allow-root-vcs-metadata";

function isObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function isExactGuardStep(step, command) {
  return isObject(step)
    && typeof step.run === "string"
    && step.run.trim() === command
    && !Object.hasOwn(step, "if")
    && step["continue-on-error"] !== true;
}

function executesNpmCi(step) {
  return isObject(step) && typeof step.run === "string" && /(?:^|\s)npm\s+ci(?:\s|$)/u.test(step.run.trim());
}

export function inspectWorkflowText(name, source) {
  let document;
  try {
    document = parseDocument(source, { version: "1.2", uniqueKeys: true, prettyErrors: false });
  } catch (error) {
    return [`${name}: YAML kann nicht ausgewertet werden: ${error instanceof Error ? error.message : String(error)}`];
  }
  if (document.errors.length) {
    return document.errors.map((error) => `${name}: YAML-Parsefehler: ${error.message}`);
  }

  let workflow;
  try {
    workflow = document.toJS({ mapAsMap: false, maxAliasCount: 100 });
  } catch (error) {
    return [`${name}: YAML-Struktur kann nicht aufgelöst werden: ${error instanceof Error ? error.message : String(error)}`];
  }
  if (!isObject(workflow) || !isObject(workflow.jobs)) return [`${name}: auswertbare jobs-Struktur fehlt`];

  const errors = [];
  for (const [jobName, job] of Object.entries(workflow.jobs)) {
    if (!isObject(job)) {
      errors.push(`${name} (${jobName}): Jobstruktur ist nicht auswertbar`);
      continue;
    }
    if (!Object.hasOwn(job, "steps")) continue;
    if (!Array.isArray(job.steps)) {
      errors.push(`${name} (${jobName}): steps-Struktur ist nicht auswertbar`);
      continue;
    }
    for (let index = 0; index < job.steps.length; index += 1) {
      const step = job.steps[index];
      if (!isObject(step)) {
        errors.push(`${name} (${jobName}, Schritt ${index + 1}): Schrittstruktur ist nicht auswertbar`);
        continue;
      }
      if (Object.hasOwn(step, "run") && typeof step.run !== "string") {
        errors.push(`${name} (${jobName}, Schritt ${index + 1}): run ist kein auswertbarer String`);
        continue;
      }
      if (!executesNpmCi(step)) continue;
      const preceding = job.steps.slice(0, index);
      if (!preceding.some((candidate) => isExactGuardStep(candidate, registryGuard))) {
        errors.push(`${name} (${jobName}, Schritt ${index + 1}): ausführbarer Registry-Guard fehlt vor npm ci`);
      }
      if (!preceding.some((candidate) => isExactGuardStep(candidate, sourceGuard))) {
        errors.push(`${name} (${jobName}, Schritt ${index + 1}): ausführbarer Quellsauberkeits-Guard fehlt vor npm ci`);
      }
    }
  }
  return errors;
}

export function inspectWorkflows(directory) {
  const errors = [];
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    if (!entry.isFile() || !/\.ya?ml$/iu.test(entry.name)) continue;
    const full = path.join(directory, entry.name);
    errors.push(...inspectWorkflowText(entry.name, fs.readFileSync(full, "utf8")));
  }
  return errors;
}

function run() {
  const projectRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const errors = inspectWorkflows(path.join(projectRoot, ".github", "workflows"));
  if (errors.length) {
    console.error(errors.join("\n"));
    process.exitCode = 1;
  } else {
    console.log("Workflow-Guard-Reihenfolge erfolgreich mit YAML 1.2 geprüft.");
  }
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) run();
