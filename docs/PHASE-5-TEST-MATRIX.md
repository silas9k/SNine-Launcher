# Phase 5 – Testmatrix und Gate

Status: lokaler Phase-5-Checkpoint vom 2026-08-01. „Bestanden“ bezeichnet nur
Prüfungen, die auf diesem Arbeitsstand tatsächlich ausgeführt wurden. Reale
Microsoft-/Minecraft-End-to-End-Starts und extern unkonfigurierte Provider werden
davon ausdrücklich nicht abgeleitet.

## Tatsächlich ausgeführte Teilnachweise

| Prüfung | reales Ergebnis | Nachweisgrenze |
|---|---|---|
| `node scripts/check-phase5-runtime-security.mjs` | bestanden | statischer Guard für Legacy-Abschaltung, IPC-Geheimnisse, Provider-, Signatur-, JAR-, Pfad- und Capability-Gates |
| gezielte Tests für den nativen Startpfad | bestanden, 16 Tests | Lock-/Hashprüfung, Native-Archivschutz, Argumente, Launch-ID und gezielter Stopp; kein realer Microsoft-/Minecraft-Start |
| gezielte NeoForge-Parser-/Plan-Tests | bestanden, 8 Tests im Gesamtlauf | sichere Fixtures für Parser, Plan, Blocker und Sandbox-Gates; keine reale Prozessorinstallation und kein Spielstart |
| `npm test` | bestanden | 71 Node-Tests, 28 Vitest-Tests, 327 vollständige DE-/EN-Schlüssel sowie alle statischen Guards |
| `npm run build` | bestanden | TypeScript-Prüfung und Vite-Produktionsbuild |
| `cargo fmt --all -- --check` | bestanden | vollständiger Rust-Formatcheck |
| `cargo check --locked` | bestanden | vollständiger Check in der vorhandenen MSVC-BuildTools-x64-Umgebung |
| `cargo clippy --locked --all-targets -- -D warnings` | bestanden | alle Rust-Targets ohne tolerierte Warnungen |
| `cargo test --locked -- --nocapture` | bestanden, 143 Tests | vollständiger Phase-5-Rust-Lauf einschließlich Windows-Junction-, Hardlink- und Job-Object-Prozessbaum-Fixtures |
| `npm run test:browser` | bestanden | fünf responsive Theme-/Locale-Fälle einschließlich Accessibility und boxloser Spielerbühne |
| `npm run test:performance` | bestanden | Browser-Performance-Harness; der temporäre JSON-Bericht wurde anschließend aus dem Quellbaum entfernt |

## Fachliche Sollmatrix

| Bereich | erforderlicher Nachweis | aktueller belegter Stand |
|---|---|---|
| Vanilla | authentifizierte Installation, Reparatur, Start und gezielter Stopp mit repräsentativer Java-17- und Java-21-Version | Unit-/statische Teilnachweise; reales E2E offen |
| Fabric | stabiler und Vorschau-Loader, korrekter Main Class, erhaltene Classpath-Reihenfolge, authentifizierter Start | Classpath-Regressionstest vorhanden; reales E2E offen |
| NeoForge | kontrollierter Installer, offline geparster Installationsplan, Laufzeitlock, Start und Reparatur | Parser und hashgebundener Plan produktiv integriert; Ausführung fail-closed, E2E nicht möglich |
| Java | kontrolliertes System-Java; Managed Java mit verifiziertem Manifest, Hash, Staging und atomarer Aktivierung | System-Java implementiert; realer Matrixlauf offen; Managed-Pfad unkonfiguriert |
| Auth | fehlende Zuordnung, erneute Anmeldung, Ownership und gültige Sitzung; keine Geheimnisse in IPC/SQLite/Logs | Implementierung und statische Guards vorhanden; Gesamttest offen |
| Revision | Fehler an jedem Operationszustand behält oder restauriert alte Revision und Runtime-Projektion | atomarer Projektions-Regressionspfad und vollständiger Rust-Lauf bestanden |
| Cache | Cachetreffer und Download werden vor Commit erneut gehasht; keine Hardlinks | Implementierung, Integritäts-, Copy- und Recovery-Tests bestanden |
| Natives | Traversal, ADS, Sondernamen, Case-Kollision, Symlinkmodus, Bombenbudget, Hashdrift und Pfadkonflikt | gezielte Tests bestanden |
| Prozesse | nur ein Start je Profil, Stop nur für Launch-ID, Exitstatus, Windows-Prozessbaum | Launch-ID-Isolation sowie suspendierte, vor Ausführung verifizierte Job-Object-Zuordnung und Kindprozess-Terminierung getestet |
| Komponentensignatur | gültiges Ed25519, unbekannter Schlüssel, falsche Domäne, Manipulation jedes signierten Feldes | Unit-Tests im vollständigen Rust-Lauf bestanden |
| Komponentenprovider | fehlende Konfiguration, ungültige Origin, Redirect, Content-Length, Größe, Hash und Abbruch | lokale Tests vorhanden; offizieller Provider extern blockiert |
| Komponentenkatalog/UI | nur verifizierte kompatible Releases, keine URLs im IPC, Auswahl statt freier Herkunftsdaten | typisierter URL-freier Vertrag und UI-Auswahl implementiert; Provider-E2E offen |
| Komponenten-JAR | Fabric-/NeoForge-Descriptor, Mod-ID, Traversal, ADS, Links, Reparse, Hardlink, Kollision und ZIP-Bombe | Unit-Tests im vollständigen Rust-Lauf bestanden |
| Komponentenwechsel | hinzufügen, Version wechseln, entfernen, Fehler ohne Zustandsänderung | ohne offiziellen Provider nur fixturebasiert nachweisbar |
| IPC | Vertrag v5, alle Befehle registriert, keine Token-/URL-Felder | statischer Nachweis bestanden |
| UI | Loading/Offline/Fehler, Capability deaktiviert, Tastatur, Fokus, Screenreaderstatus, keine Phantomfunktion | Unit-, statische, Browser-, Accessibility- und Performance-Gates bestanden; reale Desktop-Provider-E2E offen |
| Legacy-Abschaltung | keine alten globalen Commands, Quellen oder gebündelten JAR-Ressourcen | Dateien entfernt; Phase-5-Guard bestanden |

## Ausgeführtes Phase-5-Gate

Die folgende Kette wurde auf dem dokumentierten Arbeitsstand erfolgreich
ausgeführt:

```text
node scripts/check-phase5-runtime-security.mjs
npm test
npm run build
cargo fmt --all -- --check
cargo check --locked
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked -- --nocapture
npm run test:browser
npm run test:performance
```

Der dreifache Rust-Gesamtlauf sowie `npm ci` und der Tauri-/NSIS-Release-Build sind
bewusst Bestandteile des endgültigen Gesamtgates nach Phase 10 und werden hier
nicht vorweggenommen. Der vorhandene Visual-Studio-BuildTools-Compiler und das
Windows SDK wurden für die oben genannten Rust-Prüfungen erfolgreich initialisiert.

## Reale Integrationstests

Netzwerkbasierte Tests dürfen nur die fest codierten offiziellen Mojang-, Fabric-
und NeoForge-Hosts verwenden. Testserver für den S9Lab-Komponentenprovider müssen
lokal oder eindeutig als Testfixture betrieben werden und dürfen keinen
Produktions-Capabilitystatus erzeugen.

Ein tatsächlicher Vanilla-, Fabric- oder NeoForge-Start auf der
Windows-Zielarchitektur ist bislang nicht belegt. Falls diese End-to-End-Nachweise
lokal technisch nicht möglich sind, muss der jeweilige Pfad als ungeprüft oder
`unconfigured` und nicht als vollständig abgeschlossen bezeichnet werden.

Insbesondere darf der bestandene NeoForge-Parsertest nicht als erfolgreiche
NeoForge-Installation ausgelegt werden.
