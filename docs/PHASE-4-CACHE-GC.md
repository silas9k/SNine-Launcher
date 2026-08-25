# Phase 4 – Konservative Cache-Bereinigung

## Markierung

Eine Bereinigung sammelt Hashreferenzen aus:

- der SQLite-Referenztabelle,
- allen nicht terminalen Operationsplänen,
- aktiven, archivierten und im Papierkorb befindlichen Profilbäumen einschließlich Manifesten und Locks,
- Backups beziehungsweise Recovery-Dateien,
- vollständigem Operations-Staging.

Symlinks oder Sonderdateien in einer Autoritätswurzel brechen die Bereinigung ab. Unverständliche Daten werden nicht gelöscht; zufällig erkannte gültige Hashes führen höchstens zu konservativem Behalten.

## Sweep und Revalidierung

Vor der Kandidatenbildung werden zwei vollständige Markierungen durchgeführt. Unmittelbar vor jedem Move wird die Referenzmenge erneut gelesen. Cachemutationen sind zusätzlich pro Prozess serialisiert. Erst ein in allen Prüfungen unreferenziertes und erneut integritätsgeprüftes Objekt wird atomar unter `cache/quarantine/sha256` verschoben und in derselben logischen Operation in SQLite als quarantänisiert markiert.

Scheitert die SQLite-Aktualisierung, wird der Dateimove kompensiert. Wird später eine Referenz auf ein quarantänisiertes Objekt gefunden, wird es vor einem weiteren Sweep verifiziert und reaktiviert.

## Keine endgültige Löschung

Die Phase-4-GC-API enthält nur Vorschau und Quarantäne. Es gibt weder Sofortlöschung noch Ablauf-Sweep. Eine spätere Phase darf eine endgültige Löschung erst nach verbindlicher Sicherheitsfrist, erneuter vollständiger Markierung und eigenen Recovery-/Fehler-Injektionstests ergänzen.
