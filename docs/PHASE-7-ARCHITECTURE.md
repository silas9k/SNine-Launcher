# Phase 7 – Update-, Backup- und Wiederherstellungsarchitektur

Status: lokaler Checkpointstand vom 2026-08-11. Phase 7 erweitert die
unveränderlichen Profilrevisionen aus Phase 5/6, ohne neue produktive
Vertrauenswerte oder Update-Endpunkte zu erfinden.

## Getrennte Updatekanäle

Das Update-Center behandelt vier Kanäle als getrennte Zuständigkeiten:

| Kanal | lokaler Stand | Autorität |
|---|---|---|
| Launcher | gesperrt/unconfigured | benötigt eine freigegebene, signierte Produktionsquelle |
| Profile | verfügbar | lokale Profilrevisionen und Wiederherstellungspunkte |
| S9Lab Client | gesperrt/unconfigured | benötigt den freigegebenen Phase-5-Komponentenprovider |
| Inhalte | verfügbar | kontrollierter Modrinth-Provider und Phase-6-Inhaltslock |

Die versionierte Richtlinie `update-policy.json` erlaubt `manual` oder
`automatic` pro Kanal. Automatik kann nur für tatsächlich verfügbare Kanäle
gespeichert werden. Automatische Inhaltsänderungen benötigen sowohl die
Profilfreigabe als auch die Inhaltsfreigabe; eine einzelne Freigabe allein darf
kein Profil verändern. Das Update-Center kann diese konfigurierte Automatik beim
Öffnen einmal ausführen; ein systemweiter Hintergrunddienst ist nicht
vorgetäuscht.

## Vorschau und Anwendung

Eine Profilvorschau bindet sich an die aktive Basisrevision und zeigt nur
kompatible, vom Phase-6-Provider ermittelte Inhaltsänderungen. Jede Änderung
weist die Prüfkette `modrinth-sha512-and-launcher-sha256` aus. Vor der ersten
Mutation wird automatisch ein Wiederherstellungspunkt erzeugt.

Ausgewählte Updates werden erneut gegen den aktuellen Providerzustand geprüft
und einzeln als neue unveränderliche Profilrevisionen angewendet. Scheitert ein
Schritt, klont der Operationskern die Basisrevision in eine neue Revision
zurück. Der aktive Revisionszeiger wird nicht unjournalisiert zurückgeschrieben.

## Wiederherstellungspunkte

Ein lokaler Wiederherstellungspunkt liegt unter `backups/<Backup-ID>` und
enthält:

- ein versioniertes, streng geparstes `site.s9lab.restore-point`-Dokument,
- Profil- und Quellrevisionsidentität,
- ungefährliche Shell-Einstellungen ohne Geheimnisse,
- ein sortiertes Inventar veränderlicher Instanzdateien mit Größe und SHA-256
  sowie
- verifizierte Kopien dieser Dateien ohne Hardlinks.

Unveränderliche Runtime-, Inhalts- und Override-Seed-Artefakte werden nicht
dupliziert. Stattdessen hält eine Cache-Referenz der Art `backup` die zur
Quellrevision gehörenden Hashobjekte erreichbar.

## Geführte Wiederherstellung und Migration

Die Wiederherstellungsansicht lässt den Benutzer einen lokalen Sicherungspunkt
und die zu übernehmenden Bereiche auswählen:

- Microsoft-Kontozuordnung,
- ungefährliche Launcher-Einstellungen und
- veränderliche Profildateien wie Welten und Konfiguration.

Das Profil selbst wird immer als neue isolierte Profilidentität angelegt. Das
bestehende Profil bleibt unverändert und ist damit der sichere alte Datenstand.
Für eine V2-Quelle werden Runtime, Inhaltslock, Packmitgliedschaften und
Override-Seeds unter neuen Profil-/Revisionsidentitäten reproduziert. Ein
älteres V1-Profil bleibt als V1-Grundlage wiederherstellbar.

Vor dem Kopieren wird das Backup vollständig inventarisiert. Jede Zieldatei wird
nach dem Kopieren erneut gegen Größe und SHA-256 des Backup-Manifests geprüft,
bevor das neue Profil aktiviert werden darf. Scheitert die Wiederherstellung,
werden das unaktivierte Profil und gegebenenfalls bereits übernommene
Einstellungen kompensiert.

## IPC und Bedienoberfläche

IPC-Vertrag v7 ergänzt acht Phase-7-Befehle für Momentaufnahme, Richtlinie,
Vorschau, Wiederherstellungspunkt, Anwendung, Rollback, Restore und den
konfigurierten Automatiklauf. Absolute lokale Pfade, Download-URLs und Tokens
werden nicht als Phase-7-Antworten ausgegeben.

Die neue Update-Seite verwendet das bestehende responsive Designsystem: kompakte
Kanalzustände, selektierbare Änderungsvorschau, Hash-/Signaturhinweise,
Revisionszeitleiste, Backupkarten sowie bestätigte Rollback- und
Restore-Dialoge. Launcher- und Komponentenkanal zeigen ihren gesperrten Zustand
verständlich, statt eine nicht vorhandene Produktionsfähigkeit anzubieten.
