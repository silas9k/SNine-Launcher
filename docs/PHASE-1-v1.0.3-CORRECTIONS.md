# Phase 1 v1.0.3 – Korrekturliste

## Ausgangsfehler

Eine unter Windows erfolgreich erzeugte und als Junction verifizierte Struktur wurde abgelehnt, erhielt jedoch den allgemeinen Fehler `path_symlink_forbidden`. Dokumentiert und im Regressionstest erwartet war `path_reparse_point_forbidden`.

Ursache war die Reihenfolge in `validate_existing_entry`: `file_type().is_symlink()` wurde vor dem Windows-Reparse-Point-Attribut geprüft. Die Windows-Metadaten der Junction erfüllten dadurch bereits die allgemeinere Prüfung.

## Codekorrekturen

### `src-tauri/src/security/paths.rs`

- Reparse-Point-Erkennung vor die allgemeine Symlink-Erkennung verschoben.
- Verbindliche Klassifizierung dokumentiert:
  - Windows-Junction und Windows-Symlink: `path_reparse_point_forbidden`
  - Nicht-Windows-Symlink: `path_symlink_forbidden`
  - Hardlink: `path_hardlink_forbidden`
- Junction-Fixture in `VerifiedWindowsJunctionFixture` zentralisiert.
- Neuer Regressionstest `classifies_verified_windows_junctions_with_the_stable_reparse_error` ergänzt.
- Bestehender Registry-Test behält die exakte Prüfung auf `path_reparse_point_forbidden`.
- Fixture-Prüfung bleibt strikt; kein Überspringen bei fehlgeschlagener Einrichtung.
- Cleanup prüft weiterhin, dass der Zielinhalt nach Entfernen der Junction existiert.

### `scripts/check-phase1-foundation.mjs`

- neuen Regressionstest als Pflichtmerkmal ergänzt,
- statische Kontrolle ergänzt, dass die Reparse-Point-Prüfung vor der Symlink-Prüfung steht,
- neuen Test als Pflichtschritt im Windows-Workflow ergänzt.

### `.github/workflows/phase1-windows-verification.yml`

- direkten stabilen Klassifikationstest vor dem Registry-Junction-Test ergänzt,
- Berechtigungen, Signierung und Veröffentlichungsverhalten unverändert gelassen.

### `VERIFY-PHASE1-WINDOWS.ps1`

- neuen Junction-Klassifikationstest ergänzt,
- Bezeichnung auf v1.0.3 aktualisiert.

## Dokumentationskorrekturen

- `docs/PHASE-1-PATH-SECURITY.md`: stabile Plattformklassifizierung und Begründung ergänzt.
- `docs/PHASE-1-IPC-CONTRACTS.md`: Pfadfehlercodes dokumentiert.
- `docs/PHASE-1-TEST-MATRIX.md`: neue Regression ergänzt und Korrekturstand aktualisiert.
- `docs/PHASE-1-DEMO.md`: Windows-Klassifikationstest ergänzt.
- `docs/PHASE-1-COMPLETION-REPORT.md`: Bericht auf v1.0.3 aktualisiert.
- `docs/PHASE-1-v1.0.3-CORRECTIONS.md`: diese Korrekturliste ergänzt.

## Unverändert

- Produktversion 1.0.8,
- Hardlink-Erkennung und Fehlercode,
- Pfadbudget und Recovery-Logik,
- Download-, SQLite-, Cache- und IPC-Funktionsumfang,
- Sicherheitskonfiguration,
- Release- und Signierungsstatus,
- Sperre von Phase 2.
