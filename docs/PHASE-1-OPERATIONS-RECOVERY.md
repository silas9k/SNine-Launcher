# Phase 1 v1.0.1 – Operationszustandsmaschine und Recovery

## Zustände

```text
planned
→ staging
→ verifying
→ ready-to-commit
→ committing
→ validating
→ completed
```

Fehlerpfad:

```text
nicht terminaler Zustand
→ rolling-back
→ rolled-back
```

Nicht reparierbare inkonsistente Metadaten enden kontrolliert in `failed`.

## Planungs- und Pfad-Preflight

Vor dem Einfügen einer Operation in SQLite werden alle geplanten Ziele vollständig validiert:

- `staging/operations/<operation-id>/revision/manifest.json`
- `staging/operations/<operation-id>/revision/lock.json`
- alle Staging-Payloads
- `<profile-id>/revisions/<revision-id>/manifest.json`
- `<profile-id>/revisions/<revision-id>/lock.json`
- alle Profil-Payloads

Dabei gelten Normalisierung, Kollisionserkennung und das dynamische Pfadbudget der konkreten Wurzel. Ein Fehler erzeugt weder Operationsdatensatz noch Journal.

## Staging

Alle Inhalte werden zuerst ausschließlich unter der registrierten Wurzel `staging-operations` vorbereitet. Neue Dateien werden ohne Überschreiben angelegt. Danach werden Manifest-, Lock- und Payload-Hashes geprüft.

## Commit

Nach `ready-to-commit` wird die vollständig verifizierte Revisionsstruktur in das Profil verschoben. Erst danach aktiviert eine SQLite-Transaktion gemeinsam:

- Revisionsdatensatz,
- Manifest-Hash,
- Lock-Hash,
- aktiven Revisionszeiger.

Manifest, Lockdatei und SQLite-Revision werden nicht unabhängig voneinander als aktiv freigegeben.

## Journal und Gegenoperationen

Das Journal erfasst unter anderem:

- geplante Operation,
- geschriebenes Staging,
- erfolgreiche Prüfung,
- verschobene Revision,
- aktivierte Datenbankrevision,
- Zustandsübergänge,
- ausgeführte Kompensationen.

Gegenoperationen entfernen Staging beziehungsweise Zielrevision und stellen den vorherigen aktiven Revisionszeiger wieder her.

## Recovery beim nächsten Start

Beim Öffnen von `CoreServices` werden nicht terminale Operationen erkannt.

- `planned`, `staging`, `verifying`, `ready-to-commit`, `rolling-back`: vollständiger Rollback.
- `committing`: nur dann konsistenter Abschluss, wenn die neue Revision bereits aktiv und vollständig validierbar ist; andernfalls Rollback.
- `validating`: bei vollständig gültigem neuen Zustand Abschluss zu `completed`, andernfalls Rollback.
- beschädigter oder zum SQLite-Datensatz unpassender Plan: Zielaktivierung zurücknehmen, Zielrevision und Staging entfernen, Operation auf `failed` setzen.

Ergebnis ist immer entweder der vollständig alte oder der vollständig neue Zustand.

## Failure-Injection

Failure-Injection ist ausschließlich unter `#[cfg(test)]` verfügbar. Getestet werden:

- nach `planned`,
- nach `staging`,
- nach `verifying`,
- nach `ready-to-commit`,
- nach Verschieben der Revision,
- nach Datenbankaktivierung,
- während der Validierung.

Der Test `crash_recovery_never_leaves_a_mixed_revision` behält seinen vollständigen Namen und verwendet den normalen temporären Systempfad. Die v1.0.1-Korrektur repariert das Pfadmodell, statt den Testpfad künstlich zu verkürzen.
