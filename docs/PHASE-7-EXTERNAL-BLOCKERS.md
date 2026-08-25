# Phase 7 – externe Produktionsblocker und Nachweisgrenzen

Status: 2026-08-11.

## Launcher-Updatekanal

Für einen produktiven Launcher-Updater fehlen im Arbeitsumfang:

- die freigegebene HTTPS-Releasequelle,
- ein neues, nicht kompromittiertes öffentliches Vertrauensmaterial,
- signierte Release-Manifeste und Artefakte sowie
- der organisationsseitige Authenticode-Schlüssel und Signierprozess.

Der Kanal bleibt deshalb sichtbar, aber gesperrt. Es gibt keinen Raw-URL-
Fallback, kein HTTP und keine Testsignatur als Produktionsersatz.

## S9Lab-Client-Kanal

Die in Phase 5 dokumentierte Produktions-Origin und der öffentliche Schlüssel
für den modularen S9Lab Client wurden nicht extern bereitgestellt. Der
eigenständige Updatekanal bleibt daher ebenfalls gesperrt. Bereits vorhandene
lokale Resolver- und Signaturprüfungen werden nicht abgeschwächt.

## Authenticode-Gate

Der lokal erzeugte NSIS-Installer ist absichtlich unsigniert und nur ein
Diagnoseartefakt. Das verbindliche Phase-7-Freigabe-Gate „Signatur und
Authenticode grün“ kann lokal ohne den extern verwalteten Produktionsschlüssel
nicht abgeschlossen werden. Dieser Blocker rechtfertigt weder das Erzeugen noch
das Einchecken eines privaten Schlüssels.

## Reale Netzwerknachweise

Echte Inhaltsupdates benötigen die erreichbare offizielle Modrinth-API und das
CDN. Provider-Ausfall oder Ratenbegrenzung müssen als verständlicher
Fehlerzustand enden und dürfen weder Hash-, Host- noch Identitätsprüfungen
lockern.

## Lokal abgeschlossener Umfang

Profil-/Inhaltskanäle, manuelle Richtlinie, verfügbare Automatik, Vorschau,
lokale Sicherungspunkte, Revisionsrollback, selektiver Restore als neues Profil,
Fehlerkompensation und die responsive Update-Center-Oberfläche sind lokal
implementiert. Ein dauerhaft laufender Hintergrundscheduler, produktive
Signierung und Veröffentlichung gehören nicht zum vorgetäuschten lokalen Stand.
