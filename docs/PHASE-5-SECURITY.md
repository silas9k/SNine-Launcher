# Phase 5 – Sicherheitsmodell

Status: aktueller lokaler Implementierungsstand; keine Releasefreigabe.

## Vertrauensgrenzen

| Eingabe | Vertrauen | zwingende Prüfung |
|---|---|---|
| Minecraft-/Loader-Auswahl aus der UI | untrusted | begrenzte ASCII-Versionstoken und unterstützte Kombination |
| Mojang-/Loader-Metadaten | netzwerkbasiert | feste HTTPS-Hosts, keine Redirects, Größenlimit, strikte Struktur |
| Artefaktdownload | untrusted | erwartete Größe, Upstream-Digest, berechneter SHA-256, Staging |
| Profilmanifest und Lock | lokal, aber manipulierbar | Formatversion, Identitäten, Manifest-Hash, Pfade, vollständige Lockvalidierung |
| Cacheeintrag | lokal, aber manipulierbar | SQLite-Zustand, Pfad, Größe und erneuter Datei-Hash |
| Java-Programm | ausführbarer Code | kontrollierter absoluter Pfad und vollständige geprüfte Vertrauenskette, Reparse-/Symlinkprüfung, Version und Architektur; für Managed Java zusätzlich ein verifizierter Liefernachweis |
| S9Lab-Katalog und JAR | vollständig untrusted | kontrollierter Ursprung, Signatur, Kompatibilität, Größe, Hash, JAR-Struktur, Descriptor und Ziel |
| NeoForge-Installer und Prozessorplan | vollständig untrusted | gebundene Quelle, Größe und Hash, ZIP-Budgets, strikte Metadaten, Pfade, Prozessoren, Output-Hashes und Sandbox-Capabilities |
| JAR-/Native-Eintrag | vollständig untrusted | normalisierter Pfad, Typ, Budgets, Kollisions- und Linkschutz |

## Geheimnisse

Microsoft-Refresh-Token und Minecraft-Access-Token bleiben im geschützten
OS-Schlüsselspeicher beziehungsweise im Rust-Speicher. Die öffentlichen
Phase-5-Verträge enthalten nur Profil-, Laufzeit-, Komponenten- und
Prozessstatusfelder. Verboten sind insbesondere:

- Token oder Device-Codes in IPC-Antworten,
- Geheimnisse in SQLite, Manifest, Lock oder Journal,
- Startargumente oder Umgebungswerte in Launcher-Logs,
- persistente Browser-Speicherung,
- private Produktions- oder Testschlüssel im Produktbuild.

Testsignaturen dürfen nur unter `#[cfg(test)]` erzeugt werden. Produktcode enthält
ausschließlich öffentliche Ed25519-Schlüssel und nur dann, wenn sie beim Build
explizit bereitgestellt wurden.

## Komponentenvertrauen

Die Signaturdomäne lautet `S9LAB-COMPONENT-MANIFEST-V1`. Die kanonische,
längenpräfixierte Signaturnutzlast bindet:

- Signaturdomäne und Formatversion,
- Key-ID,
- Komponenten-ID und -Version,
- Minecraft-Version,
- Loader und optionale Loader-Version,
- Dateigröße und SHA-256,
- exakten relativen Zielpfad.

Der Provider leitet Katalog- und Artefaktrouten selbst aus einer einzigen,
zur Buildzeit festgelegten HTTPS-Origin ab. Weder Katalog noch IPC dürfen eine
Raw-, Download- oder Spiegel-URL einspeisen. Redirects werden nicht verfolgt.

Der öffentliche IPC-Katalog enthält ausschließlich Komponenten-ID, Version,
Minecraft-Version, Loaderbindung, Größe und SHA-256. Vor der Projektion werden die
Signatur jedes enthaltenen Manifests und dessen Laufzeitkompatibilität im
Rust-Kern geprüft. Die UI sendet bei einem Komponentenwechsel nur eine Identität
aus diesem typisierten Katalog zurück; die produktive Downloadroute bleibt intern
abgeleitet. Ist der Provider nicht konfiguriert, wird vor jedem Katalogabruf mit
einer leeren Ergebnisliste fail-closed abgebrochen.

Nach dem Streaming werden Größe und SHA-256 vor der JAR-Inspektion geprüft. Der
Descriptor muss zum Loader passen (`fabric.mod.json` beziehungsweise
`META-INF/neoforge.mods.toml`) und dieselbe Mod-ID wie das signierte Manifest
enthalten. Unbekannte Manifest- und Katalogfelder werden abgewiesen.

## Pfad- und Archivschutz

Laufzeitziele und JAR-Einträge durchlaufen die zentrale Windows-sichere
Pfadnormalisierung. Dadurch bleiben die Schutzregeln aus Phase 1 und 4 erhalten:

- kein absoluter Pfad und keine Traversal,
- keine leeren oder mehrdeutigen Segmente,
- keine gemischten oder nicht kanonischen Separatoren,
- keine Alternate Data Streams,
- keine Windows-Sondernamen,
- keine Unicode-/Case-Kollisionen,
- keine Symlinks, Junctions oder Reparse Points,
- keine vorhandenen Hardlinks,
- keine Sonderdateien.

Für JARs und native Bibliotheken gelten zusätzlich feste Grenzen für Anzahl,
komprimierte und entpackte Größe, Einzeldateigröße und Kompressionsverhältnis.
Native Bibliotheken werden erst nach vollständiger Vorvalidierung in ein neues,
startbezogenes Verzeichnis extrahiert.

Die früheren globalen Launcher-/Installerquellen und die gebündelten
Default-Mod-JARs wurden entfernt. Damit existiert kein alternativer Legacy-Pfad,
der die Revisions-, Provider- oder JAR-Prüfungen umgehen kann.

## Revisions- und Startintegrität

Die neue Runtime-Projektion wird zusammen mit dem aktiven Revisionszeiger in einer
SQLite-Transaktion geschrieben. Der Operationsplan bindet sowohl die neue als auch
die vorherige Projektion. Eine Kompensation stellt Revisionszeiger und vorherige
Projektion ebenfalls gemeinsam wieder her. Trigger und Fremdschlüssel verhindern
weiterhin profilfremde oder nicht festgeschriebene Referenzen.

Die Classpath-Reihenfolge ist sicherheitsrelevant und wird aus den kontrollierten
Metadaten erhalten: Mojang-Bibliotheken zuerst, anschließend Loaderbibliotheken,
das Client-JAR zuletzt. Eine stabile Deduplizierung entfernt nur spätere
Wiederholungen; eine alphabetische Neuordnung findet nicht statt.

Kontrolliertes System-Java akzeptiert nur Kandidaten aus begrenzten bekannten
Installationsorten und validiert die vollständige Verzeichniskette. Managed Java
bleibt ohne hashgebundene Bezugsquelle in der UI deaktiviert; fehlt das
kontrollierte Executable, lehnt auch der Rust-Kern die Policy als unverfügbar ab.
Ein vorhandenes Executable wird mindestens auf Pfad, Version und Architektur
geprüft, ersetzt aber keinen noch fehlenden Liefernachweis. Dadurch kann keine
Revision erfolgreich installiert werden, deren Java-Policy lokal nicht
aufgelöst wurde.

Unter Windows überschreitet ein Spielprozess die Spawn-Grenze nur suspendiert.
Der Rust-Kern konfiguriert ein eigenes Kill-on-close-Job-Object, ordnet den
Prozess zu, verifiziert die Mitgliedschaft und setzt erst dann exakt den primären
Thread fort. Scheitert einer dieser Schritte, wird der suspendierte Prozess
beendet. Gezieltes Stoppen terminiert das Job Object und wartet anschließend den
Wurzelprozess ein; ein Regressionstest beobachtet und beendet auch den enthaltenen
Kindprozess.

## NeoForge-Ausführungsgrenze

Der produktive NeoForge-Pfad endet nicht mehr an einem ungeprüften Installer. Er
lädt und verifiziert den gebundenen Installer und erzeugt daraus offline einen
hashgebundenen Installationsplan. ZIP-, Maven-, Metadaten-, Daten-, Pfad-,
Platzhalter- und Prozessorgrenzen werden vor jeder denkbaren Ausführung geprüft.

Die Ausführung bleibt jedoch gesperrt, wenn erwartete Prozessoroutputs keine
vertrauenswürdigen Hashes besitzen, ein Prozessor Netzwerkzugriff anfordert oder
die bereitgestellte Sandbox nicht zugleich Netzwerkfreiheit, Shellfreiheit,
begrenzte Ausgabe, Prozessbaum-Timeout und exakte Schreibziele garantiert. Für die
aktuell untersuchte reale Installerform trifft mindestens eine dieser Bedingungen
zu. Ein solcher Plan wird analysiert, aber nicht ausgeführt oder als installierte
Laufzeit festgeschrieben.

## Fail-closed Capabilities

Eine Capability ist nur nutzbar, wenn der Rust-Kern exakt `state = available` und
einen leeren `reasonCode` liefert. Browserfallbacks melden ausschließlich
`unconfigured`. Die UI muss NeoForge, S9Lab-Komponenten und jede nicht vollständig
bereitgestellte verwaltete Java-Quelle deaktivieren.

Aktuell fehlen offizielle S9Lab-Produktionswerte. Deshalb ist
`s9lab.components` korrekt `unconfigured`. Es darf weder eine Testdomain noch ein
Testschlüssel in einem Produktbuild als verfügbar erscheinen.

## Verbleibende Risiken und Blocker

- Managed Java besitzt noch keinen dokumentierten, hashgebundenen
  Download-/Aktivierungspfad und bleibt deshalb unkonfiguriert.
- NeoForge-Prozessorausführung besitzt noch keinen für die geforderten Garantien
  geeigneten Produktionspfad; sie bleibt fail-closed.
- Der S9Lab-Komponentenprovider bleibt ohne offizielle Produktions-Origin und
  offiziellen öffentlichen Schlüssel unkonfiguriert.
- Reale authentifizierte End-to-End-Starts sind noch nicht vollständig
  nachgewiesen.

Diese Punkte sind keine akzeptierten Sicherheitsausnahmen. Nicht verfügbare Pfade
bleiben sichtbar deaktiviert oder liefern einen typisierten Fehler.
