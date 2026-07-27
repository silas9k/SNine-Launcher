# Phase 1 v1.0.1 – Geänderte Dateien

Vergleichsbasis: `S9Lab-Launcher-Phase1-v1.0-final-source.zip`.

## Funktionale Korrekturen

| Datei | Änderung |
|---|---|
| `src-tauri/src/security/paths.rs` | dynamisches Pfadbudget; konservative absolute Grenze 247; strukturierte Budgetdaten; getrennte Grenztests; zwingend verifizierte Hardlink- und Junction-Fixtures |
| `src-tauri/src/security/mod.rs` | Export des dokumentierten absoluten Pfadbudgets |
| `src-tauri/src/operations/engine.rs` | vollständiger Pfad-Preflight vor SQLite-Operation und Journal; gemeinsame Zielpfadermittlung für Staging und Profil |
| `src-tauri/src/operations/mod.rs` | reales ID-/Pfadmodell, Operations-Grenztest, Recovery-Regression unter normalem TEMP |
| `scripts/check-phase1-foundation.mjs` | statische Sicherung der neuen Pfad-, Fixture-, Preflight- und CI-Regeln |
| `.github/workflows/phase1-windows-verification.yml` | Rust 1.88.0 festgelegt; explizite Windows-Regressionsläufe für Recovery, Pfadgrenzen, Hardlink und Junction |

## Dokumentation

| Datei | Änderung |
|---|---|
| `docs/PHASE-1-v1.0.1-CORRECTIONS.md` | neue vollständige Korrekturliste |
| `docs/PHASE-1-PATH-SECURITY.md` | reales Windows-Pfadbudget, Formeln und Fixture-Regeln |
| `docs/PHASE-1-OPERATIONS-RECOVERY.md` | Pfad-Preflight und korrigierte Recovery-Regeln |
| `docs/PHASE-1-TEST-MATRIX.md` | echte Ergebnisse, Grenzen der lokalen Umgebung und Windows-Befehle |
| `docs/PHASE-1-COMPLETION-REPORT.md` | aktualisierter Abschlussbericht v1.0.1 |
| `docs/PHASE-1-CHANGED-FILES.md` | diese Liste |

## Ausschließlich Rustfmt

Die folgenden Dateien unterscheiden sich gegenüber v1.0 ausschließlich durch die vollständige Rustfmt-Formatierung; ihr Verhalten wurde nicht geändert:

- `src-tauri/src/cache/mod.rs`
- `src-tauri/src/download/mod.rs`
- `src-tauri/src/error.rs`
- `src-tauri/src/ipc/mod.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/minecraft/java_progress.rs`
- `src-tauri/src/operations/recovery.rs`
- `src-tauri/src/storage/mod.rs`
- `src-tauri/src/storage/sqlite.rs`

## Nicht geändert

- `package.json`-Produktversion bleibt `1.0.8`.
- `Cargo.toml` und `Cargo.lock` wurden in dieser Korrekturrunde nicht funktional verändert.
- Keine Phase-2-Komponente wurde begonnen.
- Keine Schlüssel, Secrets, Updater oder produktiven Endpunkte wurden ergänzt.

## Korrekturrunde v1.0.3

- `.github/workflows/phase1-windows-verification.yml`
- `VERIFY-PHASE1-WINDOWS.ps1`
- `scripts/check-phase1-foundation.mjs`
- `src-tauri/src/security/paths.rs`
- `docs/PHASE-1-COMPLETION-REPORT.md`
- `docs/PHASE-1-DEMO.md`
- `docs/PHASE-1-IPC-CONTRACTS.md`
- `docs/PHASE-1-PATH-SECURITY.md`
- `docs/PHASE-1-TEST-MATRIX.md`
- `docs/PHASE-1-v1.0.3-CORRECTIONS.md`

