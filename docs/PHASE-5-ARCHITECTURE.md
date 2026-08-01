# Phase 5 – Laufzeitarchitektur

Status: aktueller lokaler Implementierungsstand. Dieses Dokument ist keine
Releasefreigabe und behauptet keine noch nicht ausgeführten End-to-End-Nachweise.

## Laufzeitmodell

Phase 5 verbindet die in Phase 3 geschützte Microsoft-Sitzung mit den in Phase 4
isolierten Profilen. Ein Profil besteht aus drei getrennten Ebenen:

| Ebene | Autorität | Veränderbarkeit |
|---|---|---|
| Profilabsicht | `manifest.json` der aktiven Revision | nur durch eine neue Revision |
| aufgelöste Laufzeit | `lock.json` | unveränderliche portable Autorität |
| Abfragestatus | `profile_runtime_projection` | abgeleitete SQLite-Projektion |
| Spielzustand | `<Profil-ID>/instance` | exklusiv durch dieses Profil veränderbar |

Minecraft-, Loader-, Asset- und optionale S9Lab-Komponentenartefakte werden als
inhaltsadressierte Cacheobjekte geladen. Die Operationsengine kopiert jedes Objekt
nach erneuter Größen- und SHA-256-Prüfung in ein Revisions-Staging. Erst danach wird
der vollständige Revisionsordner in das Profil verschoben.

Die Aktivierung der neuen Revisions-ID und das Schreiben ihrer
`profile_runtime_projection` erfolgen in derselben SQLite-Transaktion. Der
Operationsplan enthält außerdem die vorherige Projektion. Muss eine bereits
aktivierte Revision kompensiert werden, stellt eine zweite gemeinsame
SQLite-Transaktion den vorherigen Revisionszeiger und die dazugehörige Projektion
wieder her. Dateisystem-Move und SQLite-Transaktion sind technisch getrennt; das
bestehende Journal und die umgekehrte Kompensation überbrücken diese Grenze.
Hardlinks werden nicht verwendet.

## Kontrollierte Provider

Der Laufzeitresolver akzeptiert keine vom IPC übergebenen URLs:

- Mojang-Metadaten und -Inhalte besitzen getrennte feste Hostlisten.
- Fabric-Metadaten und Fabric-Maven besitzen getrennte feste Hostlisten.
- NeoForge darf ausschließlich von `maven.neoforged.net` bezogen werden.
- Redirects, Nicht-HTTPS, Benutzerinformationen, abweichende Ports und Fragmente
  werden abgewiesen.
- Größenbudgets werden vor und während des Streamings geprüft.
- Mojang-Artefakte werden mindestens gegen den veröffentlichten SHA-1 und intern
  anschließend gegen den berechneten SHA-256 gebunden.
- Fabric- und NeoForge-Artefakte werden gegen kontrolliert bezogene Sidecar-Hashes
  und nach dem Download gegen den berechneten SHA-256 gebunden.

Der S9Lab-Komponentenprovider ist davon getrennt. Ursprung und
Ed25519-Public-Keys können ausschließlich zur Buildzeit eingebettet werden. Ohne
beide Werte meldet er die Capability `unconfigured`.

Der IPC-Vertrag projiziert einen typisierten, URL-freien Komponentenkatalog. Ein
Eintrag enthält nur Komponenten-ID, Version, Minecraft-Version, Loaderbindung,
Größe und SHA-256. Der Rust-Kern gibt ausschließlich signaturgeprüfte Einträge
zurück, die zur angefragten Laufzeit passen. Ist die Capability nicht verfügbar,
liefert der Katalog ohne Netzwerkzugriff die Capability und eine leere
Eintragsliste. Die UI bietet nur diese Einträge zur Auswahl an; freie
Herkunfts-URLs oder freie Komponentenidentitäten sind nicht vorgesehen.

## Java-Strategie

Kontrolliertes System-Java ist der verfügbare und in der UI voreingestellte Pfad.
Der Rust-Kern durchsucht nur begrenzte bekannte Installationsorte, prüft die
vollständige Vertrauenskette des Kandidaten gegen Symlinks und Reparse Points und
verifiziert Version und Architektur durch einen direkten Prozessaufruf.

Der Vertrag für S9Lab-verwaltetes Java bleibt vorhanden, aber es existiert noch
keine hashgebundene Produktionsquelle samt atomarer Beschaffung. Deshalb ist die
Option in der UI sichtbar, aber deaktiviert. Im normalen unkonfigurierten Zustand
ohne bereits bereitgestelltes Managed-Java-Executable scheitert auch ein direkt
übermittelter Wunsch im Rust-Kern mit `runtime_managed_java_unavailable`. Ein
bereits vorhandener kontrollierter Pfad würde weiterhin auf Pfad, Version und
Architektur geprüft; eine Beschaffung oder Lieferprovenienz wird damit nicht
behauptet. Eine Laufzeitrevision wird erst vorbereitet, nachdem die gewählte
Java-Policy tatsächlich aufgelöst wurde.

## Installation und Reparatur

Eine Installation löst die konkrete Minecraft-, Loader- und Java-Absicht auf,
beschafft alle unveränderlichen Artefakte, erzeugt ein kanonisches Manifest und
Lock und plant eine typisierte Operation:

- `runtime-install`
- `runtime-repair`
- `component-change`

Die Reparatur verwendet ausschließlich die bereits gebundene aktive Revision als
Quelle der Absicht. Cachemetadaten allein genügen nicht: Die Operationsengine
hasht Cachequelle und erzeugte Revisionskopie erneut. Ein Fehler vor Abschluss
behält die vorherige aktive Revision bei oder stellt sie durch die bestehende
Kompensationsoperation einschließlich Runtime-Projektion wieder her.

Die Classpath-Ziele werden nicht alphabetisch umgeordnet. Bibliotheken aus dem
Mojang-Manifest behalten ihre Reihenfolge, Loaderbibliotheken folgen in ihrer
aufgelösten Reihenfolge, und das Client-JAR steht zuletzt. Dedupliziert wird stabil:
Das erste Vorkommen bleibt erhalten.

## Authentifizierter Start

Der Startpfad:

1. liest und validiert das aktive Manifest und Lock,
2. prüft jedes materialisierte Laufzeitartefakt erneut gegen Größe und SHA-256,
3. verlangt die dem Profil zugeordnete Microsoft-Identität,
4. erneuert die Minecraft-Sitzung ausschließlich im Rust-Kern,
5. löst und prüft Java,
6. validiert und extrahiert native Bibliotheken in ein startbezogenes, isoliertes
   Verzeichnis,
7. ersetzt ausschließlich bekannte Mojang-Platzhalter und
8. startet genau den aus dem Lock gebundenen Main-Class-/Classpath-Satz.

Der Minecraft-Access-Token wird nur im Rust-Prozess in die für Minecraft
erforderlichen Startargumente eingesetzt. Er ist kein IPC-, SQLite-, UI- oder
Logfeld. Standardausgabe und Standardfehler des Spielprozesses werden nicht in den
Launcher übernommen.

Der gezielte Stopp ist an die gespeicherte Launch-ID gebunden. Unter Windows wird
der Java-Prozess zunächst suspendiert erzeugt, einem launch-spezifischen Job Object
mit `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` zugeordnet, die Zuordnung verifiziert und
erst danach fortgesetzt. Dadurch kann vor der Eindämmung kein Kindprozess starten.
Stoppen und Launcher-Abbruch beenden den gesamten zugehörigen Prozessbaum, ohne
andere Launch-IDs anzutasten.

## NeoForge

Der produktive Service lädt einen versionsgebundenen NeoForge-Installer über den
kontrollierten Provider, verifiziert ihn und übergibt ihn an einen strikten
Offline-Parser. Der Parser validiert Installeridentität, ZIP-Budgets, Metadaten,
Mavenbindungen, Pfade, Platzhalter, Datenreferenzen, Prozessoren und erwartete
Ausgaben. Daraus entsteht ein hashgebundener, unveränderlicher Installationsplan.

Die Prozessorausführung bleibt bewusst fail-closed. Reale aktuelle
NeoForge-Installer deklarieren für benötigte Clientprozessoren nicht durchgehend
vertrauenswürdige Output-Hashes und enthalten Aufgaben mit Netzwerkbedarf. Zudem
ist keine Windows-Sandbox angebunden, die gleichzeitig Netzwerkfreiheit,
Prozessbaum-Timeout und eine exakte Schreib-Positivliste garantiert. Der Service
gibt deshalb den konkreten Readiness-Fehler zurück und aktiviert die
NeoForge-Capability nicht. Installation, Reparatur und Start von NeoForge sind
damit nicht als funktionsfähig behauptet.

## S9Lab-Komponente und Legacy-Abschaltung

Die optionale Komponente ist Bestandteil einer neuen unveränderlichen
Profilrevision, nicht des veränderlichen `instance/mods`-Ordners. Hinzufügen,
Wechseln und Entfernen erzeugen daher jeweils eine vollständige neue Revision.
Vor der Aufnahme gelten:

- exakt signierte Komponenten-ID, Version, Minecraft-Version, Loader,
  Loader-Version, Größe, SHA-256 und relativer Zielpfad,
- kontrollierte, aus den signierten Feldern abgeleitete Downloadroute,
- Download ausschließlich nach Operations-Staging,
- Größen-, SHA-256-, Ed25519-, JAR- und Descriptorprüfung,
- Verbot von Traversal, ADS, Sondernamen, mehrdeutigen Separatoren,
  Case-/Unicode-Kollisionen, Symlinks, Reparse Points und Hardlinks.

Die früheren globalen Minecraft-Quellen, alten Command-/Typfassaden und alle
gebündelten Default-Mod-JARs einschließlich `s9lab-client-bundled.jar` sind aus der
Arbeitskopie entfernt. `tauri.conf.json` bündelt keine Legacy-Ressourcen.

## Verbleibende Grenzen

- NeoForge bleibt aus den oben genannten Gründen ausführungsseitig
  `unconfigured`.
- Managed Java besitzt noch keine verifizierte Beschaffungs- und
  Aktivierungskette und bleibt deaktiviert.
- Der offizielle S9Lab-Produktionsprovider und sein öffentlicher
  Produktionsschlüssel fehlen extern; die Komponenten-Capability bleibt deshalb
  `unconfigured`.
- Reale authentifizierte Vanilla-, Fabric- und NeoForge-End-to-End-Starts sind im
  aktuellen lokalen Nachweisstand nicht vollständig belegt.

Diese Grenzen dürfen weder durch Testwerte noch durch eine schwächere
Sicherheitsprüfung verdeckt werden.
