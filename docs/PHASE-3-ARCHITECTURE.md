# Phase 3: Microsoft-Authentifizierung und Account-Grundlage

## Umfang

Phase 3 implementiert den Microsoft-Device-Code-Flow, den Xbox-/XSTS-Austausch, den Minecraft-Login, eine ausdrückliche Java-Entitlement-Prüfung, die Minecraft-Profilauflösung sowie Auswahl, Erneuerung und lokale Abmeldung mehrerer Accounts. Phase 4 oder spätere Installations-, Freunde-, Shop-, Coins- und Community-Funktionen gehören nicht zu diesem Stand.

## Vertrauensgrenzen

- React erhält nur `loginId`, `userCode`, die geprüfte Microsoft-HTTPS-URL und Ablauf-/Polling-Metadaten.
- `device_code`, Microsoft-Tokens, Xbox-/XSTS-Tokens und Minecraft-Tokens verbleiben im Rust-Prozess.
- Geheimnisse liegen ausschließlich im betriebssystemeigenen Credential Store. SQLite speichert nur einen opaken `vault_ref` und nicht geheime Metadaten.
- Jede erfolgreiche neue oder erneuerte Sitzung durchläuft die Entitlement- und Profilprüfung erneut.
- Die öffentliche IPC-Oberfläche ist als Vertrag Version 3 definiert und erzeugt die TypeScript-Typen deterministisch.

## Komponenten

`auth::microsoft` kapselt ausschließlich HTTPS-Aufrufe, Polling, Abbruch, Token-Austausch und Besitzprüfung. `auth::service` verwaltet nur im Rust-Speicher liegende, begrenzte Pending-Logins und stellt typsichere Account-Operationen bereit. `auth::store` koordiniert SQLite-Metadaten und den OS Credential Store mit kompensierenden Rollbacks. `storage` besitzt die versionierte Migration und alle Transaktionen. React arbeitet nur über `authCommands.ts` und die generierten IPC-Verträge.

## Offline-Verhalten

Die maximal zulässige Offline-Dauer ist im verbindlichen Plan noch nicht festgelegt. Phase 3 erfindet deshalb keinen Wert: `policy=unconfigured`, `eligible=false`, `reason=offline_policy_unconfigured`. Offline-Starts bleiben fail-closed, bis eine spätere verbindliche Produktentscheidung vorliegt.

## Rückwärtskompatibilität

Alte Account-Metadaten werden nach SQLite übernommen. Alte, noch nicht durch die neue explizite Besitzprüfung bestätigte Sitzungen werden absichtlich nicht als aktiv vertraut; der Account erhält den Zustand `relogin-required`, und vorhandene Legacy-Credentials werden nach erfolgreicher Metadatenübernahme entfernt. Bestehende Phase-0-, Phase-1- und Phase-2-Daten bleiben bestehen.

