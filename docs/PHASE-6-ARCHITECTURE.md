# Phase 6 – Inhalts- und Profilformatarchitektur

Status: aktueller lokaler Implementierungsstand. Dieses Dokument ist keine
Releasefreigabe und nimmt weder Phase 7 noch ausstehende reale
End-to-End-Nachweise vorweg.

## Autoritäten und Projektionen

Phase 6 erweitert die unveränderlichen Phase-5-Profilrevisionen um eine eigene
Inhaltsdomäne. Die Zuständigkeiten bleiben getrennt:

| Ebene | Autorität | Zweck |
|---|---|---|
| Inhaltsabsicht | `desiredContent` im `site.s9lab.profile`-Manifest v2 | explizit angeforderte Inhalte, Versionsanforderung und Aktivierungsstatus |
| Inhaltsauflösung | optionaler `content`-Block im `site.s9lab.profile-lock` v2 | kanonischer Inhaltsgraph samt `items`, `packMembers`, `overrides` und `resolutionSha256` |
| Revisionsartefakte | `revisions/<Revision>/content/...` | unveränderliche, aus dem Cache verifizierte Inhalte und Override-Seeds |
| Startprojektion | `instance` und `instance/.s9lab/content-projection.json` v2 | transaktional aus genau einer Revision abgeleiteter Startzustand |
| Portabler Austausch | `site.s9lab.profile-export` v1 | versioniertes, geheimnisfreies Profil mit inhaltsadressierten Artefakten |
| Cache | SHA-256-adressierte Cacheobjekte | gemeinsame unveränderliche Quelle; keine gemeinsam veränderlichen Instanzdateien |

Die Inhaltsauflösung ist keine veränderliche SQLite-Autorität. Sie gehört zum Lock
einer festgeschriebenen Profilrevision. SQLite-Schema v6 und die bestehende
Runtime-Projektion bleiben die Abfrage- und Operationsgrundlage; Phase 6 fügt
keine Schema-Migration hinzu.

## Inhaltsdomäne und Resolver

`src-tauri/src/content` definiert ein URL-freies, versioniertes Modell für Mods,
Modpacks, Shader- und Ressourcenpakete. Ein Release bindet Identität, Version,
Minecraft-/Loader-Kompatibilität, Abhängigkeiten, Ziel, Größe, SHA-256 und eine
typisierte Herkunftsidentität.

Der Resolver:

- verarbeitet `required`, `optional` und `incompatible` als getrennte Kanten,
- erzwingt Minecraft-Version, Loaderart und optionale Loaderversion,
- backtracked deterministisch, wenn der bevorzugte Release nicht auflösbar ist,
- begrenzt Graphgröße, Rekursion und Gesamtgröße,
- lehnt Ziel-, Vorfahren-, Case- und Unicode-Kollisionen ab,
- sortiert Auswahl, Releases und Abhängigkeiten kanonisch und
- bindet Runtime, Inhalte, Pack-Mitgliedschaften und Override-Seeds an
  `resolutionSha256`.

Optionale Abhängigkeiten werden nur bei explizit aktivierter Resolver-Policy zu
strikten Kanten. Einseitig deklarierte Konflikte bleiben im Lock erhalten und
schützen dadurch spätere Änderungen.

Aktivieren, Deaktivieren, Installieren, Entfernen, Aktualisieren und lokales
Hinzufügen erzeugen jeweils eine neue Profilrevision. Erforderliche
Abhängigkeiten können nicht isoliert deaktiviert oder entfernt werden. Ein
Laufzeitwechsel wird bei vorhandenen Inhalten abgewiesen, bis eine neue
kompatible Auflösung vorliegt; Reparatur und S9Lab-Komponentenwechsel übernehmen
den vorhandenen Inhalts-Lock.

## Kontrollierter Modrinth-Provider und Updates

`src-tauri/src/modrinth` besitzt typisierte Such-, Projekt-, Versions-,
Abhängigkeits- und Dateimodelle. Die UI kann Suchtext, Inhaltsart,
Minecraft-Version, Loader, Offset und Limit übergeben, aber keine API- oder
Download-URL.

Der Provider leitet API-Routen von `https://api.modrinth.com/v2` ab. Verwendete
Downloads werden intern an `cdn.modrinth.com`, Projekt- und Versionsidentität,
Dateiname, Größe und SHA-512 gebunden. Redirects sind deaktiviert; Antwortgrößen
und Strukturen sind begrenzt. Interne URLs sind weder serialisierbare Modellfelder
noch Teil der Debugausgabe oder des IPC-Vertrags.

Normale Inhalte werden nach Staging geladen, gegen Upstream-SHA-512 geprüft,
intern mit SHA-256 adressiert, als Archiv validiert und erst nach erfolgreicher
Auflösung in eine neue Revision übernommen.

Der getrennte Befehl `phase6_check_content_updates` ergänzt eine bereits
validierte Momentaufnahme um neuere kompatible Modrinth-Versionen. Er prüft
höchstens 256 eigenständig verwaltete Einträge mit maximal acht parallelen
Abfragen. Pack-Mitglieder werden nicht einzeln aktualisiert; ihr Update erfolgt
über das zugehörige Modpack, damit dessen Versionen und Mitgliedschaften zusammen
bleiben.

## Gemeinsamer MRPACK-Pfad

Eine direkte Modrinth-Modpack-Installation und der lokale Import einer
`.mrpack`-Datei münden in `import_staged_modrinth_pack`. Der Unterschied liegt nur
in der Herkunft des Containers:

- Der Modrinth-Pfad lädt die ausgewählte `.mrpack`-Version über den kontrollierten
  Provider nach Staging.
- Der lokale Pfad kopiert die gewählte Datei größenbegrenzt nach Staging und
  leitet Pack-ID und Version aus ihrem SHA-256 ab.

Danach gilt derselbe Ablauf:

1. Containerformat, Archiv, `modrinth.index.json`, Runtime, Ziele,
   Mitgliedschaften, Overrides und Gesamtbudgets vollständig vorprüfen.
2. Den unveränderten Packcontainer im ausschließlich intern erreichbaren
   Operations-Staging erneut öffnen, inventarisieren und für die Extraktion
   verwenden.
3. Jedes Packmitglied über seine gepinnte Modrinth-Version auf Projekt-ID,
   Versions-ID, Datei, Größe, SHA-512, Laufzeitkompatibilität und Abhängigkeiten
   zurückbinden.
4. Mitglieder herunterladen und im Staging erneut als Inhalt validieren.
5. Overrides extrahieren, SHA-256 berechnen und als unveränderliche Seeds
   vorbereiten.
6. Packübergang, Inhaltsgraph, Pack-Mitgliedschaften und Override-Seeds gemeinsam
   validieren.
7. Erst danach Container, Mitglieder und Seeds erneut hashen, in den
   inhaltsadressierten Cache übernehmen und in einer neuen Profilrevision
   aktivieren.

Cacheaktivierungen für Mitglieder und Overrides erfolgen erst nach der
vollständigen Auflösung. Ein Fehler bereinigt das zugehörige MRPACK-Staging und
aktiviert keine neue Profilrevision.

## Pack-Mitgliedschaft und Aktualisierung

`ResolvedContentPackMemberV1` bindet Pack-ID, Mitglieds-ID, exakte Version,
Standardaktivierung und den Besitzer der gemeinsamen Auswahl. Dadurch können
mehrere Packs dasselbe Mitglied nur bei identischer Version teilen. Pro
Mitgliedsauswahl gibt es höchstens einen Packbesitzer.

Beim Packupdate wird der bisherige Modrinth-Pack desselben Projekts ersetzt. Der
Übergang:

- entfernt nicht mehr enthaltene, ausschließlich vom alten Pack besessene
  Auswahlen,
- überträgt Besitz an ein weiteres Pack, wenn dieses dasselbe Mitglied weiterhin
  benötigt,
- erhält unabhängig manuell angeforderte Inhalte,
- lehnt Versionskonflikte zwischen Packs oder mit manuellen Auswahlen ab und
- berechnet Aktivierung aus manueller Absicht und aktivierten Packs neu.

Ein aktives Pack schützt standardmäßig aktivierte Mitglieder vor isoliertem
Deaktivieren. Pack-Mitglieder können nicht einzeln entfernt oder aktualisiert
werden. Direkte Benutzeraktionen an einem nicht vom aktiven Pack erzwungenen
Mitglied lösen es als manuelle Auswahl aus der Pack-Eigentümerschaft.

## Override-Seeds mit lokaler Datenhoheit

MRPACK-Dateien unter `overrides/` und `client-overrides/` werden sicher
inventarisiert. Bei demselben kanonischen Ziel hat `client-overrides/` Vorrang;
gleichrangige oder portable Case-/Unicode-Kollisionen werden abgewiesen.
`server-overrides/` werden im Clientprofil ignoriert. Andere zusätzliche
Top-Level-Dateien sind nicht zulässig.

Jeder ausgewählte Override wird an Pack-ID, relativen Zielpfad, Größe und SHA-256
gebunden und als Revisionsartefakt gespeichert. Bei der Startprojektion eines
aktivierten Packs ist er jedoch nur ein Startwert:

- Fehlt die Zieldatei beim ersten bekannten Projektionszustand, wird der
  verifizierte Seed angelegt.
- Existiert dort bereits eine reguläre Datei, bleibt sie unverändert.
- Nach der ersten Projektion werden lokale Änderungen nicht durch spätere
  Starts, Deaktivierung oder Packupdates rückgängig gemacht.
- Ein bewusst gelöschtes Ziel wird bei unverändert aktivem Projektionszustand
  nicht bei jedem Start erneut gesät. Wird der Pack später deaktiviert und
  wieder aktiviert, darf ein weiterhin fehlendes Ziel erneut aus dem
  hashgebundenen Seed angelegt werden.
- Auch beim Entfernen eines Packs wird ein bereits unter Instanzhoheit stehender
  Override nicht still gelöscht.

Der Lock-Hash beweist somit die Herkunft des verfügbaren Seeds, nicht den
aktuellen Inhalt einer danach benutzer- oder spielveränderten Instanzdatei.
Modpackcontainer selbst werden nie in die Spielinstanz projiziert.

## Modpack-Editor und IPC

IPC-Vertrag v6 ergänzt zwölf Phase-6-Befehle für Momentaufnahme, Updateprüfung,
Modrinth-Suche und -Detail, Installation, Aktivierung, Deaktivierung, Entfernung,
Update, lokales Hinzufügen, MRPACK-Import sowie Profilimport und -export.

`ContentEditor` integriert diese Fähigkeiten profilbezogen in die Bibliothek. Er
zeigt Minecraft-/Loaderbindung, Lock-Hash, installierte Inhalte,
Abhängigkeiten, Konflikte und Updatezustand. Der native Dateipfad wird nur an den
Rust-Kern übergeben; im Browserfallback werden lokale Dateiaktionen deaktiviert.

## Versioniertes S9Lab-Profilformat

`site.s9lab.profile-export` v1 ist ein deterministisches ZIP mit genau einem
`profile.json` und nach SHA-256 benannten `artifacts/`. Das Manifest enthält
Anzeigename, Laufzeitabsicht, optionale S9Lab-Komponente, Inhaltsabsicht und
Inhalts-Lock einschließlich Pack-Mitgliedschaften und Override-Seeds.

Artefakte mit gleichem Hash werden dedupliziert. Der Export prüft jede Quelle
erneut und aktiviert die neue Datei erst nach vollständigem Schreiben,
Synchronisieren und Hashen. Der Import inventarisiert das gesamte Archiv vor der
Extraktion, prüft alle Artefakte im Staging und erstellt immer ein neues Profil.
Exportiert werden die unveränderlichen Seedbytes der Revision, nicht später
veränderte oder zusätzlich angelegte Dateien aus der Spielinstanz.

## Profilduplizierung mit Phase-6-Zustand

Eine Duplizierung eines installierten V2-Profils erzeugt neue Profil- und
Revisionsidentitäten, übernimmt aber Laufzeitabsicht, Laufzeitprojektion,
Inhaltsabsicht, Inhalts-Lock, Packmitgliedschaften und Override-Seeds. Alle
unveränderlichen Laufzeit-, Inhalts- und Seedartefakte werden über ihre
verifizierten Cacheobjekte für die neue Revision materialisiert.

Aus dem veränderlichen Instanzbaum werden weder der alte `.s9lab`-Marker noch
projizierte unveränderliche Mods, Shader oder Ressourcenpakete kopiert. Welten,
Einstellungen und bereits veränderte Override-Ziele bleiben dagegen erhalten.
Dadurch startet das Duplikat mit eigener Revisionsidentität, ohne Marker des
Quellprofils oder Fremddateikonflikte zu erben.

## Abgrenzung zu Phase 7

Phase 6 stellt keine Updatekanäle für Launcher und S9Lab Client, keine
Updatevorschau, keinen automatischen Wiederherstellungspunkt, keine
Backupverwaltung und keine geführte Migration bereit. Inhaltsrevisionen und
Packupdates verwenden die vorhandene Operationskompensation; das vollständige
Update- und Backupprodukt bleibt Phase 7.
