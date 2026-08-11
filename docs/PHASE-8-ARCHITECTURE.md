# Phase 8 – Konto- und Cloud-Sync-Architektur

Status: lokaler Checkpointstand vom 2026-08-11. Die vorbereitbare Seite der
optionalen S9Lab-Verknüpfung ist implementiert. Ohne freigegebene Backend-API
bleibt der Provider absichtlich deaktiviert.

## Kontomodell

Microsoft bleibt das Basiskonto. Der neue Cloud-Bereich liest ausschließlich
die bereits ausgewählte, bestätigte Microsoft-Identität. Eine S9Lab-Identität
ist davon getrennt und wird weder erfunden noch aus dem Minecraft-Namen
abgeleitet. Der aktuelle Provider meldet `unconfigured`, besitzt keine URL und
weist Link-, Pull- und Push-Versuche mit `cloud_provider_unconfigured` ab.

## Synchronisierbarer Zustand

`SyncPayloadV1` ist eine geschlossene, versionsgebundene Struktur. Sie enthält:

- Profil-ID, Anzeigename, Lebenszyklus, Favorit und aktive Revision,
- Inhalts-ID, Typ, Version und Aktivzustand pro Profil,
- die ungefährlichen Shell-Einstellungen für Darstellung und Sprache.

Accounts, Tokens, Sitzungen, Welten, Logs, Java- oder Spielpfade und beliebige
Dateien sind strukturell nicht Teil dieses Formats. Der Payload wird kanonisch
serialisiert und mit SHA-256 an eine lokale Revision gebunden.

## Zwei Geräte und Konflikte

Der Kern enthält einen deterministischen Drei-Wege-Merge aus Basis-, Lokal- und
Remote-Zustand. Einseitige Änderungen werden übernommen. Abweichende Änderungen
desselben Feldes werden als Konflikt ausgegeben; ohne vollständige manuelle Wahl
zwischen `local` und `remote` entsteht kein aufgelöster Zustand. Das Modell ist
auf zwei Geräte begrenzt und wird erst durch einen realen Provider aktiviert.

## Oberfläche und IPC

Der einzige neue öffentliche Befehl liefert eine schmale Status- und
Revisionszusammenfassung. Der Accounts-Bereich zeigt Microsoft als Basis,
Verknüpfungs-, Online- und Gerätestatus, lokalen Umfang sowie die ausgeschlossenen
Datenklassen. Die Link-Schaltfläche ist ehrlich deaktiviert. Weder Backend-URL
noch Pfad oder Geheimnis passieren IPC.
