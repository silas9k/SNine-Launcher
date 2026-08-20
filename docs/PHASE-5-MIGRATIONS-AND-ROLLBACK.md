# Phase 5 – Migration und Rollback

Status: aktueller lokaler Implementierungsstand; kein automatischer
Downgrade-Nachweis.

## SQLite-Schema v6

Migration 6 `phase5_profile_runtime_projection` ergänzt ausschließlich die
Abfrageprojektion `profile_runtime_projection` und zugehörige Indizes und Trigger.
Sie speichert:

- Profil- und aktive Revisions-ID,
- Minecraft-Version,
- Loaderart und aufgelöste Loader-Version,
- optionale Komponenten-ID und -Version,
- Installationszustand,
- Aktualisierungszeitpunkt.

Die Migration erfindet für bestehende Phase-4-Profile keine Laufzeitkonfiguration.
Ein migriertes Profil bleibt `not-configured`, bis eine echte Phase-5-Operation eine
verifizierte Revision aktiviert.

Fremdschlüssel und Trigger erzwingen, dass eine Projektion nur eine festgeschriebene,
zum selben Profil gehörende Revision referenziert. Wird die Revision ungültig,
entfernt ein Trigger die abgeleitete Projektion.

## Profilformat v2

Phase 5 führt `site.s9lab.profile` und `site.s9lab.profile-lock` mit
`formatVersion: 2` ein. Das Manifest enthält Laufzeitabsicht,
Komponentenauswahl und Isolationsrichtlinie. Das Lock bindet:

- Profil- und Revisions-ID,
- SHA-256 des kanonischen Manifests,
- vollständig aufgelöste Laufzeitobjekte,
- Startkonfiguration,
- sortierte und deduplizierte Cacheblobs.

Account-ID, Anzeigename, Lineage, Tokens und lokale absolute Pfade gehören nicht in
das portable Manifest oder Lock.

## Operationsrollback

Installieren, Reparieren und Komponentenwechsel verwenden denselben
Staging-/Verifikations-/Commit-Ablauf:

1. Plan und vorherige aktive Revision erfassen.
2. Manifest, Lock und verifizierte Cachekopien in Operations-Staging schreiben.
3. alle gestagten Dateien erneut prüfen.
4. Revisionsordner atomar in das Profil verschieben.
5. aktive Revision und zugehörige Runtime-Projektion gemeinsam in einer
   SQLite-Transaktion umschalten.
6. aktive Dateien validieren und Cache-Referenzen binden.
7. Staging entfernen und Operation abschließen.

Ein Fehler nach dem Move kompensiert in umgekehrter Reihenfolge: vorherige aktive
Revision und deren vorherige Runtime-Projektion gemeinsam wiederherstellen, neue
Cache-Referenzen entfernen, den neuen Revisionsordner entfernen, den zugehörigen
SQLite-Revisionsdatensatz als `invalidated` für das Audit erhalten, Staging
bereinigen und die Operation als `rolled-back` markieren. Veränderliche
Instanzdaten werden dabei nicht überschrieben oder gelöscht.

Der typisierte Operationsplan enthält dafür sowohl `runtimeProjection` als auch
`previousRuntimeProjection`. Vor der Aktivierung werden Profil-, Revisions- und
Projektionsidentität abgeglichen. Vor der Kompensation muss die neue Revision noch
der erwartete aktive Stand sein. Dateisystem-Move und SQLite sind keine gemeinsame
systemweite Transaktion; das persistente Journal und die idempotente umgekehrte
Kompensation bleiben deshalb Teil der Konsistenzgrenze.

## Reparatur

Reparatur verändert die aktive Revision nicht in-place. Sie liest das gebundene
Manifest und Lock, prüft die benötigten Cacheobjekte und erzeugt eine neue Revision.
Damit bleibt der vorherige Zustand bis zum Commit vollständig erhalten.

Die Java-Policy wird bereits vor dem Erzeugen und Aktivieren der neuen Revision
aufgelöst. Der derzeit nicht beschaffbare Managed-Java-Pfad kann daher keine
scheinbar installierte Runtime-Projektion erzeugen.

## Komponenten- und Legacy-Übergang

Komponentenwechsel speichern nur die aus einem verifizierten typisierten Katalog
gewählte Komponentenidentität in einer neuen Revision. URLs, Signaturen und lokale
absolute Pfade sind keine IPC- oder Migrationsfelder.

Die entfernten globalen Minecraft-Quellen, alten Command-/Typfassaden und
gebündelten Default-Mod-JARs besaßen keine weiterhin unterstützte
Profilautorität. Ihre Entfernung benötigt daher keine Datenmigration. Bestehende
veränderliche Profilinstanzen werden dadurch weder umgeschrieben noch gelöscht.

NeoForge-Installationspläne werden erst dann revisionswirksam, wenn der gesamte
Ausführungspfad sicher bereitsteht. Pläne mit fehlenden Output-Hashes,
Netzwerkbedarf oder unzureichender Sandbox scheitern vor Revisionsaktivierung und
erzeugen deshalb keinen Rollback-Sonderfall.

## Binärrollback und Backupgrenze

Eine Datenbank mit Schema v6 wird von einem älteren Phase-4-Binary absichtlich als
`storage_schema_too_new` abgewiesen. Phase 5 besitzt noch kein vollständiges
Backup-/Migrationsprodukt aus Phase 7. Für einen manuellen Binärrollback muss daher
vor dem ersten Start des v6-Binary eine konsistente Kopie des gesamten
Launcher-Datenverzeichnisses erstellt worden sein.

Ohne eine solche Sicherung darf nicht empfohlen werden, nur die ausführbare Datei
zurückzusetzen oder Schemazeilen manuell zu löschen. Ein SQL-Downgrade ist nicht
definiert.

## Nachweisgrenze

Die zuvor offene Lücke zwischen Revisionsaktivierung und separatem Schreiben von
`profile_runtime_projection` ist geschlossen: Beide Datenbankänderungen teilen
eine Transaktion, und der Rollback stellt beide zusammen wieder her. Ein gezielter
Regressionstest deckt Aktivierung und Kompensation ab.

Unverändert offen sind ein vollständiger Crash-/Recovery-Nachweis über sämtliche
Dateisystem- und SQLite-Grenzen sowie die später vorgesehene
Backup-/Restore-Produktstrecke. Diese offenen Nachweise dürfen nicht mit der nun
atomaren SQLite-Aktualisierung verwechselt werden.
