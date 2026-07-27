param(
    [switch]$Bundle
)

$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

if (-not (Get-Command npm -ErrorAction SilentlyContinue)) {
    throw "Node.js/npm wurde nicht gefunden."
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "Rust/Cargo wurde nicht gefunden."
}

npm ci
if ($LASTEXITCODE -ne 0) { throw "npm ci fehlgeschlagen." }

npm test
if ($LASTEXITCODE -ne 0) { throw "Phase-1-Prüfungen fehlgeschlagen." }

npm run build
if ($LASTEXITCODE -ne 0) { throw "Frontend-Build fehlgeschlagen." }

Push-Location src-tauri
try {
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw "Rust-Formatprüfung fehlgeschlagen." }
    cargo check --locked
    if ($LASTEXITCODE -ne 0) { throw "Rust-Check fehlgeschlagen." }
    cargo clippy --locked --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "Rust-Clippy fehlgeschlagen." }
    cargo test --locked
    if ($LASTEXITCODE -ne 0) { throw "Rust-Tests fehlgeschlagen." }
    cargo test --locked phase1_transaction_demo -- --nocapture
    if ($LASTEXITCODE -ne 0) { throw "Technische Phase-1-Demo fehlgeschlagen." }
} finally {
    Pop-Location
}

if ($Bundle) {
    Write-Warning "Der lokale Bundle-Build ist nicht für die Verteilung freigegeben. Authenticode-Signierung ist extern noch nicht eingerichtet."
    npm run tauri:build
    if ($LASTEXITCODE -ne 0) { throw "Tauri-Build fehlgeschlagen." }
}

Write-Host "Lokale Verifikation erfolgreich." -ForegroundColor Green
