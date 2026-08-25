# Phase 6 – Testmatrix und Gate

Status: bestandener lokaler Integrationsstand vom 2026-08-11. Alle nachfolgend
als bestanden ausgewiesenen Prüfungen liefen auf demselben formatierten
Phase-6-Quellstand.

## Vorhandene gezielte Nachweise

| Bereich | ausgeführter Nachweis | Ergebnis/Nachweisgrenze |
|---|---|---|
| Resolver und Lock | Reproduzierbarkeit, Backtracking, Graphlimits, Runtimebindung, Required/Optional/Incompatible, Zyklen, einseitige Konflikte, Lock-Manipulation und Sortierung | bestanden im Rust-Gesamtlauf |
| Pack-Mitgliedschaft | gemeinsames Mitglied, eindeutiger Besitzer, Standardaktivierung, manuelle Provenienz, OR-Semantik und Versionsbindung | bestanden; reales Netzwerk-Mehrpack-E2E bleibt separat |
| Overrides im Lock | Datei-/Vorfahrenkollision, leere hashgebundene Seeds, Reihenfolge, Packbindung und Gesamtbudget | bestanden |
| Inhaltsarchive | alle vier Inhaltsarten, Traversal, ADS, Separatoren, Case-/Unicode-Kollisionen, Symlinkmodus, Hardlinks, Datei-/Verzeichnisalias, ZIP-Bombe, Größe und Hash | bestanden |
| Modrinth | typisierte Filter, exakte API-/CDN-Hosts, Redirect-/Antwortgrößenschutz, Identitätsbindung, SHA-512, URL-freie Serialisierung und Abhängigkeitsgrenzen | lokale Fixtures bestanden; reales Netzwerk-E2E separat offen |
| Profilformat | deterministischer geheimnisfreier Roundtrip, Deduplizierung, Packcontainer und Overrides einschließlich leerer Seeds, Hardlink-, Pfad-, Polyglott-, Kommentar-, Bomben- und Hashschutz | bestanden |
| Startprojektion | normale Inhalte, deaktivierte Einträge, nicht projizierter Packcontainer, Fremddateien, Case-Kollisionen, Hashdrift, Marker-Manipulation, Aktivierungsrollback und Seed-Lebenszyklus | bestanden |
| Profilduplizierung | neue V2-Identität, Cachematerialisierung sowie Ausschluss des Quellmarkers und unveränderlicher Projektionsdateien | bestanden |
| Inhaltseditor | Accessibility, reproduzierbare Inventaranzeige, Pack-Member-Schutz, typisierte Aktivierung, profilgebundene Suche/Installation, Fehler bei lokaler Datei und bestätigte Entfernung | sechs gezielte UI-Tests bestanden; kein nativer Dateidialog- oder Provider-E2E |
| statischer Phase-6-Guard | `npm run check:phase6` plus Node-Regressionen für vollständige Befehlsregistrierung, Vertragsdowngrade und URL-/Secret-Felder | bestanden |

## Fachliche Sollmatrix

| Bereich | erforderlicher Nachweis | aktueller Implementierungsstand |
|---|---|---|
| Inhaltsarten | Mods, Modpacks, Shader und Ressourcenpakete suchen, anzeigen, installieren und verwalten | Domäne, Service, IPC und UI integriert; reales Provider-E2E offen |
| Kompatibilität | Minecraft-, Loader- und Loaderversion für Suche, Release, Pack und Lock erzwingen | implementiert und durch gezielte Tests abgedeckt |
| Auflösung | deterministische Required-/Optional-Auflösung, Backtracking, Zyklen, Tiefen-/Größenlimits | implementiert; Gesamtlauf bestanden |
| Konflikte | ein- und zweiseitige Inkompatibilität sowie Ziel-/Vorfahren-/Case-/Unicode-Kollisionen fail-closed | implementiert; Gesamtlauf bestanden |
| Mutation | jede Aktivierung, Installation, Entfernung und Aktualisierung erzeugt eine neue Revision; Fehler behält alten Zustand | implementiert; Operations- und Servicefälle bestanden |
| Updateprüfung | begrenzte, parallele Prüfung nur eigenständig verwalteter Modrinth-Einträge; Packmitglieder nur über Packupdate | eigener IPC-Pfad integriert; reales Provider-E2E offen |
| MRPACK-Quellen | lokaler Import und direkte Modrinth-Packinstallation verwenden denselben geprüften Kern | implementiert; beide End-to-End-Wege separat nachzuweisen |
| MRPACK-Preflight | wiederholte Stagingprüfung, abschließendes Rehashing vor Cache, Index-/Runtime-/Ziel-/Budgetbindung | implementiert; lokale Manipulations- und Fehlerfälle bestanden |
| Pack-Mitglieder | exakte Versionen, Provider-Rückbindung, ein Besitzer, geteilte Mitglieder, konfliktfreier Packwechsel | implementiert; repräsentatives Mehrpack-E2E offen |
| Overrides | sichere `overrides/`-/`client-overrides/`-Inventarisierung, Hash/Cache/Lock, kontrolliertes Säen und Erhalt lokaler Änderungen | Kern-, Service- und Projektionsregressionen bestanden; reales MRPACK-E2E offen |
| Cache | Container, Mitglieder und Seeds werden nach Gesamtvalidierung vor Revision materialisiert und erneut gehasht | implementiert; lokale Fehlerfälle bestanden |
| lokale Datei | absoluter eindeutiger Pfad, Link-/Reparse-/Hardlinkschutz, Descriptor, Hash und Staging | implementiert; nativer Dateiauswahlpfad offen |
| Profiltransfer | deterministischer Export der Revisions-Seeds, nicht lokaler Instanzänderungen; Import als neues Profil ohne Geheimnisse | Formatpfad abgedeckt; vollständiger Service-E2E offen |
| Startprojektion | normale Inhalte transaktional; Override-Seeds nie still überschreiben oder vorhandene lokale Stände löschen | implementiert und Gesamtlauf bestanden |
| Runtimewechsel | Reparatur/Komponentenwechsel erhalten Inhalte; inkompatibler Laufzeitwechsel gesperrt | implementiert; Gesamtlauf bestanden |
| IPC | Vertrag v6, zwölf registrierte Befehle, keine Token-, Raw-URL- oder absoluten Ausgabepfade | 41 registrierte und gemeinsam typisierte Verträge bestanden |
| Performance | begrenzte Providerantworten, 256 Updatekandidaten/8 parallele Abfragen, Graph-, Pack-, Override- und Archivbudgets | Schutzlimits plus Browser-Harness bestanden |

## Ausgeführtes Phase-6-Checkpoint-Gate

| Prüfung | Ergebnis |
|---|---|
| `cargo fmt --all` und `cargo fmt --all -- --check` | bestanden |
| `cargo check --locked` | bestanden |
| `cargo clippy --locked --all-targets -- -D warnings` | bestanden, keine Warnung |
| `cargo test --locked -- --nocapture` | 216 bestanden, 0 fehlgeschlagen |
| `npm test` | alle statischen Gates bestanden; 75 Node- und 34 UI-Tests bestanden |
| `npm run build` | bestanden; 1.613 Module, 361,84 kB JS und 57,42 kB CSS vor gzip |
| `npm run verify:phase2` | fünf responsive Theme-/Locale-Browserfälle bestanden |
| `npm run tauri:build` | bestanden; unsignierte Windows-Anwendung und NSIS-Diagnoseinstaller 1.0.8 erzeugt |
| Browser-Performance | Shell 31,6 ms, interaktiv 64,1 ms; Navigation p95 2,3 ms/max. 20,6 ms; Heapdelta 1,52 MiB |

Die Performancegrenzen waren 3.000 ms Shell, 100 ms Navigation und 30 MiB
Heapdelta. Der Harness misst Chromium und ist kein Ersatz für einen nativen
Tauri-Kaltstart oder den Prozess-Working-Set-Nachweis des Endgates.

## Reale Integrationstests

Netzwerkbasierte Tests dürfen nur die im Produktpfad kontrollierten offiziellen
Modrinth-Routen tatsächlich herunterladen. Ein echter Download muss dieselben
Host-, Identitäts-, Größen-, SHA-512-, SHA-256-, Staging-, Cache- und
Archivprüfungen durchlaufen.

Ein vollständiger MRPACK-Nachweis benötigt mindestens:

1. direkte Installation eines Modrinth-Packprojekts,
2. lokalen Import desselben Formatpfads,
3. Required-, Optional- und Unsupported-Clientmitglieder,
4. sichere `overrides/`- und priorisierte `client-overrides/`-Seeds,
5. lokale Änderung und Löschung eines bereits gesäten Overrides,
6. Packdeaktivierung, -update und -entfernung ohne Überschreiben lokaler Daten,
7. geteiltes Mitglied zweier Packs mit gleicher Version,
8. Abweisung verschiedener Mitgliedsversionen und
9. Abbruch bei Download-, Hash-, Provider-, Pfad-, Budget- oder
   Revisionsfehlern ohne Wechsel der aktiven Revision.

Der dreifache Rust-Gesamtlauf, öffentliches `npm ci`, Tauri-/NSIS-Build und
Cleanroom-Nachweis gehören zum endgültigen Gesamtgate nach Phase 10 und werden
hier nicht vorweggenommen.
