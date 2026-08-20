# Phase 4 – Migrationen und Verträge

## SQLite-Schema v5

Migration 5 `phase4_profile_lifecycle_and_cache_quarantine` ergänzt:

- `profile_metadata` für Anzeigename, Favorit, Verifikationszustand und vorherigen Papierkorbzustand,
- `profile_lineage` für eine optionale, referenziell abgesicherte Duplikationsquelle,
- `cache_quarantine` für Quarantänepfad, Zeitpunkt und die fest erzwungene Richtlinie `unconfigured`,
- Indizes für Bibliotheks-, Lebenszyklus- und Cacheabfragen.

Bestehende Profile werden während derselben Transaktion mit Metadaten und Lineage-Zeilen aufgefüllt. Der Vorwärtsmigrationstest erzeugt ein echtes Schema v4 mit vorhandenem Profil und prüft anschließend, dass v5 es vollständig lesen kann. Die Migrationen bleiben streng aufsteigend und werden transaktional angewendet.

## Manifest und Lock

`ProfileManifestV1` und `ProfileLockV1` besitzen jeweils ein Formatkennzeichen und `formatVersion: 1`. Das Lock bindet Profil-ID, Revisions-ID und SHA-256 des kanonischen Manifests. Cacheobjekte werden als sortierte und deduplizierte Hash-/Größenpaare festgehalten.

## IPC-Vertrag v4

Der gemeinsame Vertrag ergänzt Befehle für Liste, Anlage, Duplikation, Archiv, Papierkorb, Wiederherstellung, Favorit, GC-Vorschau und Quarantäne. Öffentliche Strukturen enthalten keine Authentifizierungsgeheimnisse. Fehler bleiben typisiert über `code`, `messageKey` und `params`.
