import fs from "node:fs";

const read = (path) => fs.readFileSync(path, "utf8");
const lifecycle = read("TEST-NSIS-LIFECYCLE.ps1");
const verifier = read("VERIFY-PHASE10-WINDOWS.ps1");
const packager = read("CREATE-FINAL-DELIVERY.ps1");
const applier = read("APPLY-S9LAB-DELTA.ps1");
const signatures = read("VERIFY-WINDOWS-SIGNATURES.ps1");
const workflow = read(".github/workflows/release-launcher.yml");
const errors = [];

for (const [label, source, markers] of [
  ["lifecycle", lifecycle, ["/CurrentUser", "/UPDATE", "/NS", "AllowUnsignedDiagnosticInstaller", "preexisting", "uninstall.exe"]],
  ["verifier", verifier, ["check-source-cleanliness.mjs", "check-phase10-windows-release.mjs", "npm", "ci", "test:browser", "test:performance", "1..3", "tauri:build", "ExerciseInstallerLifecycle", "NotSigned"]],
  ["packager", packager, ["git status --short", "git archive", "site.s9lab.delta", "baseSha256", "targetSha256", "APPLY-S9LAB-DELTA.ps1"]],
  ["applier", applier, ["Assert-NoReparseAncestor", "Get-FileHash", "baseCommit", "targetCommit", "Git-Ziel muss", "if ($mutated)", "Remove-Item -LiteralPath"]],
]) {
  for (const marker of markers) if (!source.includes(marker)) errors.push(`missing Phase 10 ${label} invariant: ${marker}`);
}

if (!signatures.includes("Get-AuthenticodeSignature") || !signatures.includes("SignatureStatus]::Valid")) errors.push("strict Authenticode verifier is missing");
if (/Invoke-WebRequest|curl\b|http:\/\//i.test(lifecycle + verifier + packager + applier)) errors.push("unapproved network fallback found in Phase 10 scripts");
if (/Remove-Item\s+[^\r\n]*(?:\*|\$HOME|~)/i.test(lifecycle + verifier + packager + applier)) errors.push("broad destructive cleanup found in Phase 10 scripts");
if (!workflow.includes("check-phase10-windows-release.mjs") || !workflow.includes("TEST-NSIS-LIFECYCLE.ps1") || !workflow.includes("npm run test:browser") || !workflow.includes("npm run test:performance") || !workflow.includes("npm run tauri:build")) {
  errors.push("Windows workflow does not exercise the complete Phase 10 gate");
}
if (/actions\/upload-artifact|gh\s+release|npm\s+publish|git\s+push/i.test(workflow)) errors.push("publication path found in verification-only workflow");

if (errors.length) {
  console.error(errors.join("\n"));
  process.exit(1);
}
console.log("Phase 10: Cleanroom, NSIS-Lifecycle, Hash-Delta, Signatur- und Veröffentlichungsgrenzen erfolgreich geprüft.");
