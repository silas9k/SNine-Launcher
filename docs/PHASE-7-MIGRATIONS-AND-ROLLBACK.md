# Phase 7 – Migration und Rollback

Status: lokaler Checkpointstand vom 2026-08-11.

## Persistenzänderungen

Phase 7 benötigt keine SQLite-Schemaänderung; Schema v6 bleibt unverändert.
Neue Daten besitzen eigene versionierte Dateiautoritäten:

- `data/update-policy.json`, Formatversion 1,
- `backups/<Backup-ID>/backup.json`, Format
  `site.s9lab.restore-point`, Version 1.

Unbekannte Policy- oder Backupfelder, nicht unterstützte Formatversionen,
unkanonische Hashes und ungültige Pfade werden abgewiesen. Die Policy wird über
eine temporäre Datei geschrieben und atomar ersetzt.

## Update-Rollback

1. Aktive Basisrevision und Änderungsvorschau bestimmen.
2. Wiederherstellungspunkt für die Basisrevision anlegen.
3. Auswahl unmittelbar vor Anwendung erneut prüfen.
4. Änderungen als neue Revisionen ausführen.
5. Bei Fehler die Basis als neue, vollständig hashgebundene Revision klonen.

Damit bleibt die Revisionshistorie append-only. Ein Rollback verändert keine
bereits festgeschriebene Revision und verwendet denselben journalisierten
Operationspfad wie Installation und Reparatur.

## Manueller Rollback

Die Revisionszeitleiste bietet nur festgeschriebene Revisionen desselben aktiven
Profils an. Vor einem bestätigten Rollback wird ein neuer Sicherungspunkt des
aktuellen Stands erstellt. Der Zielstand wird anschließend unter einer neuen
Revisions-ID aktiviert; die bislang aktive Revision bleibt als Historie und
Recoveryquelle erhalten.

## Geführter Restore

Ein lokales Backup ist eine Wiederherstellungsquelle, kein stilles In-place-
Upgrade. Der Benutzer wählt Konto, Einstellungen und veränderliche Dateien. Das
Ergebnis ist immer ein neues Profil; die Quelle bleibt unangetastet.

Einstellungen werden nur innerhalb derselben Restore-Operation übernommen. Kann
das Profil nicht vollständig vorbereitet und aktiviert werden, wird die
vorherige Einstellungsdatei wiederhergestellt. Ein unaktivierter Datenbankeintrag
und sein Profilverzeichnis werden bereinigt.

## Versionswechsel und Abwärtsgrenze

Ein V2-Wiederherstellungspunkt reproduziert Runtime und Inhalte über seine
Quellrevision und Cachebindungen. Ein V1-Punkt erzeugt bewusst wieder eine
Phase-4-Grundlage. Es findet kein stilles Downgrade eines bestehenden Profils
statt.

Lokale Restore-Punkte sind nicht das portable Exportformat. Für Rechnerwechsel
bleibt das geheimnisfreie `.s9profile` aus Phase 6 die Austauschgrenze; echte
Zwei-Geräte-Synchronisierung gehört zu Phase 8.
