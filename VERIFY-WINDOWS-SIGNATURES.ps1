param(
    [Parameter(Mandatory = $true)]
    [string[]]$Path
)

$ErrorActionPreference = "Stop"
$failed = $false

foreach ($item in $Path) {
    if (-not (Test-Path $item -PathType Leaf)) {
        Write-Error "Datei nicht gefunden: $item"
        $failed = $true
        continue
    }

    $signature = Get-AuthenticodeSignature -FilePath $item
    if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
        Write-Error "Ungültige oder fehlende Authenticode-Signatur: $item ($($signature.Status))"
        $failed = $true
        continue
    }

    if (-not $signature.SignerCertificate) {
        Write-Error "Kein Signaturzertifikat gefunden: $item"
        $failed = $true
        continue
    }

    Write-Host "Signatur gültig: $item" -ForegroundColor Green
    Write-Host "  Herausgeber: $($signature.SignerCertificate.Subject)"
    Write-Host "  Fingerabdruck: $($signature.SignerCertificate.Thumbprint)"
}

if ($failed) {
    throw "Mindestens eine Windows-Datei ist nicht gültig signiert."
}
