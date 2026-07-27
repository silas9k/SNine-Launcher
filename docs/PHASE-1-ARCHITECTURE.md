# Phase 1 – Architekturübersicht

## Geltungsbereich

Phase 1 ergänzt ausschließlich den Plattform-, Speicher- und Operationskern. Bestehende UI- und Minecraft-Funktionen bleiben fachlich unverändert. Die öffentliche Produktversion bleibt `1.0.8`.

## Komponenten

```text
Tauri-Anwendung
└── CoreServices
    ├── PlatformAdapter
    │   ├── SystemPlatform
    │   └── FixedPlatform (Tests/Demo)
    ├── LauncherPaths
    ├── PathRegistry + SecurePath
    ├── Storage (SQLite)
    ├── OperationEngine + Startup-Recovery
    ├── DownloadService
    ├── CacheStore
    └── read-only IPC: phase1_core_status
```

### PlatformAdapter

`SystemPlatform` löst den produktiven Datenstamm über das Betriebssystem auf. `FixedPlatform` injiziert ausschließlich einen expliziten Stamm für Tests und technische Demos. Fachmodule kennen keine `%LOCALAPPDATA%`- oder plattformspezifischen Konstanten.

### LauncherPaths

Der Plattformstamm wird in kontrollierte Bereiche aufgeteilt:

```text
<data-root>/
├── data/
├── profiles/
├── cache/
│   └── blobs/sha256/
├── staging/operations/
├── migration/
├── backups/
└── logs/launcher/
```

Der Launcher-Stamm wird zunächst gegen den nächsten vorhandenen Vorfahren validiert und komponentenweise erstellt. Anschließend werden die Unterordner über `PathRegistry`, `SecurePath` und `security::fs` als registrierte Wurzeln angelegt. Auch `data/launcher.db` wird vor der SQLite-Initialisierung als `SecurePath` aufgelöst.

### PathRegistry

Die Registry besitzt einen gemeinsamen Launcher-Anker und benannte registrierte Wurzeln. Ein aufgelöster Pfad trägt:

- Root-ID,
- Launcher-Anker,
- registrierte Wurzel,
- normalisierten relativen Pfad,
- absoluten Zielpfad.

Dadurch wird auch ein später manipulierter übergeordneter Ordner zwischen Launcher-Anker und registrierter Wurzel erneut geprüft.

### Storage

Die SQLite-Datenbank liegt unter `data/launcher.db`. Jede Verbindung aktiviert:

- Foreign Keys,
- WAL-Journal,
- `synchronous = FULL`,
- Busy Timeout.

Migrationen sind eingebettet, nummeriert und jeweils transaktional. SQLite speichert keine Tokens oder Geheimnisse.

### OperationEngine

Eine simulierte Profilinstallation wird vollständig unter `staging/operations/<operation-id>/revision` vorbereitet. Manifest, Lockdatei und Payload werden geprüft, bevor der Revisionsordner in den Profilbereich verschoben wird. Erst danach werden Revisionsdatensatz und aktiver Revisionszeiger gemeinsam in einer SQLite-Transaktion aktiviert.

Beim Start prüft `CoreServices` unvollständige Operationen und führt die Recovery aus, bevor der Status über IPC verfügbar wird.

### DownloadService

Der Dienst akzeptiert nur bereits durch einen registrierten Provider aufgelöste HTTPS-Downloads über den Standardport 443. Ziel ist ausschließlich der Staging-Bereich. Teil-Dateien werden sicher mit `create_new` angelegt und erst nach Größen- und SHA-256-Prüfung aktiviert.

### CacheStore

Phase 1 stellt die Grundlage eines unveränderlichen, SHA-256-adressierten Caches bereit. Aktiviert werden sichere Kopien, niemals Hardlinks. Die vollständige konservative Garbage Collection gehört zu einer späteren Phase.

### IPC

Phase 1 fügt genau einen read-only Command hinzu: `phase1_core_status`. Test- und Failure-Injection-Funktionen sind nur unter `#[cfg(test)]` kompiliert und nicht über Tauri erreichbar.

## Datenhoheit

| Daten | Maßgebliche Quelle |
|---|---|
| Profillebenszyklus, aktiver Revisionszeiger, Operationen | SQLite |
| Portable Profilabsicht | Manifest |
| Exakt aufgelöster Zustand und Hashes | Lockdatei |
| Globale lokale Einstellungen | `settings.json` |
| Gerätespezifische Profilabweichungen | `local-settings.json` |
| Geheimnisse | Betriebssystem-Schlüsselspeicher |
| Veränderliche Spieldaten | Instanzdateisystem |

Eine Revision gilt nur dann als aktiv, wenn SQLite auf ein zusammengehöriges Manifest-Lock-Paar mit den validierten Hashes verweist.
