# Phase 2 – Testmatrix

## Selbst ausgeführte Prüfungen in der Implementierungsumgebung

| Bereich | Prüfung | Ergebnis |
|---|---|---|
| Ausgangspaket | SHA-256 des verbindlichen Phase-2-v1.0.2-Eingangskandidaten | bestätigt: `b18075781b6156cbc3fa088f5f5a5f0416c7b05c9a2c2c6239bc05008d0bfff8` |
| Registry | zusätzlich strukturierte `package.json`-Scripts und `set "NPM_CONFIG_REGISTRY=WERT"` in CMD/BAT | getrennte Positiv-/Negativtests und isolierte inspectProject-E2E-Fälle |
| Dependencies | saubere Installation mit öffentlichem `npm ci` und isoliertem Cache | erfolgreich, 188 Pakete |
| Security/Static | `npm test` | erfolgreich |
| UTF-8/Secrets/Security | bestehende Phase-0-/Phase-1-Prüfungen | erfolgreich |
| IPC | generierte Verträge und Parität | 22 geprüft, 3 gemeinsam typisiert |
| Rust statisch | Phase-1-/Phase-2-Quellprüfung | 36 Dateien erfolgreich geprüft |
| i18n | 152 Schlüssel je Sprache, Parität und Parameter | erfolgreich |
| Visible Text | unerlaubte sichtbare Rohtexte | keine Treffer |
| Designsystem | Tokens, Ressourcen, Mindestgröße, verbotene Bereiche | erfolgreich |
| Unit | Vitest | 12 Tests in 7 Dateien erfolgreich |
| Node-Regression | bestehende Guards/Browser-Cleanup plus v1.0.3-Prüferstruktur | 58 Tests vorgesehen; finaler Cleanroom-Nachweis im Übergabebericht |
| Accessibility Unit | axe-core App-Shell | keine serious/critical Verstöße |
| TypeScript | `tsc --noEmit` | erfolgreich |
| Vite | Produktionsbuild | erfolgreich, 1.603 Module |
| Browser | fünf Responsive-/Theme-/Locale-Fälle | v1.0.2 nutzerseitig erfolgreich; v1.0.3 UI unverändert |
| Browser Accessibility | axe-core serious/critical | v1.0.2 nutzerseitig erfolgreich; v1.0.3 UI unverändert |
| Layout | Dokument-/Elementüberlauf | v1.0.2 nutzerseitig erfolgreich; v1.0.3 UI unverändert |
| Dialog/Task-Center | Fokus, Escape, Viewportgrenzen | v1.0.2 nutzerseitig erfolgreich; v1.0.3 UI unverändert |
| Reduced Motion | Tastatur + tatsächliche Abschaltung | v1.0.2 nutzerseitig erfolgreich; v1.0.3 UI unverändert |
| Performance | 100 Navigationen und JS-Heap | v1.0.2 nutzerseitig erfolgreich |
| Vorschauintegration | keine Card/Panel-Styles, Clipping oder Überlappung | v1.0.2 nutzerseitig erfolgreich; v1.0.3 UI/CSS unverändert |
| Rust Format | `cargo fmt --all -- --check` | v1.0.2 nutzerseitig ausschließlich wegen zwei Umbrüchen fehlgeschlagen; v1.0.3-Nachprüfung offen |
| Cargo Check/Clippy/Test | vollständige native Prüfung | im v1.0.2-Lauf nach Rustfmt nicht erreicht; v1.0.3-Windows-Gate offen |

Am Projekt, an der CSP und an produktiven Sicherheitsregeln wurde nichts gelockert. Die eigenen v1.0.3-Ergebnisse werden im finalen Übergabebericht dokumentiert.

## Nutzerseitiger Phase-2-v1.0.2-Windows-Lauf

Auf echtem Windows 11 waren SHA/Cleanroom, Toolchain-Vorprüfung, alle drei Guards, öffentliches `npm ci`, `npm test` mit 58/58 Node- und 12/12 Vitest-Tests, TypeScript/Vite, fünf Browserfälle, Accessibility, Layout, visuelle Regression, Performance und `verify:phase2` erfolgreich. Der erste und einzige Abbruch erfolgte bei `cargo fmt --all -- --check`; rustfmt 1.9.0-stable meldete ausschließlich zwei Umbruchabweichungen in `src-tauri/src/app/config.rs`. Nachfolgende Cargo-/NSIS-Schritte wurden nicht erreicht. Dies ist ein Nutzerergebnis, keine eigene Windows-Verifikation.

## Nutzerseitiger Phase-2-v1.0.1-Windows-Lauf

Vom Nutzer auf echtem Windows 11 als erfolgreich gemeldet wurden SHA/Cleanroom, Node 24.18.0, Windows-MSVC-Rust 1.96.0, öffentliche Registry, Guards, `npm ci`, 54/54 Node- und 12/12 Vitest-Tests, Build mit 1.603 Modulen, fünf Browserfälle, Accessibility, Layout, boxlose Vorschau, Performance, Rust-Formatierung, Cargo Check und Clippy.

Der erste normale parallele Rust-Gesamtlauf scheiterte nach 34 bestandenen an 7 fehlgeschlagenen Tests: eine echte Settings-Replace-Kollision und sechs durch den zu langen Prüfer-TEMP ausgelöste Pfadbudgetfehler. Die drei Gesamtläufe und der nachgelagerte NSIS-Schritt gelten daher nicht als bestanden. Diese Resultate sind Nutzerergebnisse und keine eigene Windows-Verifikation.

## Nutzerseitiger Phase-1-Windows-Nachtrag

Der Nutzer hat den unveränderten Phase-1-v1.0.3-Ausgangsstand auf einem echten Windows-MSVC-System geprüft:

- `npm ci`, `npm test`, TypeScript und Vite erfolgreich,
- Cargo-Formatierung, Check und Clippy erfolgreich,
- Junction-, Hardlink- und Crash-Recovery-Regression erfolgreich,
- 36 Rust-Tests bestanden, 0 fehlgeschlagen,
- Tauri-Release-Build und NSIS-Bundle erfolgreich.

Der angegebene SHA-256 des lokalen, **unsignierten** NSIS-Testinstallers lautet:

`8BEB9EA6F568BCD8D38BA623458499B7E8776387B04DED315B1E2ED86CEE0EAE`

Diese Ergebnisse sind nutzerseitig und nicht als eigene Phase-2-Rust-Verifikation gekennzeichnet.

## Nicht vollständig selbst ausführbar

Auf dem aktuellen Implementierungshost ist keine Cargo-/Rustfmt-/Windows-MSVC-Toolchain verfügbar. Daher werden für Phase 2 v1.0.3 nicht als selbst bestanden behauptet:

- `cargo check --locked`,
- `cargo clippy --locked --all-targets -- -D warnings`,
- vollständiges `cargo test --locked -- --nocapture`,
- Tauri-Release-Build,
- lokales NSIS-Bundle,
- native Windows-Prozessbaum-Performance.

Die genauen Befehle stehen in `VERIFY-PHASE2-V1.0.3-WINDOWS.ps1` und `docs/PHASE-2-WINDOWS-VERIFICATION.md`.
