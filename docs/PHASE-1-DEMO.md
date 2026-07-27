# Phase 1 v1.0.1 – Technische Demo

## Voraussetzungen

- Node.js und npm gemäß Projektworkflow
- Rust 1.88.0 mit `rustfmt` und `clippy`
- Windows-Buildvoraussetzungen für Tauri nur für den optionalen Bundle-Test

## Vollständige Quellprüfung

```powershell
npm ci
npm test
npm run build

Push-Location src-tauri
cargo fmt --all -- --check
cargo check --locked
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
Pop-Location
```

## Vorführbare Transaktionsdemo

```powershell
Push-Location src-tauri
cargo test --locked phase1_transaction_demo -- --nocapture
Pop-Location
```

Die Demo:

1. erzeugt einen injizierten temporären S9Lab-Stamm,
2. legt einen minimalen Profileintrag an,
3. plant eine simulierte Installation,
4. schreibt Manifest, Lockdatei und Payload ins Staging,
5. prüft die Hashes,
6. aktiviert die Revision atomar,
7. validiert SQLite und Dateien,
8. entfernt das Staging.

## Recovery-Demo

```powershell
Push-Location src-tauri
cargo test --locked crash_recovery_never_leaves_a_mixed_revision -- --nocapture
Pop-Location
```

Der Test unterbricht den Ablauf an allen definierten Failure-Points, verwendet den normalen Windows-TEMP-Pfad, öffnet denselben temporären Stamm erneut und bestätigt anschließend entweder den vollständigen alten oder vollständigen neuen Zustand.

## Pfad- und Linkregressionen unter Windows

```powershell
Push-Location src-tauri
cargo test --locked operation_plan_preflight_enforces_the_real_root_budget_before_journaling -- --nocapture
cargo test --locked rejects_existing_hardlinks -- --nocapture
cargo test --locked classifies_verified_windows_junctions_with_the_stable_reparse_error -- --nocapture
cargo test --locked rejects_windows_directory_junctions_after_verified_fixture_creation -- --nocapture
Pop-Location
```

Der Junction-Test gilt nur dann als bestanden, wenn Windows die Junction tatsächlich angelegt und der Test ihren Reparse-Point-Status bestätigt hat.

## Gesamter Windows-Harness

```powershell
.\BUILD-WINDOWS.ps1
```

Optionaler lokaler, **unsignierter** Tauri-Bundle-Test:

```powershell
.\BUILD-WINDOWS.ps1 -Bundle
```

Ein dabei erzeugtes Bundle ist weder signiert noch veröffentlichungsbereit.
