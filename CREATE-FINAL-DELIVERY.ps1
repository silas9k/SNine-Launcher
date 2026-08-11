[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [ValidatePattern('^[a-fA-F0-9]{40}$')]
    [string]$BaseCommit = "e1412af46abfcd6dc401d4d97c5c3c402ba1491b",
    [string]$InstallerPath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

if ((& git status --short) -join "`n") { throw "Lieferartefakte dürfen nur aus einem sauberen Git-Stand erzeugt werden." }
$targetCommit = (& git rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $targetCommit -notmatch '^[a-f0-9]{40}$') { throw "Zielcommit konnte nicht bestimmt werden." }
& git cat-file -e "$BaseCommit^{commit}"
if ($LASTEXITCODE -ne 0) { throw "Basiscommit ist nicht verfügbar: $BaseCommit" }

$outputFull = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $outputFull) { throw "Ausgabeverzeichnis existiert bereits: $outputFull" }
New-Item -ItemType Directory -Path $outputFull | Out-Null
$sourceName = "S9Lab-Launcher-v1.0.8-final-source.zip"
$sourceRoot = "S9Lab-Launcher-v1.0.8-final-source"
$deltaName = "S9Lab-Launcher-v1.0.8-delta-from-e1412af.zip"
$sourceZip = Join-Path $outputFull $sourceName
$deltaZip = Join-Path $outputFull $deltaName
$workRoot = Join-Path ([IO.Path]::GetTempPath()) ("s9lab-delivery-" + [guid]::NewGuid().ToString("N"))
$baseRoot = Join-Path $workRoot "base"
$deltaRoot = Join-Path $workRoot "delta"
$payloadRoot = Join-Path $deltaRoot "files"
New-Item -ItemType Directory -Path $baseRoot, $payloadRoot | Out-Null

function Assert-RelativeRepositoryPath {
    param([string]$Path)
    if ([string]::IsNullOrWhiteSpace($Path) -or $Path -match '(^|/)(\.|\.\.)(/|$)' -or $Path -match '[:\\\x00-\x1f]' -or [IO.Path]::IsPathRooted($Path)) {
        throw "Unsicherer Repositorypfad: $Path"
    }
}

try {
    & git archive --format=zip "--prefix=$sourceRoot/" "--output=$sourceZip" $targetCommit
    if ($LASTEXITCODE -ne 0 -or !(Test-Path -LiteralPath $sourceZip -PathType Leaf)) { throw "Quellarchiv konnte nicht erzeugt werden." }

    $baseZip = Join-Path $workRoot "base.zip"
    & git archive --format=zip "--output=$baseZip" $BaseCommit
    if ($LASTEXITCODE -ne 0) { throw "Basisarchiv konnte nicht erzeugt werden." }
    Expand-Archive -LiteralPath $baseZip -DestinationPath $baseRoot

    $changed = @(& git diff --no-renames --name-only --diff-filter=ACM $BaseCommit $targetCommit --) | Where-Object { $_ }
    $deleted = @(& git diff --no-renames --name-only --diff-filter=D $BaseCommit $targetCommit --) | Where-Object { $_ }
    if ($LASTEXITCODE -ne 0) { throw "Deltaliste konnte nicht erzeugt werden." }
    $manifestEntries = [Collections.Generic.List[object]]::new()

    foreach ($path in ($changed | Sort-Object -Unique)) {
        Assert-RelativeRepositoryPath $path
        $target = Join-Path $PSScriptRoot ($path -replace '/', [IO.Path]::DirectorySeparatorChar)
        if (!(Test-Path -LiteralPath $target -PathType Leaf)) { throw "Zieldatei aus Git-Diff fehlt: $path" }
        $base = Join-Path $baseRoot ($path -replace '/', [IO.Path]::DirectorySeparatorChar)
        $payload = Join-Path $payloadRoot ($path -replace '/', [IO.Path]::DirectorySeparatorChar)
        New-Item -ItemType Directory -Path (Split-Path -Parent $payload) -Force | Out-Null
        Copy-Item -LiteralPath $target -Destination $payload
        $status = if (Test-Path -LiteralPath $base -PathType Leaf) { "modified" } else { "added" }
        $baseHash = if ($status -eq "modified") { (Get-FileHash -LiteralPath $base -Algorithm SHA256).Hash } else { $null }
        $manifestEntries.Add([ordered]@{
            path = $path
            status = $status
            baseSha256 = $baseHash
            targetSha256 = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash
            sizeBytes = (Get-Item -LiteralPath $target).Length
        })
    }
    foreach ($path in ($deleted | Sort-Object -Unique)) {
        Assert-RelativeRepositoryPath $path
        $base = Join-Path $baseRoot ($path -replace '/', [IO.Path]::DirectorySeparatorChar)
        if (!(Test-Path -LiteralPath $base -PathType Leaf)) { throw "Gelöschte Basisdatei fehlt im Basiscommit: $path" }
        $manifestEntries.Add([ordered]@{
            path = $path
            status = "deleted"
            baseSha256 = (Get-FileHash -LiteralPath $base -Algorithm SHA256).Hash
            targetSha256 = $null
            sizeBytes = 0
        })
    }
    $orderedEntries = @($manifestEntries | Sort-Object { $_.path })
    if ($orderedEntries.Count -eq 0) { throw "Zwischen Basis und Ziel wurden keine Änderungen gefunden." }
    $manifest = [ordered]@{
        format = "site.s9lab.delta"
        formatVersion = 1
        productVersion = "1.0.8"
        baseCommit = $BaseCommit.ToLowerInvariant()
        targetCommit = $targetCommit
        entries = $orderedEntries
    }
    [IO.File]::WriteAllText((Join-Path $deltaRoot "delta-manifest.json"), ($manifest | ConvertTo-Json -Depth 6), [Text.UTF8Encoding]::new($false))
    Compress-Archive -Path (Join-Path $deltaRoot "*") -DestinationPath $deltaZip -CompressionLevel Optimal

    $sourceHash = (Get-FileHash -LiteralPath $sourceZip -Algorithm SHA256).Hash
    $deltaHash = (Get-FileHash -LiteralPath $deltaZip -Algorithm SHA256).Hash
    [IO.File]::WriteAllText("$sourceZip.sha256", "$sourceHash *$sourceName`n", [Text.UTF8Encoding]::new($false))
    [IO.File]::WriteAllText("$deltaZip.sha256", "$deltaHash *$deltaName`n", [Text.UTF8Encoding]::new($false))
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot "APPLY-S9LAB-DELTA.ps1") -Destination (Join-Path $outputFull "APPLY-S9LAB-DELTA.ps1")

    $installerSection = "Nicht an diesen Paketierungslauf übergeben."
    if (![string]::IsNullOrWhiteSpace($InstallerPath)) {
        $resolvedInstaller = (Resolve-Path -LiteralPath $InstallerPath).Path
        $installerSignature = Get-AuthenticodeSignature -LiteralPath $resolvedInstaller
        $installerSection = "Pfad: $resolvedInstaller`nSHA-256: $((Get-FileHash -LiteralPath $resolvedInstaller -Algorithm SHA256).Hash)`nAuthenticode: $($installerSignature.Status)"
    }
    $changeLines = ($orderedEntries | ForEach-Object { "- $($_.status): ``$($_.path)``" }) -join "`n"
    $report = @"
# S9Lab Launcher 1.0.8 – Lieferartefakte

Zielcommit: ``$targetCommit``
Delta-Basis: ``$($BaseCommit.ToLowerInvariant())``

## Vollständiges Quellpaket

- Datei: ``$sourceName``
- Größe: $((Get-Item -LiteralPath $sourceZip).Length) Bytes
- SHA-256: ``$sourceHash``

## Delta-Paket

- Datei: ``$deltaName``
- Größe: $((Get-Item -LiteralPath $deltaZip).Length) Bytes
- SHA-256: ``$deltaHash``
- Anwenden: ``APPLY-S9LAB-DELTA.ps1`` validiert Basis, Pfade und jeden Payloadhash und rollt Teilfehler zurück.

## Diagnoseinstaller

$installerSection

## Änderungen gegenüber der Basis

$changeLines

Es wurde nichts signiert, gepusht oder veröffentlicht. Das Authenticode-Produktionsgate bleibt separat.
"@
    [IO.File]::WriteAllText((Join-Path $outputFull "S9Lab-Launcher-v1.0.8-artifact-report.md"), $report, [Text.UTF8Encoding]::new($false))

    Write-Host "Lieferartefakte erzeugt: $outputFull" -ForegroundColor Green
    Write-Host "Source SHA-256: $sourceHash"
    Write-Host "Delta SHA-256: $deltaHash"
}
finally {
    if (Test-Path -LiteralPath $workRoot) {
        $resolvedWork = (Resolve-Path -LiteralPath $workRoot).Path
        $tempPrefix = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
        if (-not $resolvedWork.StartsWith($tempPrefix, [StringComparison]::OrdinalIgnoreCase)) { throw "Unsicheres Paketierungs-Aufräumziel." }
        Remove-Item -LiteralPath $resolvedWork -Recurse -Force
    }
}
