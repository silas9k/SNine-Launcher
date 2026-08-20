[CmdletBinding()]
param(
    [string]$ZipPath = (Join-Path $PSScriptRoot "S9Lab-Launcher-Phase2-v1.0.3-final-source.zip"),
    [string]$ChecksumPath = "",
    [string]$BrowserPath = "",
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[A-Fa-f0-9]{64}$')]
    [string]$ExpectedSha256
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$verifier = Join-Path $PSScriptRoot "VERIFY-PHASE2-V1.0.3-WINDOWS.ps1"
if (!(Test-Path -LiteralPath $verifier)) {
    throw "Das verbindliche Phase-2-v1.0.3-Verifikationsskript fehlt: $verifier"
}
& $verifier -ZipPath $ZipPath -ChecksumPath $ChecksumPath -BrowserPath $BrowserPath -ExpectedSha256 $ExpectedSha256
