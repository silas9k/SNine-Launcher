[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$InstallerPath,
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[A-Fa-f0-9]{64}$')]
    [string]$ExpectedSha256,
    [Parameter(Mandatory = $true)]
    [switch]$AllowUnsignedDiagnosticInstaller,
    [switch]$KeepSandbox
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Invoke-HiddenProcess {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $true)][string[]]$ArgumentList,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $process = Start-Process -FilePath $FilePath -ArgumentList $ArgumentList -Wait -PassThru -WindowStyle Hidden
    if ($process.ExitCode -ne 0) {
        throw "$Label ist mit Exitcode $($process.ExitCode) fehlgeschlagen."
    }
}

$resolvedInstaller = (Resolve-Path -LiteralPath $InstallerPath).Path
if ((Get-FileHash -LiteralPath $resolvedInstaller -Algorithm SHA256).Hash -ne $ExpectedSha256.ToUpperInvariant()) {
    throw "Der Installer stimmt nicht mit ExpectedSha256 überein."
}
if (-not $AllowUnsignedDiagnosticInstaller) {
    throw "Der isolierte Lifecycle-Test benötigt die ausdrückliche Freigabe -AllowUnsignedDiagnosticInstaller."
}
$signature = Get-AuthenticodeSignature -LiteralPath $resolvedInstaller
if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::NotSigned) {
    throw "Erwartet wurde ausschließlich der unsignierte Diagnoseinstaller; Status: $($signature.Status)."
}

$uninstallKey = "Software\Microsoft\Windows\CurrentVersion\Uninstall\SNine Launcher"
$guardedUninstallKeys = @(
    $uninstallKey,
    "Software\Microsoft\Windows\CurrentVersion\Uninstall\S9Lab Launcher"
)
$preexisting = @($guardedUninstallKeys | ForEach-Object {
    "Registry::HKEY_CURRENT_USER\$_"
    "Registry::HKEY_LOCAL_MACHINE\$_"
} | Where-Object { Test-Path -LiteralPath $_ })
if ($preexisting.Count -ne 0) {
    throw "Der Lifecycle-Test stoppt, weil bereits eine SNine- oder frühere S9Lab-Installation registriert ist: $($preexisting -join ', ')"
}

$sandbox = Join-Path ([IO.Path]::GetTempPath()) ("s9lab-nsis-" + [guid]::NewGuid().ToString("N"))
$installRoot = Join-Path $sandbox "app"
$sandboxFull = [IO.Path]::GetFullPath($sandbox)
$tempFull = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
if (-not $sandboxFull.StartsWith($tempFull, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Der Lifecycle-Sandboxpfad liegt nicht unter dem System-TEMP."
}
New-Item -ItemType Directory -Path $sandbox | Out-Null
$installedBinary = Join-Path $installRoot "s9lab-launcher.exe"
$uninstaller = Join-Path $installRoot "uninstall.exe"
$currentUserKey = "Registry::HKEY_CURRENT_USER\$uninstallKey"

try {
    Invoke-HiddenProcess -FilePath $resolvedInstaller -ArgumentList @("/S", "/CurrentUser", "/NS", "/D=$installRoot") -Label "Isolierte NSIS-Installation"
    if (!(Test-Path -LiteralPath $installedBinary -PathType Leaf) -or !(Test-Path -LiteralPath $uninstaller -PathType Leaf)) {
        throw "Installation hat Hauptprogramm oder Uninstaller nicht vollständig erzeugt."
    }
    if (!(Test-Path -LiteralPath $currentUserKey)) {
        throw "Die isolierte CurrentUser-Installation fehlt im Uninstall-Register."
    }
    $installedVersion = (Get-Item -LiteralPath $installedBinary).VersionInfo.ProductVersion
    if ($installedVersion -notmatch '^1\.0\.8(?:\.0)?$') {
        throw "Installierte Produktversion ist unerwartet: $installedVersion"
    }
    $firstBinaryHash = (Get-FileHash -LiteralPath $installedBinary -Algorithm SHA256).Hash

    Invoke-HiddenProcess -FilePath $resolvedInstaller -ArgumentList @("/S", "/CurrentUser", "/NS", "/UPDATE", "/D=$installRoot") -Label "NSIS-In-place-Wartung"
    if (!(Test-Path -LiteralPath $installedBinary -PathType Leaf) -or !(Test-Path -LiteralPath $uninstaller -PathType Leaf)) {
        throw "Der In-place-Wartungspfad hat die Installation beschädigt."
    }
    $secondBinaryHash = (Get-FileHash -LiteralPath $installedBinary -Algorithm SHA256).Hash
    if ($secondBinaryHash -ne $firstBinaryHash) {
        throw "Der identische 1.0.10-Wartungslauf erzeugte ein abweichendes Hauptprogramm."
    }

    Invoke-HiddenProcess -FilePath $uninstaller -ArgumentList @("/S", "/CurrentUser", "_?=$installRoot") -Label "Isolierte NSIS-Deinstallation"
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    while (((Test-Path -LiteralPath $installedBinary) -or (Test-Path -LiteralPath $uninstaller)) -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 250
    }
    if ((Test-Path -LiteralPath $installedBinary) -or (Test-Path -LiteralPath $uninstaller)) {
        throw "Die Deinstallation hat Programmdateien nicht rechtzeitig entfernt."
    }
    if (Test-Path -LiteralPath $currentUserKey) {
        throw "Die Deinstallation hat den CurrentUser-Uninstall-Eintrag nicht entfernt."
    }
    Write-Host "NSIS-Lifecycle bestanden: isolierte Installation, identischer 1.0.10-In-place-Wartungspfad und Deinstallation." -ForegroundColor Green
}
finally {
    if (Test-Path -LiteralPath $uninstaller -PathType Leaf) {
        try {
            Invoke-HiddenProcess -FilePath $uninstaller -ArgumentList @("/S", "/CurrentUser", "_?=$installRoot") -Label "Lifecycle-Aufräumdeinstallation"
        }
        catch {
            Write-Warning $_.Exception.Message
        }
    }
    if (!$KeepSandbox -and (Test-Path -LiteralPath $sandbox)) {
        $resolvedSandbox = (Resolve-Path -LiteralPath $sandbox).Path
        if (-not $resolvedSandbox.StartsWith($tempFull, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Aufräumen außerhalb des System-TEMP wurde verhindert."
        }
        Remove-Item -LiteralPath $resolvedSandbox -Recurse -Force
    }
}
