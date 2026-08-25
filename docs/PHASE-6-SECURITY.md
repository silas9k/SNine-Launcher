# Phase 6 – Sicherheitsmodell für Inhalte und Profiltransfer

Status: aktueller lokaler Implementierungsstand; keine Releasefreigabe.

## Vertrauensgrenzen

| Eingabe | Vertrauen | zwingende Prüfung |
|---|---|---|
| Suchtext und Filter aus der UI | untrusted | typisierte Inhaltsart/Loader, begrenzte Versionstoken, Offset- und Ergebnislimit, keine unbekannten oder URL-Felder |
| Modrinth-API-Antwort | netzwerkbasiert | exakter HTTPS-Host, keine Redirects, JSON-Content-Type, Größenbudget, strikte Identitäten und Feldgrenzen |
| Modrinth-Dateimetadaten | vollständig untrusted | zulässige URL-Metadaten, eindeutige offizielle CDN-Route, Projekt-/Versionsbindung, sicherer Dateiname, Größe und SHA-512 |
| Inhaltsdownload | vollständig untrusted | Streaming-Größe, SHA-512, berechneter SHA-256, Staging und Archivprüfung vor Aktivierung |
| Inhaltsrelease und Lock | lokal, aber manipulierbar | Formatversion, Runtimebindung, kanonische Reihenfolge, Graph, Packmitgliedschaften, Overrides, Ziele und Lock-Hash |
| lokale Benutzerdatei | vollständig untrusted | absoluter eindeutiger Pfad, enge registrierte Wurzel, vollständige Pfadkette, reguläre Einzeldatei, Größe, Hash und Archivformat |
| MRPACK und Override | vollständig untrusted | Container-/ZIP-Budgets, exakter Index, Runtimebindung, Ziele, Provideridentitäten, Digests, Verbot ausführbarer Zieltypen und Packübergang |
| S9Lab-Profilexport | vollständig untrusted | exakte Formatversion und Eintragsmenge, ZIP-Typen/-Budgets, Manifest-/Artefaktbindung und Staging |
| bestehende Instanzdatei/Marker | lokal, aber manipulierbar | registrierter Pfad, Marker-Hash, Revisionsbindung, Dateityp und Fremddateischutz |

## Provider- und URL-Schutz

Die Modrinth-Integration akzeptiert keine freie API-, Download- oder Spiegel-URL
über IPC. API-Routen werden intern vom festen Ursprung
`https://api.modrinth.com` abgeleitet. Produktdownloads verwenden ausschließlich
die validierte Route auf `https://cdn.modrinth.com` mit Standardport 443.

Abgewiesen werden insbesondere HTTP, Host- oder Subdomaintricks, abweichende
Ports, Benutzerinformationen, Fragmente, CDN-Queryparameter, Redirects,
Identitätsdrift, fehlender SHA-512 und unsichere Dateinamen.

Ein MRPACK darf höchstens acht Downloadangaben je Datei enthalten. Alle Angaben
müssen HTTPS mit Standardport und einem begrenzten Metadaten-Hostset verwenden.
Für die tatsächliche Beschaffung muss mindestens eine eindeutige, identisch
gebundene `cdn.modrinth.com/data/<Projekt>/versions/<Version>/...`-Route vorhanden
sein; andere erlaubte Spiegelangaben werden nicht ausgewählt. Vor dem Download
bestätigt der Versionsendpunkt erneut Projekt, Version, exakte Route, Größe und
SHA-512.

Validierte Download-URLs verbleiben in privaten, von Serialisierung und
Debugausgabe ausgeschlossenen Feldern. Öffentliche Phase-6-Antworten enthalten
keine Download-URL.

## Reproduzierbare Auflösung, Packs und Konflikte

Die kanonische Lock-Nutzlast bindet Runtime, Policy, Anforderungen, Releases,
Aktivierungszustände, Abhängigkeiten, Herkunft, Ziele, Größen, SHA-256,
Pack-Mitgliedschaften und Override-Seeds. Anders sortierte oder nachträglich
veränderte Darstellungen werden abgewiesen.

Der Resolver prüft erforderliche und aktivierte optionale Kanten transitiv.
Inkompatibilitätskanten bleiben auch ohne derzeit installiertes Ziel im Lock.
Zwei Inhalte oder Overrides dürfen nach portabler Case-/Unicode-Normalisierung
weder dasselbe Ziel noch Datei-/Vorfahren-Aliase besitzen.

Pack-Mitgliedschaften erzwingen exakte Versionen, vorhandene Pack- und
Mitgliedsauswahlen, konsistente Aktivierung und höchstens einen Besitzer je
gemeinsamer Auswahl. Packupdates dürfen eine manuelle oder von einem zweiten Pack
benötigte Auswahl nicht still ersetzen oder entfernen. Versionskonflikte führen
vor der Revisionserstellung zum Abbruch.

## Pfad-, Datei- und Archivschutz

Normale Inhaltsziele dürfen nur eine einzelne Datei unter `mods/`,
`shaderpacks/`, `resourcepacks/` oder `modpacks/` bezeichnen. Die zentrale
Windows-sichere Normalisierung schützt weiterhin vor:

- absoluten Pfaden und Traversal,
- leeren, wiederholten, gemischten oder nicht kanonischen Separatoren,
- Alternate Data Streams und Windows-Sondernamen,
- Unicode-/Case-Kollisionen,
- Symlinks, Junctions, Reparse Points, Hardlinks und Sonderdateien sowie
- Datei-/Verzeichnis- und Vorfahren-Aliasen.

Lokale Inhaltsarchive werden vollständig inventarisiert, aber nicht entpackt.
Feste Budgets begrenzen Archivgröße, Eintragszahl, Einzel- und Gesamtgröße sowie
Kompressionsverhältnis. Zusätzlich muss ein zur Inhaltsart passender Descriptor
vorhanden sein.

Der gemeinsame MRPACK-Pfad prüft den Container im internen Operations-Staging
vor jeder Cacheaktivierung, öffnet ihn für Inventar und Extraktion erneut und
hasht Container, Mitglieder und Seeds unmittelbar vor ihrer abschließenden
Cacheübernahme erneut. `modrinth.index.json` lehnt unbekannte Strukturfelder ab;
zusätzliche Hashalgorithmen sind nur in begrenzter, kanonischer Form zulässig.
Clientrelevante Indexziele sind auf `mods/`,
`shaderpacks/` und `resourcepacks/` begrenzt. Projektidentitäten, Ziele,
Mitgliedszahlen und Gesamtgrößen müssen eindeutig und innerhalb fester Budgets
liegen.

## Override-Seeds

Nur reguläre Dateien unter `overrides/` oder `client-overrides/` werden als
Client-Seeds übernommen; `server-overrides/` wird ignoriert. Andere zusätzliche
Dateien sind verboten. Der Zielpfad wird zentral normalisiert, auf höchstens 32
Segmente begrenzt und gegen Indexziele sowie andere Overrides kollisionsgeprüft.

Verboten sind unter anderem Launcher-/Runtimebereiche, `.s9lab`, Welten, Logs,
Crashreports, Screenshots, Assets, Bibliotheken und Backups. Ebenso verboten sind
Account-/Servercache-Dateien und ausführbare oder skriptartige Endungen wie EXE,
MSI, BAT, CMD, PowerShell, REG, LNK und URL.

Jeder Seed wird während der Extraktion größenbegrenzt gehasht, in den
inhaltsadressierten Cache übernommen und im Lock an Pack-ID, Ziel, Größe und
SHA-256 gebunden. Dieser Hash schützt die Seedquelle und die Revisionskopie.

Nach der ersten Projektion steht die Zieldatei ausdrücklich unter Instanzhoheit.
Eine bereits vorhandene reguläre Datei wird nicht überschrieben; spätere lokale
Änderungen oder Löschung werden nicht als Hashfehler behandelt. Ein fehlendes
Ziel wird im unverändert aktiven Projektionszustand nicht bei jedem Start neu
gesät; nach einer späteren Deaktivierung und erneuten Aktivierung darf der
hashgebundene Seed ein weiterhin fehlendes Ziel wieder anlegen. Deaktivieren,
Aktualisieren oder Entfernen des Packs löscht einen vorhandenen lokalen Stand
nicht. Links, Reparse Points, Verzeichnisse oder portable Pfadabweichungen bleiben
auch für Overrides unter Instanzhoheit unzulässig.

## Profilformat und Geheimnisse

Das portable Profilformat enthält keine Tokens, Device-Codes, Sitzungsdaten,
Accountidentitäten, Logs, Welten, Crashreports, absoluten lokalen Pfade oder
Provider-/Download-URLs.

Das Manifest lehnt unbekannte Felder ab. Das Archiv erlaubt nur `profile.json`
und exakt die durch den validierten Inhalts-Lock erwarteten
`artifacts/<sha256>`-Dateien, einschließlich der unveränderlichen Override-Seeds.
Präfixdaten, ZIP-Kommentare, verschlüsselte oder besondere Einträge, zusätzliche
Dateien, Kollisionen und fehlende Artefakte werden abgewiesen. Lokal veränderte
Instanz-Overrides werden nicht zurück in den portablen Export gelesen; exportiert
wird ausschließlich der hashgebundene Seed der aktiven Revision.

## Revisions- und Projektionsintegrität

Inhaltsänderungen laufen als `content-install`, `content-change` oder
`content-import` über die typisierte Operationsengine. Manifest- und Lock-Hash,
Cachematerialisierungen, neue Runtime-Projektion und vorherige Projektion sind
Teil des Plans. Eine fehlgeschlagene Aktivierung kompensiert auf die vorherige
Revision.

Die Startprojektion besitzt einen hashgebundenen Marker v2. Normale verwaltete
Inhalte werden vor jeder Änderung erneut gegen Größe und SHA-256 geprüft und nur
markergebundene Dateien dürfen ersetzt oder entfernt werden. Override-Einträge im
Marker kennzeichnen dagegen ausschließlich die Seedherkunft; ihre Ziele unter
Instanzhoheit werden nicht still ersetzt. Ein Fehler beim erstmaligen Säen oder beim
Aktivieren normaler Inhalte stellt die vorherigen verwalteten Dateien und den
Marker wieder her.

## Verbleibende Nachweisgrenzen

- Reale Modrinth-Suche, Downloads, Updateprüfung und repräsentative lokale sowie
  direkte MRPACK-Importe sind durch Unit- und statische Tests allein nicht als
  produktives End-to-End belegt.
- Der lokale Phase-6-Gesamtlauf sowie Browser- und Chromium-Performance-Gates
  sind protokolliert. Nativer Tauri-Kaltstart, Prozess-Working-Set und das
  spätere Cleanroom-Endgate bleiben separat.
- Das Phase-7-Backup-/Update-/Migrationsprodukt existiert in Phase 6 nicht.

Diese Punkte sind keine Sicherheitsausnahmen. Ein nicht nachgewiesener oder
fehlgeschlagener Pfad darf den bisherigen aktiven Profilstand nicht verändern.
