$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
    throw "Node.js/npm wurde nicht gefunden. Installiere zuerst Node.js."
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "Rust/Cargo wurde nicht gefunden. Installiere zuerst Rust über rustup."
}
if (-not (Test-Path "node_modules")) {
    Write-Host "Installiere Node-Abhängigkeiten..." -ForegroundColor Cyan
    npm install
}
Write-Host "Starte SNine Launcher im Dev-Modus..." -ForegroundColor Magenta
npm run tauri:dev
