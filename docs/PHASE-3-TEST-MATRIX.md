# Phase 3: Testmatrix

| Bereich | Automatischer Nachweis | Gate |
|---|---|---|
| Öffentliche Registry | Registry-Guard und 33 positive/negative Registry-Tests im bestehenden Node-Satz | verpflichtend vor `npm ci` |
| Workflow/Quellsauberkeit | semantischer YAML-Guard und isolierte Source-Fixtures | verpflichtend |
| Öffentliche IPC-Daten | Generatorcheck, Wrappercheck und Phase-3-Secret-Scanner | verpflichtend |
| Device Prompt | Rust-Serialisierungstest und React-Komponententest | verpflichtend |
| Besitzprüfung | Rust-Test für bekannte, leere und Lookalike-Entitlements | Windows/Rust-Gate |
| Vault/SQLite-Trennung | Rust-Test prüft Datenbankbytes und fehlenden Vault-Eintrag | Windows/Rust-Gate |
| Rotation/Logout | Rust-Tests für Cleanup-Rollback und vollständiges lokales Logout | Windows/Rust-Gate |
| Offline-Policy | Rust- und React-Test auf `unconfigured`/fail-closed | verpflichtend |
| Logging | Rust-Redaktionstest und statischer Gate-Check | Windows/Rust-Gate |
| Frontend | TypeScript, Vite, Vitest | verpflichtend |
| UI-Regression | fünf Browserfälle, Axe, Layout, Themes, Locales, boxlose Vorschau | verpflichtend, wenn Browser ausführbar |
| Performance | Browser-Harness mit 100 Navigationen | verpflichtend, wenn Browser ausführbar |
| Rust | fmt, check, Clippy `-D warnings`, dreimal parallele Gesamttests | echter Windows-MSVC-Gate |

Ergebnisse und Exitcodes stehen im Phase-3-Zwischenstandsbericht. Ein fehlender Rust-/Windows-Runner wird nicht als bestanden behauptet.

