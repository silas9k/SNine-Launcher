# Phase 6 – Migration, Profiltransfer und Rollback

Status: aktueller lokaler Implementierungsstand. Kein automatischer
Binärdowngrade und kein Vorgriff auf Phase 7.

## Keine neue SQLite-Migration

Phase 6 belässt `schema_version` auf v6. Inhaltsabsicht und -auflösung sind
portable Autorität der unveränderlichen Profilrevision und benötigen keine neue
SQLite-Tabelle. Operationsjournal, Profilrevisionen, Cachebindungen und
`profile_runtime_projection` werden weiterverwendet.

Eine Phase-6-Installation darf deshalb nicht allein aus SQLite rekonstruiert
werden. Maßgeblich bleiben `manifest.json`, `lock.json` und ihre Hashbindungen.

## Additive Revisionsfelder

Das `site.s9lab.profile`-Manifest v2 besitzt `desiredContent` mit leerem Default.
Ein unverändertes Phase-5-Profil wird damit weiterhin als Profil ohne Inhalte
gelesen.

Der optionale `content`-Block im `site.s9lab.profile-lock` v2 verwendet
`s9lab-content-lock` v1. `packMembers` und `overrides` besitzen leere Defaults.
Bei einem nicht leeren Block werden Runtimegleichheit, gewünschte Auswahl,
kanonische Reihenfolge, Packmitgliedschaften, Override-Seeds, Ziele, Größen und
`resolutionSha256` vollständig geprüft.

Alte Revisionen werden nicht in-place umgeschrieben. Die erste Inhaltsänderung
erzeugt eine neue Revision. Nicht freigegebene Zwischenstände früherer
Phase-6-Entwicklung sind keine zugesicherte Migrationsquelle; insbesondere wird
ein Lock mit einem nach heutiger kanonischer Nutzlast falschen Hash abgewiesen.

## Inhaltsrevision und Operationsrollback

Jede Inhaltsmutation erstellt Manifest und Lock neu, bindet Runtime-, Inhalts-
und Override-Blobs in `cacheBlobs` und materialisiert sie nach
`revisions/<Revision>/content/...`.

Der Ablauf ist:

1. vorherige aktive Revision und Runtime-Projektion erfassen,
2. kanonisches Manifest und Lock samt Hashes erzeugen,
3. alle Blobs als verifizierte Cachematerialisierungen planen,
4. neue Revision im Operations-Staging schreiben und prüfen,
5. Revisionsordner festschreiben,
6. Revisionszeiger und Runtime-Projektion gemeinsam umschalten und
7. Cachebindungen und Operationsabschluss festschreiben.

Ein Fehler kompensiert mit dem Phase-5-Mechanismus auf vorherige Revision und
Projektion. Veränderliche Welten, Einstellungen und Override-Dateien unter
Instanzhoheit liegen außerhalb dieses Revisionsrollbacks.

## MRPACK-Import und Packupdate

Lokaler Import und direkte Modrinth-Installation benutzen denselben
MRPACK-Aktivierungspfad. Beide führen vollständige Vorprüfung vor der
Pack-Cacheaktivierung aus. Der Container bleibt bis zur Gesamtvalidierung im
internen Operations-Staging und wird für Inventar und Extraktion erneut geöffnet.
Container, Mitgliedsdownloads und Override-Seeds werden erst nach erfolgreicher
Gesamtauflösung nochmals gehasht und aktiviert; die Profilrevision wird zuletzt
festgeschrieben.

Ein Modrinth-Packupdate wird als Austausch des bisherigen Packs desselben
Projekts modelliert. Die vorherige Revision bleibt bis zum Commit aktiv. Die neue
Revision enthält die neue Packversion, neu berechnete Mitgliedschaften,
Abhängigkeiten und Seeds. Konflikte mit manuellen oder durch andere Packs
besessenen Versionen brechen vor der Umschaltung ab.

Beim Entfernen eines Packs werden ausschließlich von ihm besessene
Mitgliedsauswahlen entfernt. Geteilte Mitgliedschaft überträgt den Besitz; manuell
verwaltete Inhalte bleiben erhalten.

## Startprojektion und Seed-Lebenszyklus

Der Projektionsmarker verwendet Formatversion 2, weil er normale verwaltete
Artefakte von Override-Seeds unter Instanzhoheit unterscheidet. Phase-5-Profile
besitzen keinen solchen Marker. Ein unbekannter oder alter Marker aus einem nicht
freigegebenen Zwischenstand wird fail-closed abgewiesen und nicht still migriert.

Normale Inhalte verwenden weiterhin eine Dateisystemtransaktion mit Staging,
Backup, Aktivierung und umgekehrtem Rollback. Ein Fehler nach Teilaktivierung
stellt die vorherigen normalen Dateien und den Marker wieder her.

Ein Override wird nur beim ersten fehlenden Ziel aus der verifizierten
Revisionsquelle gesät. Scheitert diese erste Aktivierung, wird auch der neu
angelegte Seed durch den Projektionsrollback entfernt. Nach erfolgreichem Säen
steht die Datei unter Instanzhoheit:

- lokale Änderungen werden nicht in ein späteres Revisionsbackup verschoben,
- bewusst gelöschte Seeds werden im unverändert aktiven Projektionszustand nicht
  bei jedem Start neu erzeugt; nach Deaktivierung und späterer Neuaktivierung
  darf ein weiterhin fehlendes Ziel erneut gesät werden,
- Packdeaktivierung, -update oder -entfernung löscht sie nicht und
- ein Inhaltsrollback setzt sie nicht auf die ursprünglichen Seedbytes zurück.

Diese Asymmetrie ist beabsichtigt: Die Revision reproduziert den angebotenen
Startwert; lokale Benutzerdaten bleiben unter Instanzhoheit.

## Duplizierung eines installierten Profils

Ein installiertes V2-Profil wird nicht als rohe Kopie seines Instanzbaums
dupliziert. Die Zielkopie erhält neue Profil- und Revisionsidentitäten, während
Runtime- und Inhaltszustand über ihre verifizierten Cachebindungen in eine neue
Revision übernommen werden. Der alte Projektionsmarker und unveränderliche
projizierte Inhalte werden aus der veränderlichen Baumkopie ausgeschlossen;
Welten, Einstellungen und bereits veränderte Override-Ziele bleiben erhalten.

Damit besitzt das Duplikat einen eigenständigen Rollback- und
Projektionslebenszyklus und verweist weder auf die aktive Revision noch auf den
Marker des Quellprofils.

## Portables Profilformat v1

`site.s9lab.profile-export` v1 besitzt eine eigene Formatversion. Ein unbekannter
Wert wird abgewiesen; implizite Auf- oder Abwärtsmigration ist nicht definiert.

Export nimmt Packcontainer, Packmitglieder im Lock und alle hashgebundenen
Override-Seeds auf. Gleiche Hashes werden dedupliziert. Der Export schreibt und
synchronisiert eine neue temporäre Geschwisterdatei und benennt sie erst danach
auf einen noch nicht vorhandenen Zielnamen um. In der Instanz geänderte oder
gelöschte Overrides werden nicht als neue portable Autorität übernommen; der
Export enthält die ursprünglichen Seeds der Revision.

Import ist ein Neuimport, kein Merge:

1. Archiv vollständig inventarisieren und validieren,
2. Manifest und erwartete Hashmenge binden,
3. Artefakte im neuen Operations-Staging streamen und prüfen,
4. verifizierte Cachekopien aktivieren,
5. neues Profil erzeugen,
6. deklarierte Laufzeit installieren und
7. den importierten Inhalts-Lock in einer neuen Revision aktivieren.

Die letzten Schritte sind keine einzelne systemweite Transaktion. Scheitert die
Laufzeit- oder Inhaltsaktivierung, wird kein bestehendes Profil überschrieben;
das neu angelegte Profil kann als sichtbarer, wiederherstellbarer Zwischenstand
verbleiben. Nicht referenzierte unveränderliche Cacheobjekte bleiben Kandidaten
für die konservative Cachebereinigung.

## Laufzeit- und Binärrollback

Reparatur und S9Lab-Komponentenwechsel erhalten Inhaltsabsicht und Lock. Ein
Wechsel von Minecraft-Version oder Loader bei vorhandenem Inhalt wird mit
`content_runtime_change_requires_resolution` abgewiesen.

Weil SQLite-Schema v6 bleibt, entsteht kein neuer SQL-Downgrade. Ein älteres
Binary kennt Phase-6-Inhalte, Packmitgliedschaften, Seeds und Marker dennoch nicht
als Produktfunktion und ist kein allgemeiner Rollbackpfad. Bis Phase 7 muss ein
manueller Binärrollback auf einer konsistenten Sicherung des gesamten
Launcher-Datenverzeichnisses beruhen; einzelne Manifest-, Lock-, Marker- oder
SQLite-Dateien dürfen nicht isoliert zurückkopiert werden.
