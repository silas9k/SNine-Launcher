# Phase 4 – Testmatrix

| Bereich | Nachweis | Gate |
|---|---|---|
| Profilanlage/Revision | Rust-Tests für Manifest-/Lock-Commit und kontrolliertes Rollback | Windows/Rust |
| Duplikationsisolation | Konfiguration und Welt separat kopieren, Quelle nach Mutation unverändert, keine Hardlinks | Windows/Rust |
| Lebenszyklus | Aktiv → Archiv → Papierkorb → Archiv → Aktiv | Windows/Rust |
| Rückwärtsmigration | echtes v4-Profil wird nach v5 vollständig gelesen | Windows/Rust |
| 1.000 Profile | indizierte SQLite-Projektion unter festem Testbudget | Windows/Rust |
| Cacheintegrität | Hash/Größe, unabhängige Profilkopie | Windows/Rust |
| Mark/Sweep | Profil/Papierkorb/Backup behalten; unreferenziertes Objekt quarantänisieren; späte Referenz reaktivieren | Windows/Rust |
| Kein permanentes Löschen | statischer Phase-4-Guard und SQLite-Constraint `unconfigured` | Node + Windows/Rust |
| UI | Erstellen, Lifecycle-Aktionen, Speicherübersicht, bestätigte Quarantäne | Vitest |
| Browser/A11y/Layout | Bibliothek plus fünf Theme-/Locale-/Viewportfälle; Axe; kein horizontaler Überlauf | Browser |
| Phase-2-Regression | transparente boxlose Spielervorschau und fünf PNG-Hashes | Browser |
| Gesamtregression | Guards, npm test/build, fmt/check/Clippy, dreimal normale parallele Rust-Gesamttests | kombiniert |

Auf einem Host ohne Cargo oder Windows-MSVC werden native Ergebnisse ausdrücklich nicht als bestanden behauptet.
