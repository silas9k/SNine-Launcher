# Phase 1 – Implementierungsplan

Status vor Beginn: verbindlicher Phase-0-v1.1.2-Quellstand, Produktversion 1.0.8 unverändert.

## Ziel

Phase 1 implementiert ausschließlich den Plattform-, Speicher- und Operationskern. Die bestehende Minecraft- und UI-Logik bleibt funktional und wird nicht fachlich neu gestaltet.

## Geplante Dateien und Module

### Plattform und Pfade

- `src-tauri/src/platform/mod.rs`
  - `PlatformAdapter` für produktive und injizierbare Testpfade.
  - `SystemPlatform` für den produktiven Datenstamm.
  - `FixedPlatform` für Tests und technische Demos.
- `src-tauri/src/app/paths.rs`
  - zentrale Struktur für `data`, `profiles`, `cache`, `staging/operations`, `migration`, `backups` und `logs/launcher`.
  - bestehende Aufrufer erhalten weiterhin typisierte Pfade.

### Pfad- und Dateisicherheit

- `src-tauri/src/security/mod.rs`
- `src-tauri/src/security/paths.rs`
- `src-tauri/src/security/fs.rs`
  - relative Pfade werden komponentenweise validiert.
  - Windows-Sondernamen, ADS, Punkt-/Leerzeichen-Endungen, Trennzeichenmanipulationen und Kollisionen werden abgewiesen.
  - bestehende Symlinks, Reparse Points und Hardlinks werden vor sicherheitsrelevanten Schreibzugriffen abgewiesen.
  - atomare Schreib- und Verschiebeoperationen bleiben innerhalb registrierter Wurzeln.

### SQLite und Migrationen

- `src-tauri/src/storage/mod.rs`
- `src-tauri/src/storage/sqlite.rs`
- `src-tauri/src/storage/migrations.rs`
- `src-tauri/src/storage/models.rs`
  - eingebettete SQLite-Anbindung über die Plattformbibliothek.
  - versionierte Migrationen.
  - Tabellen für Profile, Revisionen, Operationen, Journal und Cache-Grundlagen.
  - SQLite verweist auf die Hashes des validierten Manifest-Lock-Paars.

### Operationsengine

- `src-tauri/src/operations/mod.rs`
- `src-tauri/src/operations/model.rs`
- `src-tauri/src/operations/engine.rs`
- `src-tauri/src/operations/recovery.rs`
  - explizite Zustandsmaschine.
  - Staging unter `staging/operations/<id>`.
  - Journal mit Gegenoperationen.
  - atomare Revision-Aktivierung über Dateisystem-Rename plus SQLite-Transaktion.
  - deterministische Recovery nach Unterbrechung.
  - Failure-Injection ausschließlich über Rust-Test-/Demo-Schnittstellen, niemals als Produktions-IPC.

### Download und Cache

- `src-tauri/src/download/mod.rs`
  - HTTPS-, Port-443- und Providerprüfung.
  - Größen- und SHA-256-Prüfung.
  - `.partial`-Dateien im Staging und Aktivierung erst nach erfolgreicher Prüfung.
- `src-tauri/src/cache/mod.rs`
  - unveränderliche, prüfsummenadressierte Cache-Metadaten als Phase-1-Grundlage.

### Typisierte IPC-Verträge

- `contracts/ipc-contracts.json`
- `scripts/generate-ipc-contracts.mjs`
- `src/lib/generated/ipc-contracts.ts`
- `src-tauri/src/ipc/mod.rs`
  - ein neuer, nur lesender `phase1_core_status`-Command.
  - stabiler Fehlercode, Message-Key und Parameter.
  - Generator-/Vertragstests verhindern einseitige Commands.

## Minimales Datenmodell

### `profiles`

- `id`
- `lifecycle_state`
- `active_revision_id`
- Erstellungs-/Änderungszeitpunkte

### `profile_revisions`

- `id`, `profile_id`, `operation_id`
- `manifest_sha256`, `lock_sha256`
- relative Manifest-/Lockpfade
- Revisionsstatus und Erstellungszeitpunkt

### `operations`

- ID, Typ, optionale Profil-ID
- Zustand
- serialisierter Änderungsplan
- kontrollierter relativer Staging-Pfad
- vorherige und Zielrevision
- Zeitpunkte und stabiler Fehlercode

### `operation_journal`

- streng steigende Sequenz pro Operation
- Schritt, Status und technische Details
- passende Gegenoperation

### `cache_blobs` und `cache_references`

- SHA-256, Größe, relativer unveränderlicher Pfad und Status
- Besitzerreferenz als Grundlage für spätere konservative Bereinigung

## Zustandsmaschine und Commit

`planned → staging → verifying → ready-to-commit → committing → validating → completed`

Fehler führen abhängig vom Aktivierungsstand über `rolling-back` nach `rolled-back` oder nach `failed`.

Eine Profilrevision wird erst aktiv, wenn:

1. Manifest und Lockdatei vollständig im Staging liegen.
2. beide Hashes geprüft wurden.
3. die Lockdatei den Manifest-Hash referenziert.
4. der Revisionsordner atomar in den Profilbereich verschoben wurde.
5. der Revisionsdatensatz und der aktive Revisionszeiger in einer SQLite-Transaktion gemeinsam geschrieben wurden.

Vor dem SQLite-Commit bleibt die alte Revision aktiv. Nach dem SQLite-Commit ist die neue Revision nur dann gültig, wenn Dateien und Hashes erneut validiert werden. Recovery stellt ansonsten den vorherigen Zeiger wieder her.

## Tests

- leere Datenbank, reproduzierbare Migrationen, Wiederöffnung und Vorwärtsmigration.
- Transaktionsrollback bei absichtlichem Datenbankfehler.
- Unterbrechung an jedem relevanten Operationspunkt und Recovery nach Neustart.
- kein gemischter Profilzustand.
- Pfadtraversal, Windows-Sondernamen, ADS, Punkt-/Leerzeichen-Endungen, Unicode-/Case-Kollisionen, Links und Pfadlängen.
- Download-Hashfehler, Größenfehler und Abbruch.
- IPC-Generator und Vertragsabgleich.
- Schema- und Fixture-Prüfung auf Geheimnisse.
- bestehende Phase-0-Prüfungen sowie Rust- und Frontend-Builds.

## Reihenfolge der Umsetzung

1. Plattformadapter und Verzeichnislayout.
2. Pfad- und Dateisicherheitsmodell.
3. SQLite-Wrapper, Migrationen und Repository-Methoden.
4. Operationsmodell, Commit und Recovery.
5. Download- und Cache-Grundlage.
6. typisierte IPC-Verträge und Core-Initialisierung.
7. Tests, Windows-CI, Dokumentation und bereinigte Übergabe.
