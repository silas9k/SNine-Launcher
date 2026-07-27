# S9Lab Phase 1 v1.0.1 – Korrekturen

## Anlass

Die unabhängige Windows-Prüfung des Phase-1-v1.0-Pakets ergab zwei konkrete Probleme:

1. Der ausgelieferte Rust-Quellstand war nicht vollständig mit `cargo fmt` formatiert.
2. Der Recovery-Test wurde unter einem normalen längeren Windows-TEMP-Pfad durch die eigene feste Pfadgrenze abgewiesen (`absoluteLength: 241`, `relativeLength: 117`). Mit einem kürzeren TEMP-Stamm funktionierte die Recovery-Logik.

Zusätzlich gab der alte Junction-Test bei fehlgeschlagener Fixture-Erstellung eine Windows-Fehlermeldung aus und konnte anschließend dennoch als bestanden gelten.

## Verbindliche Korrekturen

### Vollständige Rustfmt-Formatierung

Alle Rust-Dateien wurden mit Rustfmt 1.88.0 formatiert. Der finale Quellstand besteht `cargo fmt --all -- --check` in der hier verfügbaren Toolchain.

### Dynamisches und konservatives Windows-Pfadbudget

Die frühere starre Grenze von 240 UTF-16-Einheiten wurde ersetzt durch:

- maximales einzelnes Pfadsegment: 255 UTF-16-Einheiten,
- projektinterne relative Obergrenze: 220 UTF-16-Einheiten,
- konservative absolute Obergrenze: 247 sichtbare UTF-16-Einheiten,
- dynamisch je registrierter Wurzel berechnetes relatives Restbudget.

Berechnung:

```text
verfügbares relatives Budget = min(220, 247 - Länge(Wurzel) - Trennzeichen)
```

Die Grenze von 247 vermeidet eine Abhängigkeit von nicht nachgewiesener Windows-Long-Path-Unterstützung und lässt zugleich den gemeldeten normalen TEMP-Fall mit einer absoluten Länge von 241 zu.

Das tatsächliche Phase-1-Pfadmodell wird explizit geprüft:

| Bestandteil | UTF-16-Länge |
|---|---:|
| Profil-ID `profile-<32 hex>` | 40 |
| Revisions-ID `rev-<32 hex>` | 36 |
| Operations-ID `op-<32 hex>` | 35 |
| Demo-Zieldatei `instance/phase1-installed.txt` | 29 |
| tiefster relativer Profilpfad | 117 |
| tiefster relativer Stagingpfad | 74 |

Ein Installationsplan validiert seine vollständigen Staging- und Zielpfade, bevor ein Operationsdatensatz oder Journal angelegt wird. Ein zu langer Pfad erzeugt somit keinen teilweise geplanten Vorgang.

### Getrennte Grenztests

Neu beziehungsweise präzisiert:

- exakt verfügbares relatives Budget wird akzeptiert,
- eine UTF-16-Einheit darüber wird abgelehnt,
- der registrierte Wurzelpfad wird in das absolute Budget eingerechnet,
- Komponenten- und relative Limits werden getrennt geprüft,
- das echte generierte Profil-/Revisions-/Operationsmodell wird geprüft,
- ein vollständiger Operationsplan an der absoluten Grenze wird akzeptiert,
- derselbe Plan eine Einheit darüber wird vor dem Journaling abgelehnt.

Der Name des Recovery-Tests wurde nicht verkürzt. Der Test verwendet weiterhin den normalen temporären Systempfad.

### Hardlink-Test

Der Test:

1. erstellt den Hardlink zwingend mit `fs::hard_link(...).expect(...)`,
2. liest anschließend die reale Linkanzahl,
3. verlangt eine Linkanzahl größer als eins,
4. prüft für beide Dateinamen exakt den Fehler `path_hardlink_forbidden`.

Eine fehlgeschlagene Fixture-Erstellung kann nicht übersprungen werden.

### Junction-Test

Die Untersuchung des alten Tests zeigte:

- Erstellung über `cmd /C mklink /J`,
- Ausgabe direkt an die Konsole,
- bedingte Sicherheitsprüfung nur bei erfolgreichem Exitstatus.

Dadurch konnte die gemeldete Windows-Fehlermeldung erscheinen und der Test trotzdem ohne Fehler enden.

Der neue Test:

1. erstellt die Junction über nicht interaktives Windows PowerShell und `New-Item -ItemType Junction`,
2. übergibt Pfade als Umgebungsvariablen statt über selbst zusammengesetzte Shell-Quoting-Strings,
3. erfasst Standardausgabe und Fehlerausgabe,
4. verlangt zwingend einen erfolgreichen Prozessstatus,
5. prüft über Dateimetadaten, dass tatsächlich ein Reparse Point angelegt wurde,
6. erwartet danach exakt `path_reparse_point_forbidden`,
7. schlägt bei jeder fehlerhaften Testvorbereitung fehl.

### Windows-CI

Der Workflow verwendet eine festgelegte Rust-Toolchain 1.88.0 und führt weiterhin den vollständigen Testlauf aus. Zusätzlich werden die Recovery-, Hardlink-, Junction- und Pfadgrenzregressionen auf `windows-latest` einzeln ausgeführt. Der Workflow besitzt nur `contents: read`, verwendet keine Signierschlüssel und veröffentlicht keine Artefakte.

## Unverändert

- öffentliche Produktversion: `1.0.8`,
- Funktionsumfang von Phase 1,
- keine Phase-2-Arbeiten,
- kein Remote-Updater,
- keine Schlüssel oder Secrets,
- keine Veröffentlichung oder produktive Änderung.
