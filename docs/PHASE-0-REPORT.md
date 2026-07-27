# S9Lab Phase 0 – Abschlussbericht v1.1.2

**Quellbasis:** `S9Lab-Launcher-Phase0-v1.1.2-source.zip`  
**Ergebnis:** final bereinigter Phase-0-v1.1.2-Quellstand  
**Phase 1:** nicht begonnen

## 1. Status

Der Quellstand v1.1.2 wurde als neue verbindliche Grundlage untersucht. Die vom Projektinhaber unter Windows durchgeführten Korrekturen sind im Quelltext beziehungsweise in den Lockdateien vorhanden und untereinander konsistent.

Der Stand bleibt **nicht veröffentlichungsfähig**. Der erzeugte Windows-Installer und die ausführbare Datei sind laut bereitgestelltem Windows-Prüfergebnis nicht mit Authenticode signiert. Es wurden keine Releases veröffentlicht, keine produktiven Dienste verändert, keine Git-Historie umgeschrieben und keine externen Schlüssel widerrufen.

## 2. Bestandsaufnahme des angelieferten ZIPs

Die ZIP-Struktur wurde vor Änderungen direkt geprüft:

- ZIP-Integrität erfolgreich; kein beschädigter Eintrag.
- 115 Archiveinträge, davon 111 Dateien.
- keine absoluten Pfade und keine `..`-Pfadsegmente.
- keine `.git`-, `.idea`-, `.vscode`-, `node_modules`-, `dist`- oder `target`-Verzeichnisse.
- keine Dateien mit privaten Schlüssel- oder Zertifikatserweiterungen wie `.key`, `.pem`, `.p12` oder `.pfx`.
- keine Backup-Dateien nach den geprüften Backup-Namensmustern.
- `src-tauri/gen` war als generiertes Tauri-Schemaverzeichnis enthalten. Es ist für das Quellübergabepaket nicht erforderlich, steht bereits in `.gitignore` und wurde aus dem finalen Paket entfernt.

Die bekannten privaten Schlüsselmarker kommen ausschließlich als Suchbegriffe im Prüfskript `scripts/check-secrets.mjs` vor. Das Skript schließt sich selbst plattformunabhängig von der Inhaltsprüfung aus. Es wurde kein tatsächliches privates Schlüsselmaterial gefunden.

## 3. Bestätigung der lokalen v1.1.2-Korrekturen

### Cargo.lock

Der neue Lockfile-Stand wurde mit v1.1.1 verglichen:

- v1.1.1: 557 Pakete
- v1.1.2: 538 Pakete
- 19 Pakete entfernt
- keine neuen Pakete hinzugefügt

Die Änderungen betreffen nicht mehr benötigte transitive Abhängigkeiten und bereinigte Abhängigkeitsreferenzen. Der Launcher verwendet weiterhin die in `Cargo.toml` festgelegten direkten Abhängigkeiten. Der erfolgreiche `cargo check --locked`-Lauf wurde vom Projektinhaber unter Windows bereitgestellt.

### Rust-Code

Strukturell bestätigt:

- ungenutzter `Path`-Import aus `src-tauri/src/app/config.rs` entfernt.
- ungenutzte Fehlerzustände `JavaNotFound` und `AlreadyRunning` entfernt.
- genau eine Definition von `JavaStage` und `JavaProgress`; beide liegen in `java_progress.rs`.
- keine Definition oder Verwendung von `resolve_java` mehr vorhanden.
- Clippy-Korrektur `is_multiple_of(25)` vorhanden.
- die geänderten Rust-Dateien entsprechen dem von `rustfmt` erwartbaren Formatbild.

Die erfolgreichen Läufe von `cargo fmt`, `cargo check`, Clippy und Rust-Tests sind bereitgestellte Windows-Ergebnisse und wurden in dieser Umgebung nicht erneut mit Cargo ausgeführt.

### npm-Skriptfreigabe

In `package.json` ist ausschließlich folgende Freigabe vorhanden:

```json
"allowScripts": {
  "esbuild@0.25.12": true
}
```

`package-lock.json` löst `esbuild` exakt auf Version `0.25.12` auf.

## 4. Zusätzlich durchgeführte lokale Phase-0-Korrekturen

### Windows-CI erweitert

`.github/workflows/phase0-npm-test-windows.yml` führt künftig auf `windows-latest` aus:

1. `npm ci`
2. `npm test`
3. TypeScript-Prüfung und Vite-Produktionsbuild über `npm run build`
4. `cargo fmt --all -- --check`
5. `cargo check --locked`
6. `cargo clippy --locked --all-targets -- -D warnings`
7. `cargo test --locked`

Der Workflow besitzt nur Leserechte, lädt keine Signierschlüssel und veröffentlicht keine Artefakte. Die YAML-Struktur wurde lokal geparst. Ein realer GitHub-Actions-Lauf steht bis zum Einchecken des Workflows aus.

### Externen CSS-Laufzeitimport entfernt

`src/styles.css` enthielt entgegen dem bisherigen Phase-0-Bericht noch einen Google-Fonts-`@import`. Dieser Import wurde entfernt. Der lokale System-Schriftstack bleibt bestehen.

`check-security-config.mjs` lehnt nun externe HTTP-/HTTPS-CSS-Laufzeitimporte im aktiven Quellbereich ab, damit dieser Zustand nicht unbemerkt zurückkehrt.

### Bundlekonfiguration eindeutig gemacht

Die widersprüchliche generische MSI-/NSIS-Zielangabe wurde entfernt. Die Windows-Plattformkonfiguration ist nun die einzige maßgebliche Quelle für das Windows-Bundleziel und enthält ausschließlich:

```json
"targets": ["nsis"]
```

Die automatische Sicherheitsprüfung kontrolliert diese Eindeutigkeit.

## 5. Fehlende MSI-Ausgabe – Ursache und Entscheidung

### Ursache

Tauri lädt auf Windows automatisch `tauri.windows.conf.json` und führt diese Konfiguration nach der Hauptkonfiguration zusammen. Die Zusammenführung folgt JSON Merge Patch. Das Array

```json
"targets": ["nsis"]
```

in der Windows-Konfiguration ersetzt deshalb die zuvor in `tauri.conf.json` definierte Liste `['msi', 'nsis']`. Der beobachtete reine NSIS-Build entspricht damit der tatsächlich wirksamen Konfiguration und ist kein Nachweis für einen erfolgreichen MSI-Build.

### Entscheidung für Phase 0

MSI wird **vorerst bewusst nicht repariert**, sondern aus der widersprüchlichen Hauptkonfiguration entfernt.

Begründung:

- Nur NSIS wurde unter Windows tatsächlich erfolgreich erzeugt.
- MSI wurde weder erfolgreich gebaut noch als Releaseanforderung beschlossen.
- Ein zweites Installerformat erweitert Test-, Signatur-, Update- und Supportaufwand.
- Phase 0 dient der sicheren, reproduzierbaren Quellbasis und nicht der Erweiterung von Distributionsformaten.
- Die vorhandene NSIS-Konfiguration enthält bereits die aktuell getesteten Installationsoptionen.

Ein MSI-Ziel kann später separat freigegeben werden. Dafür sind ein expliziter Windows-CI-Build, WiX-spezifische Tests, Authenticode-Signierung, Installations-/Upgrade-/Deinstallationsprüfungen und eine klare Produktanforderung erforderlich.

Es wird ausdrücklich **kein erfolgreicher MSI-Build behauptet**.

## 6. Prüfungen

### In dieser Arbeitsumgebung selbst ausgeführt

| Prüfung | Ergebnis |
|---|---|
| ZIP-Integrität des angelieferten Pakets | Erfolgreich |
| Archivpfad- und Ausschlussprüfung | Erfolgreich; `src-tauri/gen` als generiert erkannt und final entfernt |
| Vergleich v1.1.1 → v1.1.2 | Erfolgreich; genannte Quelländerungen bestätigt |
| JSON-Parsing von Tauri-, npm- und Lock-Konfigurationen | Erfolgreich |
| YAML-Parsing beider GitHub-Actions-Workflows | Erfolgreich |
| `npm test` nach allen lokalen Korrekturen | Erfolgreich |
| UTF-8-/Mojibake-Prüfung | Erfolgreich |
| Secret-Prüfung | Erfolgreich |
| Sicherheitskonfigurationsprüfung | Erfolgreich |
| Versionssynchronisierung | Erfolgreich; Version `1.0.8` |
| IPC-Vertragsprüfung | Erfolgreich; 19 zentrale Commands |
| statische Rust-Modul-/Assetprüfung | Erfolgreich; 20 Rust-Dateien |
| Prüfung auf externe CSS-Laufzeitimporte | Erfolgreich; keine Treffer |
| Prüfung der eindeutigen NSIS-Konfiguration | Erfolgreich |

Ein erneutes `npm ci` konnte in dieser isolierten Ausführungsumgebung wegen fehlender DNS-/Registry-Erreichbarkeit (`EAI_AGAIN`) nicht abgeschlossen werden. Deshalb wurden der TypeScript-/Vite-Build, Cargo-Prüfungen und ein Tauri-Bundle nach den letzten Dokumentations- und Konfigurationskorrekturen hier nicht erneut ausgeführt.

### Vom Projektinhaber bereitgestellte echte Windows-Ergebnisse

Folgende Ergebnisse wurden ausdrücklich als auf dem verbindlichen v1.1.2-Quellstand erfolgreich durchgeführt bereitgestellt und werden als **bereitgestellte Windows-Ergebnisse**, nicht als eigene Ausführung, dokumentiert:

- `npm ci`
- `npm test`
- UTF-8-, Secret-, Sicherheits-, Versions-, IPC- und statische Rust-Prüfungen
- TypeScript-Prüfung
- Vite-Produktionsbuild mit 1.603 Modulen
- `cargo fmt --all -- --check`
- `cargo check --locked`
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo test --locked`
- alle drei Rust-Tests bestanden
- vollständiges `BUILD-WINDOWS.ps1` erfolgreich
- Tauri-Release-Build erfolgreich
- Release-EXE erfolgreich erzeugt
- NSIS-Installer erfolgreich erzeugt

Nicht bereitgestellt und nicht behauptet:

- erfolgreicher MSI-Build
- gültige Authenticode-Signatur
- veröffentlichungsfähiger Installer
- erfolgreicher GitHub-Actions-Lauf des aktualisierten Workflows

## 7. Verbleibende externe Aufgaben

1. Kompromittierte Schlüssel aus allen produktiven Systemen, Repositories, CI-Secrets und normalen Backups entfernen und extern widerrufen.
2. Falls für die Sicherheitsuntersuchung notwendig, höchstens eine streng isolierte, verschlüsselte, zugriffsgeschützte und auditierte Beweiskopie behalten.
3. Neuen Offline-Root- und delegierten Release-Vertrauensanker in einer geschützten Signierinfrastruktur einrichten.
4. Windows-Code-Signing-Zertifikat und vertrauenswürdiges Timestamping einrichten.
5. Launcher-EXE, NSIS-Installer, Updater-Helfer und Uninstaller vor jeder Verteilung mit Authenticode signieren und prüfen.
6. Manuellen Sicherheits-Vertrauenswechsel für bereits installierte Launcher planen und freigeben.
7. Offizielle HTTPS-Backend- und Update-Domains bereitstellen.
8. Aktualisierten Windows-CI-Workflow ausführen und die Ergebnisse dauerhaft dokumentieren.
9. Den finalen Quellstand erneut unter Windows mit `BUILD-WINDOWS.ps1 -Bundle` prüfen, ohne das unsignierte Ergebnis zu veröffentlichen.

## 8. Verbleibende Risiken

- Bereits installierte Altversionen können weiterhin dem kompromittierten alten Schlüssel vertrauen.
- Es existiert noch keine produktive neue Signatur- und Widerrufskette.
- Der erzeugte NSIS-Installer und die Release-EXE sind unsigniert.
- Die letzten lokalen Änderungen an Workflow, CSS-Prüfung und Bundleziel wurden hier nicht durch einen echten Windows-/Tauri-Bundlelauf validiert.
- Der S9Lab Client ist weiterhin lokal gebündelt. Der minimale sichere und signierte Komponentenresolver ist erst als Voraussetzung von Phase 5 geplant.
- MSI ist bewusst nicht Bestandteil des aktuellen Phase-0-Buildumfangs.

## 9. Freigabezustand

Der Stand ist als **finales Phase-0-v1.1.2-Quellpaket zur Prüfung** geeignet. Er ist nicht als öffentlicher Release, signierter Sicherheitsinstaller oder Nutzerdistribution freigegeben.

Phase 1 wurde nicht begonnen und darf erst nach ausdrücklicher Freigabe gestartet werden.
