# Phase 3: SQLite-Migration 4

Die Schema-Version steigt ausschließlich von 3 auf 4. Die Migration ist monoton, transaktional und wird über die bestehende `schema_migrations`-Tabelle protokolliert.

## Neue Tabellen

- `accounts`: verifizierte, nicht geheime Account-Metadaten, Sessionzustand und opaker `vault_ref`.
- `launcher_account_selection`: genau eine aktive Launcher-Auswahl.
- `profile_account_assignments`: optionale, referenziell abgesicherte Account-Zuordnung je Profil.

Die Tabellen enthalten ausdrücklich keine Passwörter, Device Codes oder Access-, Refresh-, Xbox-, XSTS- beziehungsweise Minecraft-Tokens. Löschungen eines Accounts entfernen seine Auswahl und Profilzuordnungen über Fremdschlüsselregeln.

## Legacy-Übernahme

Die alte `accounts.json` wird erst nach erfolgreicher SQLite-Übernahme in `accounts.phase3-migrated.json` umbenannt. Unverifizierte Altsitzungen werden nicht in den aktiven Phase-3-Vault übernommen. Stattdessen wird ein rückwärtskompatibler Metadatensatz mit `relogin-required` angelegt und der alte Credential-Eintrag nach erfolgreicher Übernahme entfernt.

## Fehler- und Recovery-Verhalten

Ein neuer Credential-Eintrag wird vor dem SQLite-Verweis erstellt. Scheitert SQLite, wird der neue Eintrag entfernt. Scheitert bei einer Rotation das Entfernen des alten Eintrags, werden SQLite-Verweis und Metadaten auf den vorherigen Stand zurückgesetzt und das neue Secret entfernt. Scheitern Primär- und Kompensationsschritt, entsteht ein stabil typisierter zusammengesetzter Fehler; kein Fehler wird still unterdrückt.

