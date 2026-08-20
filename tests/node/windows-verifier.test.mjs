import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const verifierPath = new URL("../../VERIFY-PHASE2-V1.0.3-WINDOWS.ps1", import.meta.url);
const wrapperPath = new URL("../../VERIFY-PHASE2-WINDOWS.ps1", import.meta.url);
const combinedVerifierPath = new URL("../../VERIFY-PHASE3-PHASE4-WINDOWS.ps1", import.meta.url);
const verifier = fs.readFileSync(verifierPath, "utf8");
const wrapper = fs.readFileSync(wrapperPath, "utf8");
const combinedVerifier = fs.readFileSync(combinedVerifierPath, "utf8");

test("v1.0.3 verifier uses a short independent runtime TEMP with an explicit path budget", () => {
  assert.match(verifier, /\$runtimeTemp\s*=\s*Join-Path \$systemTempRoot \("s9v-"/u);
  assert.doesNotMatch(verifier, /\$runtimeTemp\s*=\s*Join-Path \$verificationRoot/u);
  assert.match(verifier, /\$successFixtureRelativeUtf16\s*=\s*117/u);
  assert.match(verifier, /\$legacySafeMaxAbsoluteUtf16\s*=\s*247/u);
  assert.match(verifier, /\$projectedSuccessFixtureUtf16\s+-gt\s+\$legacySafeMaxAbsoluteUtf16/u);
});

test("v1.0.3 verifier restores every mutated environment variable in an outer finally", () => {
  for (const name of [
    "TEMP",
    "TMP",
    "NPM_CONFIG_CACHE",
    "NPM_CONFIG_REGISTRY",
    "CARGO_HOME",
    "S9LAB_BROWSER_PATH",
    "S9LAB_VISUAL_OUTPUT",
    "S9LAB_PERFORMANCE_OUTPUT",
  ]) {
    assert.match(verifier, new RegExp(`"${name}"`, "u"));
  }
  assert.match(verifier, /try\s*\{[\s\S]*\}\s*finally\s*\{/u);
  assert.match(verifier, /if \(\$saved\.WasSet\)[\s\S]*Set-Item[\s\S]*else[\s\S]*Remove-Item/u);
});

test("v1.0.3 verifier retains the full parallel Windows gate without serialization", () => {
  assert.match(verifier, /foreach \(\$run in 1\.\.3\)/u);
  assert.match(verifier, /@\("test", "--locked", "--", "--nocapture"\)/u);
  assert.doesNotMatch(verifier, /--test-threads(?:=|\s+)1/u);
  for (const command of [
    "scripts/check-public-registry.mjs",
    "scripts/check-source-cleanliness.mjs",
    "scripts/check-workflow-guards.mjs",
    '"ci", "--registry=https://registry.npmjs.org/"',
    '"run", "test:browser"',
    '"run", "test:performance"',
    '"run", "verify:phase2"',
    '"clippy", "--locked", "--all-targets", "--", "-D", "warnings"',
    '"run", "tauri:build"',
  ]) {
    assert.ok(verifier.includes(command), `missing Windows gate command: ${command}`);
  }
});

test("compatibility wrapper delegates only to the v1.0.3 verifier", () => {
  assert.match(wrapper, /VERIFY-PHASE2-V1\.0\.3-WINDOWS\.ps1/u);
  assert.doesNotMatch(wrapper, /VERIFY-PHASE2-V1\.0\.[12]-WINDOWS\.ps1/u);
  assert.equal(fs.existsSync(new URL("../../VERIFY-PHASE2-V1.0.2-WINDOWS.ps1", import.meta.url)), false);
});

test("combined Phase 3 and 4 verifier binds the cumulative cleanroom archive", () => {
  assert.match(combinedVerifier, /S9Lab-Launcher-Phase3-Phase4-v1\.0-final-source\.zip/u);
  assert.match(combinedVerifier, /\[Parameter\(Mandatory = \$true\)\][\s\S]*\$ExpectedSha256/u);
  assert.match(combinedVerifier, /\$checksumHash\s+-ne\s+\$expectedHash/u);
  assert.match(combinedVerifier, /\$actualHash\s+-ne\s+\$expectedHash/u);
});

test("combined verifier runs both phase gates before public npm installation", () => {
  const install = combinedVerifier.indexOf('Invoke-Checked -Label "Öffentliches npm ci"');
  assert.ok(install > 0);
  for (const guard of [
    "scripts/check-public-registry.mjs",
    "scripts/check-source-cleanliness.mjs",
    "scripts/check-workflow-guards.mjs",
    "scripts/check-phase3-auth-security.mjs",
    "scripts/check-phase4-profile-isolation.mjs",
  ]) {
    const guardIndex = combinedVerifier.indexOf(guard);
    assert.ok(guardIndex >= 0 && guardIndex < install, `${guard} must run before npm ci`);
  }
});

test("combined verifier preserves the full parallel Rust and unsigned NSIS gate", () => {
  assert.match(combinedVerifier, /foreach \(\$run in 1\.\.3\)/u);
  assert.match(combinedVerifier, /@\("test", "--locked", "--", "--nocapture"\)/u);
  assert.doesNotMatch(combinedVerifier, /--test-threads(?:=|\s+)1/u);
  for (const command of [
    '"fmt", "--all", "--", "--check"',
    '"check", "--locked"',
    '"clippy", "--locked", "--all-targets", "--", "-D", "warnings"',
    '"run", "tauri:build"',
  ]) {
    assert.ok(combinedVerifier.includes(command), `missing combined Windows gate command: ${command}`);
  }
  assert.match(combinedVerifier, /Authenticode-Status/u);
  assert.match(combinedVerifier, /Es wurde nichts signiert oder veröffentlicht/u);
});
