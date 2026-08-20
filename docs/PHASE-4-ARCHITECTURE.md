# Phase 4 – Architektur

## Umfang und Demo

Phase 4 implementiert Profile, isolierte Instanzdaten, atomare Profilrevisionen und einen unveränderlichen, inhaltsadressierten Cache. Die Demo umfasst Erstellen, sicheres Duplizieren, Favorisieren, Archivieren, Papierkorb, Wiederherstellen, Suche, Startseiten-Auswahl und eine Speicherübersicht mit ausschließlich wiederherstellbarer Cache-Quarantäne.

Phase 5 ist ausdrücklich nicht enthalten: Es gibt keine Vanilla-, Fabric- oder NeoForge-Auflösung, keinen echten Minecraft-Start und keinen S9Lab-Client-Resolver.

## Datenhoheit

| Information | Autorität |
|---|---|
| Lebenszyklus, Anzeigename, Favorit, aktive Revision, Account-Zuordnung | SQLite |
| portable Profilabsicht und Isolationsrichtlinie | versioniertes `manifest.json` |
| aufgelöste Revision, Manifest-Hash und Cacheprojektion | versioniertes `lock.json` |
| veränderliche Instanz-, Konfigurations- und Weltdaten | ausschließlich Profilverzeichnis |
| unveränderliche verifizierte Objekte | Cache nach SHA-256 |

IPC-Vertrag 4 ergänzt ausschließlich typisierte Phase-4-Befehle. Bestehende Vertragsteile bleiben rückwärtskompatibel.

## Profilanlage und Revision

Ein neues Profil erhält eine kollisionsfreie technische ID, SQLite-Metadaten und eine vollständig getrennte Verzeichnisstruktur. Manifest und Lock werden in der vorhandenen Operationsengine gestaged, gehasht, gegeneinander geprüft, atomar als Revisionsordner verschoben und gemeinsam mit dem aktiven Revisionszeiger in SQLite aktiviert.

Fehler vor oder während der ersten Aktivierung rollen Staging, Revision, Profilprojektion und neu angelegte Profildateien zurück. Bestehende Profile werden dabei nicht verändert.

## Sichere Duplizierung

Die veränderlichen Bereiche `mods`, `config`, `saves`, Ressourcen-/Shader-/Datenpakete, Screenshots, Logs und Crashreports werden rekursiv als neue Dateien kopiert. Hardlinks sind verboten; Symlinks, Reparse Points und Sonderdateien führen fail-closed zum Abbruch. Konfigurationen und Welten eines Duplikats können daher das Ausgangsprofil nicht verändern.

Der zentrale Cache bleibt unveränderlich. Eine Projektion in ein Profil verwendet eine verifizierte normale Kopie; ein zukünftiges Copy-on-Write-Verfahren wäre nur nach eigener Plattformverifikation zulässig.

## Lebenszyklus

Archiv und Papierkorb sind logische SQLite-Zustände. Das Verschieben in den Papierkorb löscht keine Dateien und entfernt keine Cache-Referenzen. Wiederherstellung führt in den Zustand zurück, aus dem das Profil gelöscht wurde; ein zweiter Restore-Schritt hebt ein Archiv auf.

## Performance

Die Bibliotheksabfrage besitzt passende SQLite-Indizes und einen Rust-Test mit 1.000 Profilprojektionen. Der verbindliche native Grenzwert wird im Windows-Gate geprüft; Browsernavigation und responsive Bibliothekslayouts werden zusätzlich im bestehenden Performance- und Browser-Harness geprüft.
