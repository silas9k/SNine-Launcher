# Phase 8 – Sicherheit

Status: 2026-08-11.

## Fail-closed Provider

`CloudSyncProvider` trennt den lokalen Sync-Kern von einer späteren
Produktionsanbindung. Die ausgelieferte Implementierung besitzt keinen Endpoint,
keinen HTTP-Fallback, kein eingebettetes Token und keine lokale Ersatzsitzung.
Alle mutierenden Provideroperationen schlagen kontrolliert fehl.

## Datenminimierung

Der synchronisierbare Payload ist opt-in durch feste Rust-Felder. Er kann keine
Token, Accountobjekte, Welten, Logs, Dateipfade, Java-Pfade oder frei gewählten
Dateien serialisieren. Gelöschte Profile werden nicht aufgenommen. Sortierung,
kanonisches JSON und SHA-256 machen die lokale Revisionsbindung reproduzierbar.

## Konflikt- und Sitzungsgrenzen

Konflikte werden nicht nach „letzter Schreibvorgang gewinnt“ verschluckt. Jede
beidseitige Änderung verlangt genau eine lokale oder entfernte Auswahl. Da keine
Produktionsanbindung vorliegt, werden weder Cookies noch Sitzungen angelegt. Ein
späterer Provider muss OS-gebundene sichere Sitzungen, Ablauf, Widerruf,
Gerätebindung und Replay-Schutz separat nachweisen.

## Automatische Guards

Der Phase-8-Guard prüft Vertrag und Registrierung, geschlossenen Payload,
Provider-Sperre, Zwei-Geräte-Limit, Konfliktauflösung und das Fehlen erfundener
Netzendpunkte. Rust-Tests decken Merge, unvollständige Auswahl, Providerfehler
und secret-freie Serialisierung ab.
