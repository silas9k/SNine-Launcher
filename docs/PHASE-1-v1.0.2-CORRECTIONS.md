# S9Lab Phase 1 v1.0.2 – Windows-Korrekturen

**Produktversion:** 1.0.8, unverändert  
**Umfang:** ausschließlich Phase 1  
**Phase 2:** nicht begonnen

## Ausgangsbefund

Die unabhängige Windows-Prüfung von v1.0.1 bestand Frontend, Rustfmt, `cargo check`, Clippy und den Recovery-Test. Von 35 Windows-Rust-Tests bestanden 33. Fehlgeschlagen waren:

1. `operation_plan_preflight_enforces_the_real_root_budget_before_journaling`
2. `rejects_windows_directory_junctions_after_verified_fixture_creation`

## 1. Deterministisches Operations-Pfadbudget

Der frühere Grenztest leitete die Länge des Launcher-Stamms aus `std::env::temp_dir()` und einer angenommenen Trennzeichenlänge ab. Ein TEMP-Pfad mit abschließendem Trennzeichen konnte dadurch den vorgesehenen Ein-Einheiten-Unterschied neutralisieren.

v1.0.2 berechnet die Grenze aus den tatsächlich aufgelösten registrierten Wurzeln:

- `profiles`
- `staging/operations`

Die Produktion verwendet weiterhin das dynamische Restbudget der echten Wurzel und die konservative absolute Grenze von 247 sichtbaren UTF-16-Einheiten. Nur Tests können über eine `#[cfg(test)]`-Schnittstelle ein kleineres absolutes Limit injizieren. Das Produktionsmodell wurde nicht gelockert.

Der deterministische Regressionstest:

- verwendet ein normales temporäres Stammverzeichnis,
- berechnet den längsten tatsächlich abgeleiteten absoluten Planpfad,
- akzeptiert exakt diese Grenze,
- verwendet für den Ablehnungsfall exakt eine UTF-16-Einheit weniger,
- ist dadurch unabhängig von der zufälligen Länge und einem möglichen abschließenden Trennzeichen des System-TEMP-Pfads.

## 2. Vollständiger Preflight vor SQLite und Dateisystem

Vor dem ersten Operationsdatensatz, Journal oder operationsspezifischen Staging-Verzeichnis werden nun alle abgeleiteten Pfade validiert.

### Profilwurzel

- Profil-ID
- `revisions`
- Revisions-ID
- `manifest.json`
- `lock.json`
- alle Payload-Ziele

### Stagingwurzel

- Operations-ID
- `revision`
- `manifest.json`
- `lock.json`
- alle Payload-Ziele

Die Validierung verwendet die tatsächlich im `PathRegistry` registrierte und absolut aufgelöste Wurzel. Unter Windows werden Pfadlängen als UTF-16-Einheiten berechnet.

Bei unzureichendem Budget gilt:

- stabiler Fehlercode `path_too_long`,
- kein Eintrag in `operations`,
- kein Eintrag in `operation_journal`,
- kein Verzeichnis unter `staging/operations/<id>`.

## 3. Reales Phase-1-Pfadmodell

| Bestandteil | UTF-16-Einheiten |
|---|---:|
| Profil-ID `profile-<32 hex>` | 40 |
| Revisions-ID `rev-<32 hex>` | 36 |
| Operations-ID `op-<32 hex>` | 35 |
| Payload `instance/phase1-installed.txt` | 29 |
| tiefster relativer Profilpfad | 117 |
| tiefster relativer Stagingpfad | 74 |

Für jede Benutzerwurzel wird das verfügbare relative Budget separat berechnet:

```text
min(220, 247 - UTF16(Wurzel) - erforderliches Trennzeichen)
```

Der Preflight prüft zusätzlich die konkrete absolute Länge jedes abgeleiteten Ziels. Sehr lange Benutzerpfade werden kontrolliert abgelehnt, statt erst während Staging oder Commit zu scheitern.

## 4. Junction-Fixture unter Windows

Die Junction wird jetzt mit einer von Windows PowerShell unterstützten Form erstellt:

```powershell
New-Item -ItemType Junction -Path $link -Target $target -ErrorAction Stop
```

Sicherheitsmerkmale:

- Link- und Zielpfad werden ausschließlich über Umgebungsvariablen übergeben.
- Es gibt keine Stringinterpolation der Pfade im PowerShell-Programm.
- Die Fixture-Erstellung muss einen erfolgreichen Exitstatus liefern.
- PowerShell liefert ein JSON-Prüfergebnis zurück.
- Der Link muss existieren.
- Das Ziel muss existieren.
- `Attributes` muss `ReparsePoint` enthalten.
- `LinkType` muss, sofern von der PowerShell-Version bereitgestellt, `Junction` sein.
- Rust bestätigt den Reparse Point zusätzlich über `symlink_metadata`.
- S9Lab muss den Pfad anschließend mit `path_reparse_point_forbidden` ablehnen.

Die Bereinigung entfernt zuerst ausschließlich die Junction mit `remove_dir`. Eine Markerdatei im Ziel muss danach weiterhin existieren. Erst nach dieser Prüfung wird das Zielverzeichnis separat entfernt.

Eine fehlerhafte Fixture-Erstellung führt immer zu einem fehlgeschlagenen Test. Administratorrechte und Windows Developer Mode werden nicht vorausgesetzt.

## 5. Windows-CI

Der read-only Workflow führt die beiden korrigierten Regressionen zuerst einzeln aus und danach den vollständigen Rust-Testlauf mit Ausgabe:

1. Operations-Preflight
2. Junction-Ablehnung
3. `cargo test --locked -- --nocapture`
4. weitere Recovery-, Hardlink- und Pfadgrenztests
5. technische Phase-1-Demo
6. erst danach den weiterhin unsignierten lokalen Tauri-Build

Der Workflow lädt keine Artefakte hoch, veröffentlicht nichts und verwendet keine Signierschlüssel.

## 6. Unverändert

- Produktversion 1.0.8
- Phase-1-Funktionsumfang
- konservative Produktionspfadgrenze
- Hardlink-Verbot für veränderliche Profildaten
- kein Remote-Updater
- keine Secrets oder Schlüssel
- keine Phase-2-Arbeiten
