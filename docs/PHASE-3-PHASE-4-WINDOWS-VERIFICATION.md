# Phase 3 und Phase 4 – Windows-Verifikation

## Voraussetzungen

- Windows 11 mit echter MSVC-Rust-Toolchain,
- Node.js Hauptversion 24,
- effektive npm-Registry exakt `https://registry.npmjs.org/`,
- lokaler Edge-, Chrome- oder Chromium-Browser,
- das kumulative Quell-ZIP, seine separate SHA-Datei und ein unabhängig übermittelter SHA-256-Wert.

## Aufruf

```powershell
.\VERIFY-PHASE3-PHASE4-WINDOWS.ps1 -ExpectedSha256 "<unabhängig übermittelter SHA-256>"
```

Optional können `-ZipPath`, `-ChecksumPath` und `-BrowserPath` übergeben werden. ZIP und SHA-Datei müssen beide dem verpflichtenden `ExpectedSha256` entsprechen, bevor entpackt wird.

## Gate

Das Skript erzeugt einen neuen Cleanroom, einen separaten kurzen Runtime-TEMP und stellt alle veränderten Umgebungsvariablen in einem äußeren `finally` exakt wieder her. Es führt aus:

1. Registry-, Source-, Workflow-, Phase-3- und Phase-4-Gates vor Installation,
2. öffentliches `npm ci`, vollständiges `npm test`, Build, Browser, Performance und erhaltenes Phase-2-Gate,
3. `cargo fmt --all -- --check`, `cargo check --locked`, Clippy mit `-D warnings`,
4. dreimal `cargo test --locked -- --nocapture` ohne Testserialisierung,
5. einen lokalen unsignierten Tauri-/NSIS-Diagnosebuild.

Jeder native Exitcode wird unmittelbar erfasst. Prüfverzeichnis, Visuals und Performancebericht bleiben zur Diagnose erhalten; es wird nichts signiert oder veröffentlicht. Erst ein vollständig grüner echter Windows-Lauf kann als Grundlage für eine Freigabe dienen.
