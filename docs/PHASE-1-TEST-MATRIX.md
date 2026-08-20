# Phase 1 v1.0.3 – Testmatrix

## Statuslegende

- **Selbst ausgeführt:** in der aktuellen Arbeitsumgebung tatsächlich ausgeführt.
- **Nutzerseitiges Windows-Ergebnis:** vom Projektinhaber bereitgestellt, nicht als eigene Ausführung ausgegeben.
- **Windows-Nachprüfung erforderlich:** kann nur auf einem echten Windows-System abschließend bestätigt werden.

## Nutzerseitiger Windows-Ausgangsbefund für v1.0.2

Erfolgreich:

- `npm ci`
- `npm test`
- TypeScript
- Vite-Build mit 1.604 Modulen
- `cargo fmt --all -- --check`
- `cargo check --locked`
- Clippy mit `-D warnings`
- Operations-Preflight
- Hardlink-Test
- Junction-Fixture erfolgreich erstellt
- Junction als Reparse Point und Junction verifiziert
- S9Lab lehnte die Junction ab

Fehlgeschlagen:

- `rejects_windows_directory_junctions_after_verified_fixture_creation` erwartete `path_reparse_point_forbidden`, erhielt aber `path_symlink_forbidden`.

Diese Ergebnisse gelten ausschließlich für v1.0.2 und nicht automatisch für v1.0.3.

## Selbst ausgeführte Prüfungen für v1.0.3

| Prüfung | Ergebnis | Umfang |
|---|---|---|
| `npm ci` | erfolgreich | 84 Pakete |
| `npm test` | erfolgreich | UTF-8, Secrets, Sicherheit, Version, IPC, Rust-Statik, Phase-1-Foundation |
| `npm run build` | erfolgreich | TypeScript und Vite, 1.604 Module |
| IPC-Verträge | erfolgreich | 20 Commands geprüft, 1 gemeinsam typisiert |
| statische Rust-Prüfung | erfolgreich | 36 Rust-Dateien |
| Klassifizierungsreihenfolge | erfolgreich, statisch | Reparse-Point-Prüfung steht vor Symlink-Prüfung |
| `cargo fmt --all -- --check` | nicht ausgeführt | keine Rust-Toolchain in der Umgebung |
| `cargo check --locked` | nicht ausgeführt | keine Rust-Toolchain in der Umgebung |
| Clippy `-D warnings` | nicht ausgeführt | keine Rust-Toolchain in der Umgebung |
| Rust-Tests | nicht ausgeführt | keine Rust-Toolchain in der Umgebung |
| Windows-Junction-Tests | nicht ausführbar | echtes Windows erforderlich |
| Tauri-/NSIS-Windows-Build | nicht ausgeführt | echtes Windows erforderlich |

## Regressionsabdeckung

| Bereich | Test |
|---|---|
| stabile direkte Junction-Klassifizierung | `classifies_verified_windows_junctions_with_the_stable_reparse_error` |
| echte Windows-Junction über Registry | `rejects_windows_directory_junctions_after_verified_fixture_creation` |
| echte Hardlink-Fixture | `rejects_existing_hardlinks` |
| Operations-Preflight vor Journal | `operation_plan_preflight_enforces_the_real_root_budget_before_journaling` |
| reales ID- und Zielpfadmodell | `generated_profile_and_staging_paths_fit_the_documented_budget` |
| relative Grenze erlaubt | `accepts_path_at_the_available_relative_boundary` |
| relative Grenze +1 abgelehnt | `rejects_path_one_unit_beyond_the_available_relative_boundary` |
| Wurzellänge im absoluten Budget | `absolute_path_budget_accounts_for_the_registered_root_length` |
| Recovery an allen Failure-Points | `crash_recovery_never_leaves_a_mixed_revision` |
| Traversal und Windows-Sonderfälle | `rejects_traversal_and_windows_special_cases` |
| Case-/Unicode-Kollisionen | `detects_case_collisions` und Windows-Normalisierungstest |
| technische Demo | `phase1_transaction_demo` |

## Verbindliche Windows-Reihenfolge

1. `npm ci`
2. `npm test`
3. `npm run build`
4. `cargo fmt --all -- --check`
5. `cargo check --locked`
6. `cargo clippy --locked --all-targets -- -D warnings`
7. direkten Junction-Klassifikationstest einzeln
8. Registry-Junction-Test einzeln
9. Hardlink-Test einzeln
10. Recovery-Test einzeln
11. kompletter Rust-Testlauf mit `--nocapture`
12. erst danach Tauri-/NSIS-Build

Ein erfolgreicher Bundle-Build ist weiterhin unsigniert und nicht veröffentlichungsbereit.
