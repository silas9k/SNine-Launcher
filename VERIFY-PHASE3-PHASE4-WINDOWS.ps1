[CmdletBinding()]
param(
    [string]$ZipPath = (Join-Path $PSScriptRoot "S9Lab-Launcher-Phase3-Phase4-v1.0-final-source.zip"),
    [string]$ChecksumPath = "",
    [string]$BrowserPath = "",
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[A-Fa-f0-9]{64}$')]
    [string]$ExpectedSha256
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
if (Test-Path variable:PSNativeCommandUseErrorActionPreference) {
    $PSNativeCommandUseErrorActionPreference = $false
}

$expectedArchiveName = "S9Lab-Launcher-Phase3-Phase4-v1.0-final-source.zip"
$expectedRootName = "S9Lab-Launcher-Phase3-Phase4-v1.0-final-source"
$publicRegistry = "https://registry.npmjs.org/"
$results = [System.Collections.Generic.List[object]]::new()
$environmentVariableNames = @(
    "TEMP",
    "TMP",
    "NPM_CONFIG_CACHE",
    "NPM_CONFIG_REGISTRY",
    "CARGO_HOME",
    "S9LAB_BROWSER_PATH",
    "S9LAB_VISUAL_OUTPUT",
    "S9LAB_PERFORMANCE_OUTPUT"
)
$savedEnvironment = @{}
foreach ($name in $environmentVariableNames) {
    $item = Get-Item -LiteralPath ("Env:" + $name) -ErrorAction SilentlyContinue
    $wasSet = ($null -ne $item)
    $value = $null
    if ($wasSet) {
        $value = $item.Value
    }
    $savedEnvironment[$name] = [pscustomobject]@{
        WasSet = $wasSet
        Value = $value
    }
}
$systemTempRoot = [IO.Path]::GetTempPath()

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)][string]$Label,
        [Parameter(Mandatory = $true)][string]$File,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$WorkingDirectory
    )
    Write-Host "`n[$Label] $File $($Arguments -join ' ')" -ForegroundColor Cyan
    $exitCode = -1
    Push-Location $WorkingDirectory
    try {
        & $File @Arguments
        $exitCode = $LASTEXITCODE
    }
    finally {
        Pop-Location
    }
    $results.Add([pscustomobject]@{ Check = $Label; ExitCode = $exitCode })
    if ($exitCode -ne 0) {
        throw "$Label ist mit Exitcode $exitCode fehlgeschlagen."
    }
}

try {
$resolvedZip = (Resolve-Path -LiteralPath $ZipPath).Path
if ((Split-Path -Leaf $resolvedZip) -ne $expectedArchiveName) {
    throw "Unerwarteter ZIP-Dateiname. Erwartet: $expectedArchiveName"
}
if ([string]::IsNullOrWhiteSpace($ChecksumPath)) {
    $ChecksumPath = "$resolvedZip.sha256"
}
$resolvedChecksum = (Resolve-Path -LiteralPath $ChecksumPath).Path
$checksumLine = (Get-Content -LiteralPath $resolvedChecksum -Raw).Trim()
if ($checksumLine -notmatch '^(?<Hash>[A-Fa-f0-9]{64})\s+\*?(?<Name>.+)$') {
    throw "Die SHA-256-Datei hat nicht das erwartete Format."
}
$checksumHash = $Matches.Hash.ToUpperInvariant()
$expectedHash = $ExpectedSha256.ToUpperInvariant()
$checksumName = $Matches.Name.Trim()
if ($checksumName -ne $expectedArchiveName) {
    throw "Die SHA-256-Datei nennt nicht den erwarteten ZIP-Dateinamen."
}
$actualHash = (Get-FileHash -LiteralPath $resolvedZip -Algorithm SHA256).Hash.ToUpperInvariant()
if ($checksumHash -ne $expectedHash) {
    throw "SHA-256-Datei stimmt nicht mit dem unabhängigen ExpectedSha256 überein. ExpectedSha256: $expectedHash; SHA-Datei: $checksumHash"
}
if ($actualHash -ne $expectedHash) {
    throw "SHA-256 stimmt nicht überein. Erwartet: $expectedHash; erhalten: $actualHash"
}
Write-Host "ZIP vor dem Entpacken bestätigt: $actualHash" -ForegroundColor Green

$nodeVersion = (& node --version).Trim()
$nodeExitCode = $LASTEXITCODE
$results.Add([pscustomobject]@{ Check = "Node-Vorprüfung"; ExitCode = $nodeExitCode })
if ($nodeExitCode -ne 0) { throw "Node-Version konnte nicht ermittelt werden (Exitcode $nodeExitCode)." }
if ($nodeVersion -notmatch '^v24(?:\.|$)') { throw "Node.js 24 ist erforderlich; gefunden: $nodeVersion" }

$rustVerbose = (& rustc -vV) -join "`n"
$rustExitCode = $LASTEXITCODE
$results.Add([pscustomobject]@{ Check = "rustc-Host-Vorprüfung"; ExitCode = $rustExitCode })
if ($rustExitCode -ne 0) { throw "rustc-Host konnte nicht ermittelt werden (Exitcode $rustExitCode)." }
if ($rustVerbose -notmatch '(?im)^host:\s+.*-pc-windows-msvc\s*$') {
    throw "Ein echter Windows-MSVC-rustc-Host ist erforderlich."
}

$effectiveRegistry = (& npm config get registry).Trim()
$registryExitCode = $LASTEXITCODE
$results.Add([pscustomobject]@{ Check = "npm-Registry-Vorprüfung"; ExitCode = $registryExitCode })
if ($registryExitCode -ne 0) { throw "Effektive npm-Registry konnte nicht ermittelt werden (Exitcode $registryExitCode)." }
if ($effectiveRegistry -cne $publicRegistry) {
    throw "Die effektive npm-Registry muss exakt $publicRegistry sein; gefunden: $effectiveRegistry"
}
Write-Host "Vorprüfung bestätigt: $nodeVersion, Windows-MSVC-rustc, $effectiveRegistry" -ForegroundColor Green

$verificationRoot = Join-Path $systemTempRoot ("S9Lab-Phase3-Phase4-Verify-" + [guid]::NewGuid().ToString("N"))
if (Test-Path -LiteralPath $verificationRoot) {
    throw "Das neue Prüfverzeichnis existiert unerwartet bereits: $verificationRoot"
}
New-Item -ItemType Directory -Path $verificationRoot | Out-Null
$extractRoot = Join-Path $verificationRoot "source"
New-Item -ItemType Directory -Path $extractRoot | Out-Null
Expand-Archive -LiteralPath $resolvedZip -DestinationPath $extractRoot
$topLevel = @(Get-ChildItem -LiteralPath $extractRoot -Force)
if ($topLevel.Count -ne 1 -or !$topLevel[0].PSIsContainer -or $topLevel[0].Name -ne $expectedRootName) {
    throw "Das ZIP muss genau das Stammverzeichnis $expectedRootName enthalten."
}
$projectRoot = $topLevel[0].FullName

$runtimeTemp = Join-Path $systemTempRoot ("s9v-" + [guid]::NewGuid().ToString("N").Substring(0, 12))
$npmCache = Join-Path $verificationRoot "npm-cache"
$cargoHome = Join-Path $verificationRoot "cargo-home"
if (Test-Path -LiteralPath $runtimeTemp) {
    throw "Der neue kurze Runtime-TEMP-Pfad existiert unerwartet bereits: $runtimeTemp"
}
$runtimeTempUtf16 = [Text.Encoding]::Unicode.GetByteCount($runtimeTemp) / 2
$fixtureRootOverheadUtf16 = 40
$successFixtureRelativeUtf16 = 117
$legacySafeMaxAbsoluteUtf16 = 247
$projectedSuccessFixtureUtf16 = $runtimeTempUtf16 + 1 + $fixtureRootOverheadUtf16 + 1 + $successFixtureRelativeUtf16
if ($projectedSuccessFixtureUtf16 -gt $legacySafeMaxAbsoluteUtf16) {
    throw "Der kurze Runtime-TEMP-Pfad lässt nicht genug Budget für gültige Erfolgs-Fixtures: TEMP=$runtimeTempUtf16, projiziert=$projectedSuccessFixtureUtf16, Maximum=$legacySafeMaxAbsoluteUtf16"
}
New-Item -ItemType Directory -Path $runtimeTemp, $npmCache, $cargoHome | Out-Null
Write-Host "Kurzer Runtime-TEMP bestätigt: $runtimeTemp (UTF-16: $runtimeTempUtf16; projizierte Erfolgs-Fixture: $projectedSuccessFixtureUtf16/$legacySafeMaxAbsoluteUtf16)" -ForegroundColor Green
$env:TEMP = $runtimeTemp
$env:TMP = $runtimeTemp
$env:NPM_CONFIG_CACHE = $npmCache
$env:NPM_CONFIG_REGISTRY = "https://registry.npmjs.org/"
$env:CARGO_HOME = $cargoHome

if ([string]::IsNullOrWhiteSpace($BrowserPath)) {
    $browserCandidates = @(
        "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        "C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        (Join-Path $env:LOCALAPPDATA "Google\Chrome\Application\chrome.exe")
    )
    $BrowserPath = $browserCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
}
if ([string]::IsNullOrWhiteSpace($BrowserPath) -or !(Test-Path -LiteralPath $BrowserPath)) {
    throw "Kein Chromium-/Edge-/Chrome-Browser gefunden. Übergib -BrowserPath."
}
$env:S9LAB_BROWSER_PATH = (Resolve-Path -LiteralPath $BrowserPath).Path
$env:S9LAB_VISUAL_OUTPUT = Join-Path $verificationRoot "browser-visuals"
$env:S9LAB_PERFORMANCE_OUTPUT = Join-Path $verificationRoot "browser-performance.json"

$os = Get-CimInstance Win32_OperatingSystem
Write-Host "Windows: $($os.Caption) $($os.Version) Build $($os.BuildNumber)"
Write-Host "Browser: $env:S9LAB_BROWSER_PATH ($((Get-Item -LiteralPath $env:S9LAB_BROWSER_PATH).VersionInfo.FileVersion))"
Invoke-Checked -Label "Node-Version" -File "node" -Arguments @("--version") -WorkingDirectory $projectRoot
Invoke-Checked -Label "npm-Version" -File "npm" -Arguments @("--version") -WorkingDirectory $projectRoot
Invoke-Checked -Label "Rust-Version" -File "rustc" -Arguments @("--version") -WorkingDirectory $projectRoot
Invoke-Checked -Label "Cargo-Version" -File "cargo" -Arguments @("--version") -WorkingDirectory $projectRoot
Invoke-Checked -Label "rustfmt-Version" -File "rustfmt" -Arguments @("--version") -WorkingDirectory $projectRoot
Invoke-Checked -Label "Clippy-Version" -File "cargo" -Arguments @("clippy", "--version") -WorkingDirectory $projectRoot

Invoke-Checked -Label "Registry-Guard vor Installation" -File "node" -Arguments @("scripts/check-public-registry.mjs") -WorkingDirectory $projectRoot
Invoke-Checked -Label "Quellpaket-Sauberkeit" -File "node" -Arguments @("scripts/check-source-cleanliness.mjs") -WorkingDirectory $projectRoot
Invoke-Checked -Label "Workflow-Guard vor Installation" -File "node" -Arguments @("scripts/check-workflow-guards.mjs") -WorkingDirectory $projectRoot
Invoke-Checked -Label "Phase-3-Authentifizierungs- und Sicherheitsgate vor Installation" -File "node" -Arguments @("scripts/check-phase3-auth-security.mjs") -WorkingDirectory $projectRoot
Invoke-Checked -Label "Phase-4-Profilisolations- und Cachegate vor Installation" -File "node" -Arguments @("scripts/check-phase4-profile-isolation.mjs") -WorkingDirectory $projectRoot
Invoke-Checked -Label "Öffentliches npm ci" -File "npm" -Arguments @("ci", "--registry=https://registry.npmjs.org/") -WorkingDirectory $projectRoot
Invoke-Checked -Label "Vollständige Frontend-Tests" -File "npm" -Arguments @("test") -WorkingDirectory $projectRoot
Invoke-Checked -Label "TypeScript und Produktionsbuild" -File "npm" -Arguments @("run", "build") -WorkingDirectory $projectRoot
Invoke-Checked -Label "Browser, Accessibility, Layout und visuelle Regression" -File "npm" -Arguments @("run", "test:browser") -WorkingDirectory $projectRoot
Invoke-Checked -Label "Browser-Performance" -File "npm" -Arguments @("run", "test:performance") -WorkingDirectory $projectRoot
Invoke-Checked -Label "Erhaltenes Phase-0-bis-2-Verifikationsskript" -File "npm" -Arguments @("run", "verify:phase2") -WorkingDirectory $projectRoot

$rustRoot = Join-Path $projectRoot "src-tauri"
Invoke-Checked -Label "Rust-Formatierung" -File "cargo" -Arguments @("fmt", "--all", "--", "--check") -WorkingDirectory $rustRoot
Invoke-Checked -Label "Cargo Check" -File "cargo" -Arguments @("check", "--locked") -WorkingDirectory $rustRoot
Invoke-Checked -Label "Clippy ohne Warnungen" -File "cargo" -Arguments @("clippy", "--locked", "--all-targets", "--", "-D", "warnings") -WorkingDirectory $rustRoot
foreach ($run in 1..3) {
    Invoke-Checked -Label "Paralleler Rust-Gesamtlauf $run/3" -File "cargo" -Arguments @("test", "--locked", "--", "--nocapture") -WorkingDirectory $rustRoot
}

Invoke-Checked -Label "Lokaler unsignierter Tauri-/NSIS-Build" -File "npm" -Arguments @("run", "tauri:build") -WorkingDirectory $projectRoot
$nsisDirectory = Join-Path $rustRoot "target\release\bundle\nsis"
$installers = @(Get-ChildItem -LiteralPath $nsisDirectory -Filter "*.exe" -File)
if ($installers.Count -ne 1) {
    throw "Genau ein NSIS-Bundle wurde erwartet; gefunden: $($installers.Count)."
}
$architecture = switch ($env:PROCESSOR_ARCHITECTURE) {
    "AMD64" { "x64" }
    "ARM64" { "arm64" }
    "x86" { "x86" }
    default { throw "Nicht unterstützte Windows-Architektur: $env:PROCESSOR_ARCHITECTURE" }
}
$expectedInstallerName = "S9Lab Launcher_1.0.8_$architecture-setup.exe"
if ($installers[0].Name -ne $expectedInstallerName) {
    throw "Unerwartetes NSIS-Bundle. Erwartet: $expectedInstallerName; gefunden: $($installers[0].Name)"
}
$installerSignature = Get-AuthenticodeSignature -LiteralPath $installers[0].FullName
if ($installerSignature.Status -ne "NotSigned") {
    throw "Der reine Diagnose-Build sollte unsigniert sein; Authenticode-Status: $($installerSignature.Status)"
}
$installerHash = (Get-FileHash -LiteralPath $installers[0].FullName -Algorithm SHA256).Hash

Write-Host "`nAlle Prüfungen mit Exitcode 0:" -ForegroundColor Green
$results | Format-Table -AutoSize
Write-Host "NSIS-Diagnosebundle: $($installers[0].FullName)"
Write-Host "NSIS SHA-256: $installerHash"
Write-Host "Authenticode: $($installerSignature.Status)"
Write-Host "Visuelle Ergebnisse: $env:S9LAB_VISUAL_OUTPUT"
Write-Host "Performance-Ergebnis: $env:S9LAB_PERFORMANCE_OUTPUT"
Write-Host "Prüfverzeichnis bleibt zur Nachvollziehbarkeit erhalten: $verificationRoot"
Write-Host "Kurzer Runtime-TEMP bleibt zur Nachvollziehbarkeit erhalten: $runtimeTemp"
Write-Host "Phase 3 und Phase 4 wurden vollständig geprüft. Phase 5 wurde nicht begonnen." -ForegroundColor Green
Write-Host "Es wurde nichts signiert oder veröffentlicht." -ForegroundColor Green
}
finally {
    foreach ($name in $environmentVariableNames) {
        $saved = $savedEnvironment[$name]
        if ($saved.WasSet) {
            Set-Item -LiteralPath ("Env:" + $name) -Value ([string]$saved.Value)
        }
        else {
            Remove-Item -LiteralPath ("Env:" + $name) -ErrorAction SilentlyContinue
        }
    }
}
