[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$PatchZip,
    [Parameter(Mandatory = $true)][string]$TargetRoot,
    [switch]$KeepBackup
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.IO.Compression.FileSystem

function Resolve-SafeTarget {
    param([string]$Root, [string]$Relative)
    if ([string]::IsNullOrWhiteSpace($Relative) -or $Relative -match '(^|/)(\.|\.\.)(/|$)' -or $Relative -match '[:\\\x00-\x1f]' -or [IO.Path]::IsPathRooted($Relative)) {
        throw "Unsicherer relativer Patchpfad: $Relative"
    }
    $candidate = [IO.Path]::GetFullPath((Join-Path $Root ($Relative -replace '/', [IO.Path]::DirectorySeparatorChar)))
    $prefix = $Root.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if (-not $candidate.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Patchpfad verlässt das Ziel: $Relative"
    }
    return $candidate
}

function Assert-NoReparseAncestor {
    param([string]$Root, [string]$Path)
    $rootItem = Get-Item -LiteralPath $Root -Force
    if (($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "Das Zielstammverzeichnis ist ein Reparse Point."
    }
    $relative = [IO.Path]::GetRelativePath($Root, $Path)
    $current = $Root
    foreach ($part in ($relative -split '[\\/]')) {
        $current = Join-Path $current $part
        if (Test-Path -LiteralPath $current) {
            $item = Get-Item -LiteralPath $current -Force
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Reparse Point im Patchziel wurde abgewiesen: $current"
            }
        }
    }
}

$resolvedPatch = (Resolve-Path -LiteralPath $PatchZip).Path
$resolvedRoot = (Resolve-Path -LiteralPath $TargetRoot).Path
if (!(Test-Path -LiteralPath $resolvedRoot -PathType Container)) { throw "Zielstamm fehlt." }

$workRoot = Join-Path ([IO.Path]::GetTempPath()) ("s9lab-delta-" + [guid]::NewGuid().ToString("N"))
$extractRoot = Join-Path $workRoot "patch"
$backupRoot = Join-Path $workRoot "backup"
New-Item -ItemType Directory -Path $extractRoot, $backupRoot | Out-Null
$mutated = $false
$entries = @()

try {
    $archive = [IO.Compression.ZipFile]::OpenRead($resolvedPatch)
    try {
        $manifestEntry = $archive.Entries | Where-Object { $_.FullName -ceq "delta-manifest.json" }
        if (@($manifestEntry).Count -ne 1) { throw "Delta enthält nicht genau ein Manifest." }
        $reader = [IO.StreamReader]::new($manifestEntry.Open(), [Text.UTF8Encoding]::new($false, $true))
        try { $manifestText = $reader.ReadToEnd() } finally { $reader.Dispose() }
        $manifest = $manifestText | ConvertFrom-Json
        if ($manifest.format -cne "site.s9lab.delta" -or $manifest.formatVersion -ne 1) { throw "Unbekanntes Deltaformat." }
        if ($manifest.baseCommit -notmatch '^[a-f0-9]{40}$' -or $manifest.targetCommit -notmatch '^[a-f0-9]{40}$') { throw "Ungültige Commitbindung im Delta." }
        $entries = @($manifest.entries)
        if ($entries.Count -eq 0) { throw "Leeres Delta wird abgewiesen." }
        $expectedPayload = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
        $seenPaths = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        foreach ($entry in $entries) {
            $null = Resolve-SafeTarget -Root $resolvedRoot -Relative $entry.path
            if (!$seenPaths.Add([string]$entry.path)) { throw "Doppelter oder Case-kollidierender Deltapfad: $($entry.path)" }
            if ($entry.status -notin @("added", "modified", "deleted")) { throw "Unbekannter Deltastatus: $($entry.status)" }
            if ($entry.status -ne "added" -and $entry.baseSha256 -notmatch '^[A-Fa-f0-9]{64}$') { throw "Fehlender Basishash: $($entry.path)" }
            if ($entry.status -ne "deleted") {
                if ($entry.targetSha256 -notmatch '^[A-Fa-f0-9]{64}$' -or [int64]$entry.sizeBytes -lt 0) { throw "Ungültige Zielbindung: $($entry.path)" }
                [void]$expectedPayload.Add("files/$($entry.path)")
            }
        }
        foreach ($zipEntry in $archive.Entries) {
            $name = $zipEntry.FullName -replace '\\', '/'
            if ([string]::IsNullOrWhiteSpace($name) -or $name -match '(^|/)(\.|\.\.)(/|$)' -or $name -match '[:\\\x00-\x1f]' -or $name.StartsWith('/')) {
                throw "Unsicherer ZIP-Eintrag im Delta: $name"
            }
            if ($name.EndsWith('/')) { continue }
            if ($name -ceq "delta-manifest.json") { continue }
            if (!$expectedPayload.Remove($name)) { throw "Unerwartete oder doppelte Datei im Delta: $name" }
        }
        if ($expectedPayload.Count -ne 0) { throw "Delta-Payload ist unvollständig: $($expectedPayload -join ', ')" }
    }
    finally { $archive.Dispose() }

    [IO.Compression.ZipFile]::ExtractToDirectory($resolvedPatch, $extractRoot)

    if (Test-Path -LiteralPath (Join-Path $resolvedRoot ".git")) {
        $gitStatus = (& git -C $resolvedRoot status --short) -join "`n"
        if ($LASTEXITCODE -ne 0 -or $gitStatus) { throw "Git-Ziel muss vor dem Patch sauber sein." }
        $head = (& git -C $resolvedRoot rev-parse HEAD).Trim()
        if ($LASTEXITCODE -ne 0 -or $head -cne $manifest.baseCommit) { throw "Git-Ziel entspricht nicht dem gebundenen Basiscommit." }
    }

    foreach ($entry in $entries) {
        $target = Resolve-SafeTarget -Root $resolvedRoot -Relative $entry.path
        Assert-NoReparseAncestor -Root $resolvedRoot -Path $target
        if ($entry.status -eq "added") {
            if (Test-Path -LiteralPath $target) { throw "Hinzugefügtes Patchziel existiert bereits: $($entry.path)" }
        }
        else {
            if (!(Test-Path -LiteralPath $target -PathType Leaf)) { throw "Basisdatei fehlt: $($entry.path)" }
            $baseHash = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash
            if ($baseHash -ne ([string]$entry.baseSha256).ToUpperInvariant()) { throw "Basisdatei wurde verändert: $($entry.path)" }
        }
        if ($entry.status -ne "deleted") {
            $payload = Join-Path $extractRoot ("files/" + $entry.path)
            if (!(Test-Path -LiteralPath $payload -PathType Leaf)) { throw "Payload fehlt: $($entry.path)" }
            if ((Get-Item -LiteralPath $payload).Length -ne [int64]$entry.sizeBytes) { throw "Payloadgröße stimmt nicht: $($entry.path)" }
            if ((Get-FileHash -LiteralPath $payload -Algorithm SHA256).Hash -ne ([string]$entry.targetSha256).ToUpperInvariant()) { throw "Payloadhash stimmt nicht: $($entry.path)" }
        }
    }

    foreach ($entry in $entries) {
        $target = Resolve-SafeTarget -Root $resolvedRoot -Relative $entry.path
        $backup = Resolve-SafeTarget -Root $backupRoot -Relative $entry.path
        if ($entry.status -ne "added") {
            New-Item -ItemType Directory -Path (Split-Path -Parent $backup) -Force | Out-Null
            Copy-Item -LiteralPath $target -Destination $backup
        }
        $mutated = $true
        if ($entry.status -eq "deleted") {
            Remove-Item -LiteralPath $target -Force
        }
        else {
            $payload = Join-Path $extractRoot ("files/" + $entry.path)
            New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force | Out-Null
            Copy-Item -LiteralPath $payload -Destination $target -Force
        }
    }

    foreach ($entry in $entries) {
        $target = Resolve-SafeTarget -Root $resolvedRoot -Relative $entry.path
        if ($entry.status -eq "deleted") {
            if (Test-Path -LiteralPath $target) { throw "Gelöschtes Ziel ist nach Patch noch vorhanden: $($entry.path)" }
        }
        elseif ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash -ne ([string]$entry.targetSha256).ToUpperInvariant()) {
            throw "Zieldatei stimmt nach Patch nicht: $($entry.path)"
        }
    }
    $mutated = $false
    Write-Host "Delta erfolgreich angewendet und SHA-256-verifiziert: $($manifest.baseCommit) -> $($manifest.targetCommit)" -ForegroundColor Green
}
catch {
    if ($mutated) {
        foreach ($entry in ($entries | Sort-Object path -Descending)) {
            $target = Resolve-SafeTarget -Root $resolvedRoot -Relative $entry.path
            $backup = Resolve-SafeTarget -Root $backupRoot -Relative $entry.path
            if ($entry.status -eq "added") {
                if (Test-Path -LiteralPath $target -PathType Leaf) { Remove-Item -LiteralPath $target -Force }
            }
            elseif (Test-Path -LiteralPath $backup -PathType Leaf) {
                New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force | Out-Null
                Copy-Item -LiteralPath $backup -Destination $target -Force
            }
        }
    }
    throw
}
finally {
    if (Test-Path -LiteralPath $workRoot) {
        if ($KeepBackup) {
            Write-Host "Patch-Arbeitsverzeichnis bleibt erhalten: $workRoot"
        }
        else {
            $resolvedWork = (Resolve-Path -LiteralPath $workRoot).Path
            $tempPrefix = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
            if (-not $resolvedWork.StartsWith($tempPrefix, [StringComparison]::OrdinalIgnoreCase)) { throw "Unsicheres Patch-Aufräumziel." }
            Remove-Item -LiteralPath $resolvedWork -Recurse -Force
        }
    }
}
