param(
  [Parameter(Mandatory=$true)]
  [string]$JarPath
)

$ErrorActionPreference = "Stop"
$project = Split-Path -Parent $MyInvocation.MyCommand.Path
$target = Join-Path $project "src-tauri\resources\default-profile-mods\s9lab-client-bundled.jar"

if (!(Test-Path $JarPath)) { throw "JAR nicht gefunden: $JarPath" }
if ([IO.Path]::GetExtension($JarPath) -ne ".jar") { throw "Bitte eine .jar-Datei auswählen." }

Copy-Item $JarPath $target -Force
Write-Host "SNine Client aktualisiert:" -ForegroundColor Green
Write-Host $target
Write-Host "Jetzt mit 'npm run tauri:build' neu bauen." -ForegroundColor Cyan
