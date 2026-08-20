# Phase 1 – Pfad- und Dateisicherheitsmodell

## Registrierte Wurzeln

Produktive Schreibzugriffe werden über `PathRegistry` auf folgende kontrollierte Bereiche begrenzt:

- `data`
- `profiles`
- `cache`
- `cache/blobs/sha256`
- `staging/operations`
- `migration`
- `backups`
- `logs/launcher`

Die registrierten Wurzeln werden aus der vom Plattformadapter gelieferten, absolut aufgelösten Launcher-Wurzel abgeleitet. Technische Tests verwenden injizierte temporäre Wurzeln und schreiben nicht in reale Nutzerdaten.

## Pfadnormalisierung

Abgelehnt werden unter anderem:

- absolute Pfade und Windows-Präfixe,
- `..`-Segmente,
- leere oder mehrdeutige Segmente,
- Nullzeichen und Steuerzeichen,
- Alternate Data Streams über `:`,
- reservierte Windows-Namen,
- abschließende Punkte oder Leerzeichen,
- verbotene Windows-Zeichen,
- überlange Komponenten,
- Case- und Unicode-normalisierte Kollisionen,
- bestehende Symlinks, Hardlinks, Junctions und andere Reparse Points.

Zwei normalisierte Ziele mit demselben Kollisionsschlüssel werden abgelehnt. Es gilt niemals „letzte Datei gewinnt“.

## Dynamisches Pfadlängenmodell

Bis sämtliche beteiligten Windows-Komponenten nachweislich long-path-fähig sind, gilt ein konservatives sichtbares absolutes Maximum von 247 UTF-16-Einheiten.

```text
verfügbar relativ = min(220, 247 - UTF16(registrierte Wurzel) - Trennzeichen)
```

Unter Windows werden Pfade in UTF-16-Einheiten gemessen. Das Budget wird für jede tatsächlich registrierte Wurzel separat berechnet. Ein längerer Benutzerpfad reduziert daher unmittelbar das verfügbare Profil- oder Stagingbudget.

Die Testinjektion eines abweichenden absoluten Limits ist ausschließlich unter `#[cfg(test)]` verfügbar. Der Produktionspfad verwendet immer das normale Sicherheitslimit.

## Operations-Preflight

`OperationEngine::validate_plan_paths` läuft vor:

- dem Einfügen des Operationsdatensatzes,
- dem ersten Journaleintrag,
- dem Erstellen eines operationsspezifischen Staging-Verzeichnisses,
- jedem Manifest-, Lock- oder Payload-Schreibvorgang.

Geprüft werden alle abgeleiteten Pfade.

### Profile

```text
<profile-id>
<profile-id>/revisions
<profile-id>/revisions/<revision-id>
<profile-id>/revisions/<revision-id>/manifest.json
<profile-id>/revisions/<revision-id>/lock.json
<profile-id>/revisions/<revision-id>/<payload>
```

### Staging

```text
<operation-id>
<operation-id>/revision
<operation-id>/revision/manifest.json
<operation-id>/revision/lock.json
<operation-id>/revision/<payload>
```

Der längste konkrete absolute Pfad bestimmt, ob der Plan zulässig ist. Ein Fehler erzeugt `path_too_long` mit Wurzel-, Relativ- und Absolutlängen sowie den geltenden Grenzen.

## Reales Phase-1-Budget

| Bestandteil | UTF-16-Einheiten |
|---|---:|
| Profil-ID | 40 |
| Revisions-ID | 36 |
| Operations-ID | 35 |
| Demo-Payload | 29 |
| tiefster relativer Profilpfad | 117 |
| tiefster relativer Stagingpfad | 74 |

Die absolute Länge ergibt sich jeweils aus der echten registrierten Wurzel, einem gegebenenfalls erforderlichen Trennzeichen und dem relativen Ziel.

## Keine Nebenwirkungen bei Preflight-Fehlern

Der deterministische Regressionstest setzt die erlaubte Grenze anhand der tatsächlich abgeleiteten Pfade. Eine zweite Instanz erhält exakt eine UTF-16-Einheit weniger Budget. Bei deren Ablehnung wird ausdrücklich geprüft:

- `operations` enthält keinen Datensatz,
- `operation_journal` enthält keinen Datensatz,
- `staging/operations` enthält kein operationsspezifisches Verzeichnis.

## Hardlinks

Bestehende Dateien mit einer Linkanzahl größer als eins werden abgelehnt. Der Test muss einen echten Hardlink erzeugen und die Linkanzahl nachweisen, bevor die Ablehnung geprüft wird.

Veränderliche Profildateien werden niemals über Hardlinks aus dem Cache bereitgestellt.

## Windows-Junctions

Der Windows-Test erstellt eine echte Junction ohne Administratorrechte:

```powershell
New-Item -ItemType Junction -Path $link -Target $target -ErrorAction Stop
```

Link und Ziel werden über Umgebungsvariablen übergeben. Die Einrichtung wird nur akzeptiert, wenn:

- Link und Ziel existieren,
- das ReparsePoint-Attribut vorhanden ist,
- `LinkType`, sofern verfügbar, `Junction` lautet,
- Rust den Reparse Point ebenfalls erkennt.

Danach muss die Registry deterministisch `path_reparse_point_forbidden` liefern. Die Klassifizierungsreihenfolge ist verbindlich:

1. Unter Windows werden Reparse Points zuerst erkannt. Junctions und Windows-Symlinks erhalten dadurch den stabilen Code `path_reparse_point_forbidden`.
2. Auf Plattformen ohne Windows-Reparse-Point-Semantik werden Symlinks als `path_symlink_forbidden` klassifiziert.
3. Hardlinks bleiben davon getrennt und liefern ausschließlich `path_hardlink_forbidden`.

Diese gemeinsame Windows-Klassifizierung ist beabsichtigt, weil sowohl Junctions als auch Windows-Symlinks auf Reparse-Point-Metadaten beruhen. Sie verhindert, dass dieselbe Junction abhängig von der Reihenfolge der allgemeinen Symlinkprüfung wechselnde Fehlercodes erzeugt. Zwei Windows-Regressionstests prüfen sowohl die direkte Metadatenklassifizierung als auch die Ablehnung über die Registry.

Bei der Bereinigung wird nur die Junction entfernt. Eine Markerdatei beweist anschließend, dass der Zielinhalt unverändert blieb.

## Restrisiko

Die Phase-1-Prüfungen verhindern bekannte lexikalische und bestehende Linkangriffe. Eine spätere zusätzliche Härtung kann Windows-Handle-basierte Operationen einsetzen, um privilegierte Race Conditions zwischen Prüfung und Zugriff weiter zu reduzieren.
