# Phase 7 – Sicherheit

Status: lokaler Checkpointstand vom 2026-08-11.

## Vertrauensmodell

- Launcher- und S9Lab-Client-Updates bleiben fail-closed, solange freigegebene
  Produktionsursprünge, öffentlicher Signaturanker und Releaseverfahren fehlen.
- Die Richtlinie weist Automatik für einen gesperrten Kanal ab.
- Inhaltsupdates verwenden ausschließlich den kontrollierten Phase-6-Provider:
  HTTPS, Host- und Identitätsbindung, Größenlimit, Upstream-SHA-512 und interner
  SHA-256 vor Aktivierung.
- Kein privater Produktionsschlüssel wird erzeugt, gespeichert oder in Tests
  ersetzt.

## Backup- und Pfadsicherheit

Wiederherstellungspunkte verwenden ausschließlich registrierte `profiles`- und
`backups`-Wurzeln. Für Inventarisierung und Kopie bleiben Traversal-, Symlink-,
Junction-, Reparse-Point-, ADS-, Sondernamen-, Separator-, Unicode-, Case- und
Hardlinkschutz aktiv.

Grenzen:

- höchstens 32.768 veränderliche Dateien,
- höchstens 2 GiB je Datei,
- höchstens 8 GiB Gesamtdaten und
- höchstens 16 MiB für das Backup-Manifest.

Der Sicherungspunkt wird zunächst unter einer internen Staging-ID erstellt und
erst nach Kopie, Größen- und SHA-256-Prüfung atomar aktiviert. Bei Fehlern werden
Staging, Ziel und Cache-Referenz bereinigt.

Beim Restore müssen Manifest und tatsächlicher Baum dieselbe vollständige
Dateimenge mit denselben Größen und Hashes besitzen. Zielpfade werden erneut über
die Registry aufgelöst. Anschließend wird jede im neuen Profil angelegte Datei
noch einmal gegen den erwarteten Hash geprüft. Das bindet die aktive Kopie auch
an Änderungen, die nach der ersten Inventarprüfung auftreten könnten.

## Datenminimierung

Das Backup enthält keine OAuth-Tokens, Refresh-Tokens, Passwörter, Logs,
Crashreports oder privaten Schlüssel. Eine optionale Kontoübernahme speichert
nur die bereits vorhandene interne Kontozuordnung des Profils; Geheimnisse
bleiben im Betriebssystem-Credential-Store. Die übernommenen Einstellungen sind
auf `ShellSettings` begrenzt.

`.s9lab`-Projektionsmarker und unveränderliche projizierte Mods, Ressourcen- und
Shaderpakete werden nicht als veränderliche Dateien kopiert. Sie werden aus der
verifizierten Revision und dem Cache neu materialisiert. Benutzerveränderte
Welten, Konfigurationen und Override-Ziele bleiben dagegen unter lokaler
Datenhoheit.

## Atomarität und Recovery

Vor Updates und manuellen Rollbacks wird automatisch ein Sicherungspunkt
erstellt. Ein fehlgeschlagenes Update behält oder rekonstruiert die zuvor aktive
Revision über den vorhandenen Operationskern. Ein Restore überschreibt nie das
Quellprofil, sondern aktiviert erst nach erfolgreicher Vorbereitung eine neue
Profilidentität. Fehler bei der Einstellungsübernahme lösen eine Rückkehr zur
vorherigen Einstellungsdatei aus.

Authenticode ist kein lokaler Ersatz für diese Datenatomarität. Das finale
Authenticode-Gate bleibt bis zu einem extern signierten Release Candidate offen.
