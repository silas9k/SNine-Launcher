# Phase 7 – Testmatrix und Gate

Status: bestandener lokaler Checkpointstand vom 2026-08-11. Alle als bestanden
ausgewiesenen Prüfungen liefen auf demselben formatierten Phase-7-Quellstand.

## Fachliche Matrix

| Bereich | erforderlicher Nachweis | lokaler Stand |
|---|---|---|
| Kanäle | Launcher, Profile, S9Lab Client und Inhalte bleiben getrennt | implementiert; nicht konfigurierte Produktionskanäle fail-closed |
| Richtlinie | manuell/automatisch nur für verfügbare Kanäle, atomare Persistenz | gezielter Rust-Test und statischer Guard |
| Vorschau | Basisrevision, Auswahl und verständliche Änderungsübersicht | Backend und UI integriert; realer Provider-E2E separat |
| Verifikation | Inhaltsdownload an SHA-512 und internen SHA-256 gebunden | Phase-6-Pipeline wiederverwendet; Launcher/Client extern blockiert |
| Updatefehler | Wiederherstellungspunkt vor Mutation, Rückkehr zur Basisrevision | Operationspfad implementiert; Gesamtlauf bestanden |
| Rollback | nur festgeschriebene Revision desselben Profils; neue Revision statt Pointer-Rewrite | implementiert |
| Backup | Staging, Grenzwerte, kein `.s9lab`, keine unveränderlichen Projektionsdateien, SHA-256-Inventar | gezielter Rust-Test bestanden |
| Restore | exakte Inventarprüfung, Rehash nach Kopie, neues isoliertes Profil, Quellstand bleibt erhalten | gezielter Rust-Test bestanden |
| Manipulation | geänderte Backupdatei und falsche erwartete Kopie werden abgewiesen und bereinigt | gezielter Rust-Test bestanden |
| Auswahl | Konto, ungefährliche Einstellungen und veränderliche Dateien getrennt auswählbar | Backend und bestätigter UI-Dialog integriert |
| UI | dunkles responsives Update-Center, klare Sperr-/Recoveryzustände, Accessibility | drei gezielte UI-Tests, Chromium-Desktop-/Mobilprüfung und Gesamtgate bestanden |
| IPC | Vertrag v7, acht Phase-7-Befehle, keine URL-/Token-/Pfadausgabe | 49 gemeinsam typisierte Verträge bestanden |
| statischer Guard | vollständige Registrierung und Sicherheitsinvarianten | `check:phase7` und Node-Regression integriert |

## Ausgeführtes Checkpoint-Gate

| Prüfung | Ergebnis |
|---|---|
| `cargo fmt --all` und `cargo fmt --all -- --check` | bestanden |
| `cargo check --locked` | bestanden |
| `cargo clippy --locked --all-targets -- -D warnings` | bestanden, keine Warnung |
| `cargo test --locked -- --nocapture` | 220 bestanden, 0 fehlgeschlagen |
| öffentliches `npm ci` über `https://registry.npmjs.org/` | 187 Pakete aus Lockdatei installiert; danach Gates erneut ausgeführt |
| `npm test` | alle statischen Gates bestanden; 78 Node- und 37 UI-Tests bestanden |
| `npm run build` | bestanden; 1.616 Module, 384,13 kB JS und 67,33 kB CSS vor gzip |
| `npm run verify:phase2` | fünf responsive Theme-/Locale-Fälle und Performance-Harness bestanden |
| Browser-Sichtprüfung Update-Center | Desktop und 390 px Mobilbreite; kein horizontales Überlaufen |
| `npm run tauri:build` | bestanden; Anwendung und unsignierter NSIS-Diagnoseinstaller 1.0.8 erzeugt |

Browser-Performance: Shell-ready 9,8 ms, interaktiv 49,0 ms,
Navigation p95 3,1 ms/maximal 24,1 ms und behaltenes Heapdelta 1,52 MiB. Die
Grenzen sind 3.000 ms, 100 ms und 30 MiB. Der Harness ersetzt weder einen nativen
Tauri-Kaltstart noch den späteren Prozess-Working-Set-Nachweis.

Diagnoseinstaller:

- Pfad: `src-tauri/target/release/bundle/nsis/S9Lab Launcher_1.0.8_x64-setup.exe`
- Größe: 6.099.440 Bytes
- SHA-256: `2FE0273F5AEA5C124BE7F4E5E5CB05C67B97ABCE8CF5EFDF4C644BCEBD3458E3`
- Authenticode-Status: `NotSigned`

Das Authenticode-Gate bleibt entsprechend
`PHASE-7-EXTERNAL-BLOCKERS.md` offen. Das ist kein lokal bestandener
Produktionsrelease.
