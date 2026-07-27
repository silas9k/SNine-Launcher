# Phase 4 – Sicherheit und Datenintegrität

## Isolationsgarantie

- Veränderliche Profildateien werden nie über Hardlinks geteilt.
- Konfigurationen und Welten werden als eigenständige Dateien kopiert.
- Pfadvalidierung, UTF-16-Budget, Windows-Sondernamen, ADS-, Symlink-, Reparse-Point- und vorhandene-Hardlink-Sperren aus Phase 1 bleiben aktiv.
- Quellbäume mit Symlinks oder Sonderdateien werden nicht teilweise übernommen.
- Cachematerialisierung prüft Größe und SHA-256 der erzeugten Profilkopie.

## Transaktionen und Recovery

Profilrevisionen verwenden die bestehende Zustandsmaschine und das Journal. Manifest und Lock werden vor Commit und nach Aktivierung geprüft. Ein kontrolliert injizierter Fehler nach dem Revisions-Move muss das neue Profil, seine Revision, sein Staging und seine veränderlichen Dateien vollständig entfernen. Die Operation bleibt als abgekoppelter, typisierter Rollbacknachweis erhalten.

## Cachekonkurrenz

Cacheaktivierung, Profilmaterialisierung, Vorschau und Quarantäne teilen pro Launcher-Prozess eine `Mutex`. Ein vergifteter Lock liefert stabil `cache_mutation_lock_poisoned`. Dadurch kann eine gleichzeitige Aktivierung nicht zwischen Markierung und Sweep durch eine zweite Mutationsoperation überholt werden.

## Bewusst fail-closed

Die endgültige Quarantänefrist ist im Masterplan noch nicht verbindlich festgelegt. Deshalb erzwingt SQLite ausschließlich `deletion_policy = 'unconfigured'`; es existiert kein produktiver Pfad zur dauerhaften Löschung. Auch die Benutzeroberfläche bietet nur eine bestätigte, wiederherstellbare Quarantäne an.

Phase 5 oder spätere laufende Instanzen, Imports und Updates existieren in diesem Stand nicht. Bevor solche Konsumenten eingeführt werden, müssen sie Cache-Referenzen als eigene Autorität registrieren; Phase 4 erfindet dafür keine vorgezogenen Fachfunktion.
