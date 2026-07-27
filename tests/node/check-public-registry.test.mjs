import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  canonicalTarballUrl,
  findTextViolations,
  inspectLockObject,
  inspectProject,
  normalizeLockObject,
  validateRegistryConfigValue,
  validateRegistryUrl,
} from "../../scripts/check-public-registry.mjs";

const joinUrl = (...parts) => parts.join("");

test("accepts only canonical public npm tarball URLs", () => {
  const url = canonicalTarballUrl("node_modules/@scope/example", "1.2.3");
  assert.equal(url, "https://registry.npmjs.org/@scope/example/-/example-1.2.3.tgz");
  assert.deepEqual(validateRegistryUrl(url), []);
});

test("rejects insecure, credentialed, private and unapproved registries", () => {
  const fixtures = [
    joinUrl("http", "://registry.npmjs.org/example/-/example-1.0.0.tgz"),
    joinUrl("https://user:secret", "@registry.npmjs.org/example/-/example-1.0.0.tgz"),
    joinUrl("https", "://internal.registry.invalid/npm/example/-/example-1.0.0.tgz"),
    joinUrl("https", "://registry.private.invalid/npm/example/-/example-1.0.0.tgz"),
    joinUrl("https://packages.corporate", ".invalid/npm/example/-/example-1.0.0.tgz"),
    joinUrl("https://registry.npmjs.org", ":444/example/-/example-1.0.0.tgz"),
    joinUrl("https://registry.npmjs.org/example/-/example-1.0.0.tgz", "?token=fixture"),
    joinUrl("https://registry.npmjs.org/example/-/example-1.0.0.tgz", "#fragment"),
  ];
  for (const fixture of fixtures) {
    assert.ok(validateRegistryUrl(fixture).length > 0, fixture);
  }
});

test("accepts only the exact canonical normal and scoped npm registry setting", () => {
  assert.deepEqual(validateRegistryConfigValue("https://registry.npmjs.org/"), []);
  assert.deepEqual(findTextViolations("fixture.npmrc", [
    "registry=https://registry.npmjs.org/",
    "@scope:registry=https://registry.npmjs.org/",
  ].join("\n")), []);
});

test("rejects every non-canonical npm registry setting", () => {
  const forbidden = [
    joinUrl("https://registry.npmjs.org", ":444/"),
    joinUrl("https://registry.npmjs.org", "/not-root/"),
    joinUrl("registry=", "//registry", ".private.invalid/npm/"),
    joinUrl("http", "://registry.npmjs.org/"),
    joinUrl("https://user:secret", "@registry.npmjs.org/"),
    joinUrl("https://registry.npmjs.org/", "?query=1"),
    joinUrl("https://registry.npmjs.org/", "#fragment"),
    "${NPM_REGISTRY}",
  ];
  for (const value of forbidden) {
    const scopedKey = ["@scope:", "registry="].join("");
    const source = value.startsWith("registry=") ? value : `${scopedKey}${value}`;
    assert.ok(findTextViolations("fixture.npmrc", source).length > 0, source);
  }
});

test("rejects generic private and internal URLs in shipped text", () => {
  const fixtures = [
    joinUrl("https", "://10.20.30.40/api/"),
    joinUrl("https", "://service", ".internal.invalid/api/"),
    joinUrl("http", "://localhost:8080/"),
  ];
  for (const fixture of fixtures) {
    assert.ok(findTextViolations("README.md", fixture).length > 0, fixture);
  }
});

test("detects structured JSON registry fields", () => {
  const badPath = joinUrl("https://registry.npmjs.org", "/not-root/");
  const protocolRelative = joinUrl("//registry", ".private.invalid/npm/");
  for (const registry of [badPath, protocolRelative, "${NPM_REGISTRY}"]) {
    const source = JSON.stringify({ publishConfig: { registry } });
    assert.ok(findTextViolations("package.json", source).length > 0, source);
  }
  assert.deepEqual(findTextViolations("package.json", JSON.stringify({
    publishConfig: { registry: "https://registry.npmjs.org/" },
  })), []);
});

test("detects YAML and environment registry variants", () => {
  const badPath = joinUrl("https://registry.npmjs.org", "/not-root/");
  const protocolRelative = joinUrl("//registry", ".private.invalid/npm/");
  const envKey = joinUrl("NPM_CONFIG_", "REGISTRY");
  const fixtures = [
    ["workflow.yml", `env:\n  ${envKey}: ${badPath}`],
    ["workflow.yaml", `registry-url: ${protocolRelative}`],
    ["verify.ps1", `$env:${envKey} = \"${badPath}\"`],
    ["verify.sh", `export ${envKey}=${protocolRelative}`],
    ["README.md", `npm ci ${joinUrl("--reg", "istry=")}${badPath}`],
  ];
  for (const [name, source] of fixtures) assert.ok(findTextViolations(name, source).length > 0, `${name}: ${source}`);
});

test("detects quoted CLI, inline assignments, YAML flow and raw protocol-relative URLs", () => {
  const relative = joinUrl("//registry", ".private.invalid/npm/");
  const badPath = joinUrl("https://registry.npmjs.org", "/not-root/");
  const envKey = joinUrl("NPM_CONFIG_", "REGISTRY");
  const fixtures = [
    ["package.json", JSON.stringify({ scripts: {
      first: `npm ci ${joinUrl("--reg", "istry=")}\"${relative}\"`,
      second: `${envKey}=${relative} npm ci`,
      third: `${envKey}=${badPath} npm ci`,
    } })],
    ["command.txt", `npm ci ${joinUrl("--reg", "istry")} '${relative}'`],
    ["verify.ps1", `if ($true) { $env:${envKey} = \"${relative}\" }`],
    ["workflow.yml", `env: { ${envKey}: \"${relative}\" }`],
    ["README.md", relative],
  ];
  for (const [name, source] of fixtures) assert.ok(findTextViolations(name, source).length > 0, `${name}: ${source}`);
});

function assertExactRegistryViolation(errors, label) {
  assert.ok(errors.some((entry) => entry.includes("Registry-Konfiguration muss exakt https://registry.npmjs.org/ entsprechen")), `${label}: ${errors.join("\n")}`);
}

test("rejects a non-root registry in one package.json script", () => {
  const badPath = joinUrl("https://registry.npmjs.org", "/not-root/");
  const envKey = joinUrl("NPM_CONFIG_", "REGISTRY");
  const errors = findTextViolations("package.json", JSON.stringify({ scripts: { installFixture: `${envKey}=${badPath} npm ci` } }));
  assertExactRegistryViolation(errors, "package.json script");
});

test("accepts the canonical registry in one package.json script", () => {
  const envKey = joinUrl("NPM_CONFIG_", "REGISTRY");
  const source = JSON.stringify({ scripts: { installFixture: `${envKey}=https://registry.npmjs.org/ npm ci` } });
  assert.deepEqual(findTextViolations("package.json", source), []);
});

for (const extension of ["cmd", "bat"]) {
  test(`rejects a non-root registry in set-quoted .${extension} syntax`, () => {
    const badPath = joinUrl("https://registry.npmjs.org", "/not-root/");
    const envKey = joinUrl("NPM_CONFIG_", "REGISTRY");
    const errors = findTextViolations(`fixture.${extension}`, `set "${envKey}=${badPath}"\r\n`);
    assertExactRegistryViolation(errors, `fixture.${extension}`);
  });

  test(`accepts the canonical registry in set-quoted .${extension} syntax`, () => {
    const envKey = joinUrl("NPM_CONFIG_", "REGISTRY");
    assert.deepEqual(findTextViolations(`fixture.${extension}`, `set "${envKey}=https://registry.npmjs.org/"\r\n`), []);
  });
}

function temporaryProject(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "s9-registry-e2e-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.writeFileSync(path.join(root, "package-lock.json"), JSON.stringify({ lockfileVersion: 3, packages: {} }));
  return root;
}

test("inspectProject scans actual shell and command script extensions end to end", (t) => {
  const root = temporaryProject(t);
  const relative = joinUrl("//registry", ".private.invalid/npm/");
  const badPath = joinUrl("https://registry.npmjs.org", "/not-root/");
  const envKey = joinUrl("NPM_CONFIG_", "REGISTRY");
  fs.writeFileSync(path.join(root, "package.json"), JSON.stringify({ scripts: {
    installFixture: `${envKey}=${relative} npm ci`,
  } }));
  const fixtures = new Map([
    ["fixture.sh", `export ${envKey}=${relative}\n`],
    ["fixture.bash", `${envKey}=${badPath} npm ci\n`],
    ["fixture.zsh", `npm ci ${joinUrl("--reg", "istry=")}\"${relative}\"\n`],
    ["fixture.cmd", `set ${envKey}=${relative}\r\n`],
    ["fixture.bat", `npm ci ${joinUrl("--reg", "istry")} '${relative}'\r\n`],
  ]);
  for (const [name, content] of fixtures) fs.writeFileSync(path.join(root, name), content);
  const errors = inspectProject(root);
  for (const name of ["package.json", ...fixtures.keys()]) {
    assert.ok(errors.some((entry) => entry.includes(name)), `${name} was not inspected: ${errors.join("\n")}`);
  }
});

test("inspectProject accepts canonical registry assignments in extensionless scripts", (t) => {
  const root = temporaryProject(t);
  fs.writeFileSync(path.join(root, "package.json"), JSON.stringify({ scripts: {
    installFixture: "NPM_CONFIG_REGISTRY=https://registry.npmjs.org/ npm ci",
  } }));
  const cliKey = joinUrl("--reg", "istry=");
  fs.writeFileSync(path.join(root, "launcher"), `npm ci ${cliKey}\"https://registry.npmjs.org/\"\n`);
  assert.deepEqual(inspectProject(root), []);
});

test("inspectProject rejects one isolated non-root package.json script", (t) => {
  const root = temporaryProject(t);
  const badPath = joinUrl("https://registry.npmjs.org", "/not-root/");
  const envKey = joinUrl("NPM_CONFIG_", "REGISTRY");
  fs.writeFileSync(path.join(root, "package.json"), JSON.stringify({ scripts: { installFixture: `${envKey}=${badPath} npm ci` } }));
  assertExactRegistryViolation(inspectProject(root), "isolated package.json project");
});

for (const extension of ["cmd", "bat"]) {
  test(`inspectProject rejects one isolated set-quoted .${extension} assignment`, (t) => {
    const root = temporaryProject(t);
    const badPath = joinUrl("https://registry.npmjs.org", "/not-root/");
    const envKey = joinUrl("NPM_CONFIG_", "REGISTRY");
    fs.writeFileSync(path.join(root, `fixture.${extension}`), `set "${envKey}=${badPath}"\r\n`);
    assertExactRegistryViolation(inspectProject(root), `isolated .${extension} project`);
  });
}

test("detects reserved hosts and common private registry products and paths", () => {
  const fixtures = [
    joinUrl("https://packages.company", ".example/artifactory/api/npm/npm-public/"),
    joinUrl("https://repo.vendor", ".invalid/repository/npm-group/"),
    joinUrl("https", "://nexus.vendor", ".test/npm/"),
    joinUrl("https", "://verdaccio.vendor", ".example/"),
  ];
  for (const fixture of fixtures) assert.ok(findTextViolations("README.md", fixture).length > 0, fixture);
});

test("detects registry violations in npm configuration text", () => {
  const source = joinUrl("registry=", "http", "://127.0.0.1/npm/");
  assert.ok(findTextViolations("fixture.npmrc", source).length > 0);
});

test("normalizes only resolved URLs and preserves versions and integrities", () => {
  const lock = {
    lockfileVersion: 3,
    packages: {
      "": { name: "fixture", version: "1.0.0" },
      "node_modules/example": {
        version: "2.3.4",
        resolved: joinUrl("https://packages.corporate", ".invalid/npm/example/-/example-2.3.4.tgz"),
        integrity: "sha512-fixture",
      },
    },
  };
  assert.equal(normalizeLockObject(lock), 1);
  assert.equal(lock.packages["node_modules/example"].version, "2.3.4");
  assert.equal(lock.packages["node_modules/example"].integrity, "sha512-fixture");
  assert.deepEqual(inspectLockObject(lock), []);
});

test("rejects non-canonical package-lock resolved paths", () => {
  const lock = {
    lockfileVersion: 3,
    packages: {
      "node_modules/example": {
        version: "2.3.4",
        resolved: joinUrl("https://registry.npmjs.org", "/not-root/example-2.3.4.tgz"),
      },
    },
  };
  assert.ok(inspectLockObject(lock).some((entry) => entry.includes("kanonischen öffentlichen Tarball-Pfad")));
});
