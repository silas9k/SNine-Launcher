import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { spawnSync } from "node:child_process";

test("phase10 project passes the Windows release guard", () => {
  const result = spawnSync(process.execPath, ["scripts/check-phase10-windows-release.mjs"], { encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr || result.stdout);
});

test("phase10 PowerShell delivery scripts are syntactically valid", () => {
  const files = ["APPLY-S9LAB-DELTA.ps1", "CREATE-FINAL-DELIVERY.ps1", "TEST-NSIS-LIFECYCLE.ps1", "VERIFY-PHASE10-WINDOWS.ps1"];
  const command = `$failed=$false; @(${files.map((file) => `'${file}'`).join(",")}) | ForEach-Object { $t=$null;$e=$null;[void][System.Management.Automation.Language.Parser]::ParseFile((Resolve-Path $_),[ref]$t,[ref]$e); if($e.Count){$failed=$true;$e|ForEach-Object{Write-Error $_.Message}} }; if($failed){exit 1}`;
  const result = spawnSync("pwsh", ["-NoProfile", "-Command", command], { encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr || result.stdout);
});

test("delta applier requires clean bound input and rejects reparse traversal", () => {
  const source = fs.readFileSync("APPLY-S9LAB-DELTA.ps1", "utf8");
  assert.match(source, /git -C \$resolvedRoot status --short/);
  assert.match(source, /Assert-NoReparseAncestor/);
  assert.match(source, /baseSha256/);
  assert.match(source, /targetSha256/);
  assert.doesNotMatch(source, /Remove-Item\s+[^\r\n]*(?:\*|\$HOME|~)/i);
});
