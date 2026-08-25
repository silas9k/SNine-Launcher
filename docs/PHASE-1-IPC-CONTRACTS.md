# Phase 1 – IPC-Vertragsübersicht

## Gemeinsame Quelle

`contracts/ipc-contracts.json` ist die maschinenlesbare Phase-1-Vertragsquelle. `scripts/generate-ipc-contracts.mjs` erzeugt daraus die TypeScript-Typen unter `src/lib/generated/ipc-contracts.ts`.

`npm test` prüft:

- dass die generierte Datei aktuell ist,
- dass jeder Frontend-Command ein registriertes Rust-Gegenstück besitzt,
- dass der Rust-Test Name, Contract-Version, Eingabe-, Ausgabe- und Fehlerfelder mit derselben JSON-Datei abgleicht.

## Phase-1-Command

### `phase1_core_status`

Eingabe: keine Felder.

Ausgabe:

- `schemaVersion`
- `platform`
- `registeredRoots`
- `incompleteOperations`
- `startupRecoveredOperations`

Fehler:

- `code`
- `messageKey`
- `params`

## Berechtigung

Der Command akzeptiert ausschließlich das Tauri-Hauptfenster mit Label `main`. Er ist read-only und nimmt keine Pfade entgegen.

## Nicht ausgeliefert

Nicht über IPC erreichbar sind:

- Failure-Injection,
- simulierte Abstürze,
- technische Profilanlage,
- direkte Datenbankmanipulation,
- freie Downloads,
- freie Dateisystempfade.

Die technische Demo läuft als Rust-Test.
## Stabile Pfadfehlercodes

Die Phase-1-Fehlerdeskriptoren verwenden für Dateisystemlinks folgende stabile Klassifizierung:

- `path_reparse_point_forbidden`: Windows-Junctions und Windows-Symlinks, da beide als Reparse Points erkannt werden.
- `path_symlink_forbidden`: Symlinks auf Plattformen ohne Windows-Reparse-Point-Klassifizierung.
- `path_hardlink_forbidden`: Dateien mit nachgewiesener Hardlink-Anzahl größer als eins.

Die Windows-Reparse-Point-Prüfung läuft vor der allgemeinen Symlink-Prüfung. Ein verifizierter Junction-Pfad darf daher niemals als `path_symlink_forbidden` erscheinen.

