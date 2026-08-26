[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$ZipPath,
    [string]$ChecksumPath = "",
    [Parameter(Mandatory = $true)][ValidatePattern('^[A-Fa-f0-9]{64}$')][string]$ExpectedSha256,
    [string]$BrowserPath = "",
    [Parameter(Mandatory = $true)][string]$EvidencePath,
    [string]$InstallerOutputPath = "",
    [switch]$ExerciseInstallerLifecycle,
    [switch]$KeepVerificationRoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
if (Test-Path variable:PSNativeCommandUseErrorActionPreference) { $PSNativeCommandUseErrorActionPreference = $false }

$expectedArchiveName = "S9Lab-Launcher-v1.0.10-final-source.zip"
$expectedRootName = "S9Lab-Launcher-v1.0.10-final-source"
$publicRegistry = "https://registry.npmjs.org/"
$results = [Collections.Generic.List[object]]::new()
$environmentNames = @("TEMP", "TMP", "NPM_CONFIG_CACHE", "NPM_CONFIG_REGISTRY", "CARGO_HOME", "S9LAB_BROWSER_PATH", "S9LAB_VISUAL_OUTPUT", "S9LAB_PERFORMANCE_OUTPUT")
$savedEnvironment = @{}
foreach ($name in $environmentNames) {
    $item = Get-Item -LiteralPath ("Env:" + $name) -ErrorAction SilentlyContinue
    $savedEnvironment[$name] = [pscustomobject]@{ WasSet = $null -ne $item; Value = if ($null -ne $item) { $item.Value } else { $null } }
}
$verificationRoot = $null
$runtimeTemp = $null

function Invoke-Checked {
    param([string]$Label, [string]$File, [string[]]$Arguments, [string]$WorkingDirectory)
    Write-Host "`n[$Label] $File $($Arguments -join ' ')" -ForegroundColor Cyan
    $exitCode = -1
    Push-Location $WorkingDirectory
    try { & $File @Arguments; $exitCode = $LASTEXITCODE } finally { Pop-Location }
    $results.Add([pscustomobject]@{ check = $Label; exitCode = $exitCode })
    if ($exitCode -ne 0) { throw "$Label ist mit Exitcode $exitCode fehlgeschlagen." }
}

try {
    $resolvedZip = (Resolve-Path -LiteralPath $ZipPath).Path
    if ((Split-Path -Leaf $resolvedZip) -cne $expectedArchiveName) { throw "Unerwarteter ZIP-Dateiname: $(Split-Path -Leaf $resolvedZip)" }
    if ([string]::IsNullOrWhiteSpace($ChecksumPath)) { $ChecksumPath = "$resolvedZip.sha256" }
    $resolvedChecksum = (Resolve-Path -LiteralPath $ChecksumPath).Path
    $checksumLine = (Get-Content -LiteralPath $resolvedChecksum -Raw).Trim()
    if ($checksumLine -notmatch '^(?<Hash>[A-Fa-f0-9]{64})\s+\*?(?<Name>.+)$') { throw "Ungültiges SHA-256-Dateiformat." }
    $expectedHash = $ExpectedSha256.ToUpperInvariant()
    if ($Matches.Name.Trim() -cne $expectedArchiveName -or $Matches.Hash.ToUpperInvariant() -ne $expectedHash) { throw "SHA-Datei ist nicht unabhängig an Name und ExpectedSha256 gebunden." }
    $actualHash = (Get-FileHash -LiteralPath $resolvedZip -Algorithm SHA256).Hash
    if ($actualHash -ne $expectedHash) { throw "Quellarchivhash stimmt nicht." }

    $nodeVersion = (& node --version).Trim()
    if ($LASTEXITCODE -ne 0 -or $nodeVersion -notmatch '^v24(?:\.|$)') { throw "Node.js 24 ist erforderlich; gefunden: $nodeVersion" }
    $rustHost = (& rustc -vV) -join "`n"
    if ($LASTEXITCODE -ne 0 -or $rustHost -notmatch '(?im)^host:\s+.*-pc-windows-msvc\s*$') { throw "Ein Windows-MSVC-Rusthost ist erforderlich." }
    if (!(Get-Command cl.exe -ErrorAction SilentlyContinue)) { throw "Die MSVC-Developer-Umgebung ist nicht aktiv (cl.exe fehlt)." }
    $registry = (& npm config get registry).Trim()
    if ($LASTEXITCODE -ne 0 -or $registry -cne $publicRegistry) { throw "Öffentliche npm-Registry erforderlich: $publicRegistry" }

    $systemTemp = [IO.Path]::GetTempPath()
    $verificationRoot = Join-Path $systemTemp ("s9lab-v10-verify-" + [guid]::NewGuid().ToString("N"))
    $runtimeTemp = Join-Path $systemTemp ("s9v10-" + [guid]::NewGuid().ToString("N").Substring(0, 10))
    $extractRoot = Join-Path $verificationRoot "source"
    $npmCache = Join-Path $verificationRoot "npm-cache"
    $cargoHome = Join-Path $verificationRoot "cargo-home"
    New-Item -ItemType Directory -Path $extractRoot, $runtimeTemp, $npmCache, $cargoHome | Out-Null
    Expand-Archive -LiteralPath $resolvedZip -DestinationPath $extractRoot
    $topLevel = @(Get-ChildItem -LiteralPath $extractRoot -Force)
    if ($topLevel.Count -ne 1 -or !$topLevel[0].PSIsContainer -or $topLevel[0].Name -cne $expectedRootName) { throw "Quellarchiv benötigt genau den erwarteten Stammordner." }
    $projectRoot = $topLevel[0].FullName
    $env:TEMP = $runtimeTemp
    $env:TMP = $runtimeTemp
    $env:NPM_CONFIG_CACHE = $npmCache
    $env:NPM_CONFIG_REGISTRY = "https://registry.npmjs.org/"
    $env:CARGO_HOME = $cargoHome

    if ([string]::IsNullOrWhiteSpace($BrowserPath)) {
        $BrowserPath = @(
            "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            "C:\Program Files\Microsoft\Edge\Application\msedge.exe",
            (Join-Path $env:LOCALAPPDATA "Google\Chrome\Application\chrome.exe")
        ) | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf } | Select-Object -First 1
    }
    if ([string]::IsNullOrWhiteSpace($BrowserPath) -or !(Test-Path -LiteralPath $BrowserPath -PathType Leaf)) { throw "Chromium-/Edge-/Chrome-Pfad fehlt." }
    $env:S9LAB_BROWSER_PATH = (Resolve-Path -LiteralPath $BrowserPath).Path
    $env:S9LAB_VISUAL_OUTPUT = Join-Path $verificationRoot "browser-visuals"
    $env:S9LAB_PERFORMANCE_OUTPUT = Join-Path $verificationRoot "browser-performance.json"

    Invoke-Checked "Registry-Guard vor Installation" "node" @("scripts/check-public-registry.mjs") $projectRoot
    Invoke-Checked "Quellpaket-Sauberkeit" "node" @("scripts/check-source-cleanliness.mjs") $projectRoot
    Invoke-Checked "Workflow-Guard vor Installation" "node" @("scripts/check-workflow-guards.mjs") $projectRoot
    foreach ($phase in 1, 3, 4, 5, 6, 7, 8, 9, 10) {
        $scriptName = switch ($phase) {
            1 { "check-phase1-foundation.mjs" }
            3 { "check-phase3-auth-security.mjs" }
            4 { "check-phase4-profile-isolation.mjs" }
            5 { "check-phase5-runtime-security.mjs" }
            6 { "check-phase6-content-security.mjs" }
            7 { "check-phase7-update-security.mjs" }
            8 { "check-phase8-cloud-security.mjs" }
            9 { "check-phase9-player-security.mjs" }
            10 { "check-phase10-windows-release.mjs" }
        }
        Invoke-Checked "Phase-$phase-Guard vor Installation" "node" @("scripts/$scriptName") $projectRoot
    }
    Invoke-Checked "Öffentliches npm ci" "npm" @("ci", "--registry=https://registry.npmjs.org/") $projectRoot
    Invoke-Checked "Vollständige Frontend-/Securitytests" "npm" @("test") $projectRoot
    Invoke-Checked "TypeScript und Produktionsbuild" "npm" @("run", "build") $projectRoot
    Invoke-Checked "Browser, Accessibility und responsive Layouts" "npm" @("run", "test:browser") $projectRoot
    Invoke-Checked "Browser-Performance und Speicher" "npm" @("run", "test:performance") $projectRoot

    $rustRoot = Join-Path $projectRoot "src-tauri"
    Invoke-Checked "Rust-Formatierung" "cargo" @("fmt", "--all", "--", "--check") $rustRoot
    Invoke-Checked "Cargo Check" "cargo" @("check", "--locked") $rustRoot
    Invoke-Checked "Clippy ohne Warnungen" "cargo" @("clippy", "--locked", "--all-targets", "--", "-D", "warnings") $rustRoot
    foreach ($testName in @(
        "crash_recovery_never_leaves_a_mixed_revision",
        "rejects_existing_hardlinks",
        "classifies_verified_windows_junctions_with_the_stable_reparse_error",
        "profile_duplicate_accepts_empty_root_relative_path_without_relaxing_separator_checks",
        "duplicate_of_runtime_profile_clones_v2_revision_with_new_identity",
        "restore_point_is_verified_and_restores_an_isolated_profile_copy",
        "three_way_merge_combines_two_devices_and_requires_manual_conflict_choices"
    )) {
        Invoke-Checked "Windows-Regression $testName" "cargo" @("test", "--locked", $testName, "--", "--nocapture") $rustRoot
    }
    foreach ($run in 1..3) { Invoke-Checked "Paralleler Rust-Gesamtlauf $run/3" "cargo" @("test", "--locked", "--", "--nocapture") $rustRoot }

    Invoke-Checked "Unsignierter Tauri-/NSIS-Releasebuild" "npm" @("run", "tauri:build") $projectRoot
    $nsisDirectory = Join-Path $rustRoot "target\release\bundle\nsis"
    $installers = @(Get-ChildItem -LiteralPath $nsisDirectory -Filter "*.exe" -File)
    if ($installers.Count -ne 1) { throw "Genau ein NSIS-Installer erwartet; gefunden: $($installers.Count)." }
    $installer = $installers[0]
    if ($installer.Name -cne "SNine Launcher_1.0.10_x64-setup.exe") { throw "Unerwarteter Installername: $($installer.Name)" }
    $installerSignature = Get-AuthenticodeSignature -LiteralPath $installer.FullName
    if ($installerSignature.Status -ne [System.Management.Automation.SignatureStatus]::NotSigned) { throw "Diagnoseinstaller sollte NotSigned sein; Status: $($installerSignature.Status)" }
    $installerHash = (Get-FileHash -LiteralPath $installer.FullName -Algorithm SHA256).Hash
    if ($ExerciseInstallerLifecycle) {
        Invoke-Checked "Isolierter NSIS-Install/Wartung/Uninstall-Lifecycle" "pwsh" @("-NoProfile", "-File", (Join-Path $projectRoot "TEST-NSIS-LIFECYCLE.ps1"), "-InstallerPath", $installer.FullName, "-ExpectedSha256", $installerHash, "-AllowUnsignedDiagnosticInstaller") $projectRoot
    }

    $retainedInstallerPath = $installer.FullName
    if (![string]::IsNullOrWhiteSpace($InstallerOutputPath)) {
        $retainedInstallerPath = [IO.Path]::GetFullPath($InstallerOutputPath)
        $retainedParent = Split-Path -Parent $retainedInstallerPath
        if (!(Test-Path -LiteralPath $retainedParent)) { New-Item -ItemType Directory -Path $retainedParent | Out-Null }
        if (Test-Path -LiteralPath $retainedInstallerPath) { throw "InstallerOutputPath existiert bereits: $retainedInstallerPath" }
        Copy-Item -LiteralPath $installer.FullName -Destination $retainedInstallerPath
        if ((Get-FileHash -LiteralPath $retainedInstallerPath -Algorithm SHA256).Hash -ne $installerHash) { throw "Kopierter Diagnoseinstaller stimmt nicht mit dem Cleanroom-Build überein." }
    }

    $performance = Get-Content -LiteralPath $env:S9LAB_PERFORMANCE_OUTPUT -Raw | ConvertFrom-Json
    $os = Get-CimInstance Win32_OperatingSystem
    $evidence = [ordered]@{
        format = "site.s9lab.phase10-evidence"
        formatVersion = 1
        verifiedAt = [DateTime]::UtcNow.ToString("o")
        sourceArchive = $resolvedZip
        sourceSha256 = $actualHash
        windows = "$($os.Caption) $($os.Version) Build $($os.BuildNumber)"
        node = $nodeVersion
        rustHost = (($rustHost -split "`n" | Where-Object { $_ -match '^host:' }) -join "")
        browser = $env:S9LAB_BROWSER_PATH
        checks = @($results)
        performance = $performance
        installer = [ordered]@{ path = $retainedInstallerPath; sizeBytes = $installer.Length; sha256 = $installerHash; authenticode = [string]$installerSignature.Status; lifecycleExercised = [bool]$ExerciseInstallerLifecycle }
        signed = $false
        published = $false
    }
    $evidenceFull = [IO.Path]::GetFullPath($EvidencePath)
    $evidenceParent = Split-Path -Parent $evidenceFull
    if (!(Test-Path -LiteralPath $evidenceParent)) { New-Item -ItemType Directory -Path $evidenceParent | Out-Null }
    [IO.File]::WriteAllText($evidenceFull, ($evidence | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
    Write-Host "`nPhase-10-Cleanroom vollständig bestanden." -ForegroundColor Green
    Write-Host "NSIS: $retainedInstallerPath"
    Write-Host "NSIS SHA-256: $installerHash"
    Write-Host "Authenticode: $($installerSignature.Status)"
    Write-Host "Evidenz: $evidenceFull"
}
finally {
    foreach ($name in $environmentNames) {
        $saved = $savedEnvironment[$name]
        if ($saved.WasSet) { Set-Item -LiteralPath ("Env:" + $name) -Value ([string]$saved.Value) }
        else { Remove-Item -LiteralPath ("Env:" + $name) -ErrorAction SilentlyContinue }
    }
    if (!$KeepVerificationRoot) {
        foreach ($path in @($verificationRoot, $runtimeTemp)) {
            if ($null -ne $path -and (Test-Path -LiteralPath $path)) {
                $resolved = (Resolve-Path -LiteralPath $path).Path
                $tempPrefix = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
                if (-not $resolved.StartsWith($tempPrefix, [StringComparison]::OrdinalIgnoreCase)) { throw "Unsicheres Cleanroom-Aufräumziel: $resolved" }
                Remove-Item -LiteralPath $resolved -Recurse -Force
            }
        }
    }
    elseif ($null -ne $verificationRoot) { Write-Host "Cleanroom bleibt erhalten: $verificationRoot" }
}
