# Phase 1 – SQLite-Schema und Migrationen

## Datenbank

Pfad: `data/launcher.db`

Aktueller Phase-1-Schemastand: **3**

## Migrationen

| Version | Name | Inhalt |
|---:|---|---|
| 1 | `phase1_core_schema` | Profile, Revisionen, Operationen, Journal, Cache-Blobs und Cache-Referenzen |
| 2 | `phase1_indexes_and_guards` | Indizes und Guard für aktive Revisionen |
| 3 | `phase1_state_machine_and_revision_insert_guard` | Insert-Guard und erlaubte Operationsübergänge |

Die Tabelle `schema_migrations` speichert Version, Namen und Anwendungszeitpunkt. Jede Migration läuft in einer SQLite-Transaktion. Eine Datenbank mit neuerer, unbekannter Version wird abgelehnt.

## Tabellen

### profiles

- `id` – technische Profil-ID
- `lifecycle_state` – `active`, `archived` oder `trash`
- `active_revision_id` – einzig aktiver Revisionszeiger
- Erstellungs-, Änderungs-, Archiv- und Papierkorbzeitpunkte

### profile_revisions

- Revisions-ID, Profil-ID und erzeugende Operation
- `manifest_sha256` und `lock_sha256`
- kontrollierte relative Manifest- und Lockpfade
- Status `committed` oder `invalidated`

Der Trigger `profile_active_revision_belongs_to_profile` verhindert, dass ein Profil auf eine fremde oder nicht committed Revision zeigt.

### operations

- Operations-ID und Typ
- optionale Profil-ID
- expliziter Zustand
- serialisierter Änderungsplan
- kontrollierter relativer Staging-Pfad
- vorherige und Zielrevision
- Zeitpunkte
- stabiler Fehlercode und JSON-Parameter

### operation_journal

- streng steigende Sequenz je Operation
- Schritt und Status
- technische Details
- passende Gegenoperation

### cache_blobs

- SHA-256 als Primärschlüssel
- unveränderliche Größe und relativer Pfad
- Zustand `staged`, `verified` oder `quarantined`

Eine bestehende Prüfsumme darf nicht mit abweichender Größe oder abweichendem Pfad neu registriert werden.

### cache_references

Grundlage für spätere konservative Referenzermittlung:

- Blob-Prüfsumme
- Besitzerart
- Besitzer-ID

## Aktivierung einer Revision

`Storage::activate_revision` führt in einer Transaktion aus:

1. aktuellen Profilzeiger lesen,
2. erwartete Vorgängerrevision prüfen,
3. Revisionsdatensatz mit Manifest- und Lock-Hash einfügen,
4. aktiven Profilzeiger auf dieselbe Revision setzen.

Bei jedem Fehler wird die komplette Transaktion zurückgerollt.

## Wiederherstellung

- Eine beschädigte oder zu neue Datenbank wird nicht still überschrieben.
- Manifest und Lockdatei werden nicht unabhängig aktiviert.
- Bei Recovery wird der aktive Zeiger nur auf eine validierte Revision gesetzt beziehungsweise auf die Vorgängerrevision zurückgestellt.
- Fehlende Schlüsselspeichereinträge werden nicht aus SQLite rekonstruiert; Phase 1 speichert dort keine Geheimnisse.
