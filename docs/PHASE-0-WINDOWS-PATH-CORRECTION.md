# Phase 0 v1.1.1 – Windows-Pfadkorrektur

## Ursache

`_walk.mjs` verwendet `path.join` und liefert deshalb unter Windows Backslash-Pfade. Die Selbst-Ausschlüsse in `check-utf8.mjs` und `check-secrets.mjs` verglichen diese Werte mit fest codierten Forward-Slash-Pfaden.

## Änderungen

- Selbst-Ausschluss in `check-utf8.mjs` über `path.resolve`.
- Selbst-Ausschluss in `check-secrets.mjs` über `path.resolve`.
- Dokumentationsausschluss in `check-utf8.mjs` auf absolute, plattformunabhängige Pfadprüfung umgestellt.
- rekursive Pfadbildung in `check-security-config.mjs` auf `path.join` umgestellt.
- übrige MJS-Prüfskripte auf dieselbe Fehlerklasse geprüft; keine weiteren betroffenen Vergleiche gefunden.
- dedizierter Windows-CI-Workflow für `npm ci` und `npm test` ergänzt.

## Lokale Verifikation

- `npm ci`: erfolgreich
- `npm test`: erfolgreich
- `npm run build`: erfolgreich
- `npm audit --omit=dev --audit-level=critical`: 0 Schwachstellen
- Windows-Auflösungsäquivalenz mit `path.win32.resolve`: erfolgreich

Rust/Cargo und ein echter Windows-Runner stehen in dieser lokalen Linux-Umgebung nicht zur Verfügung. Der dedizierte GitHub-Actions-Workflow übernimmt die reale Windows-Ausführung nach Einchecken des Pakets.

Der Funktionsumfang des Launchers wurde nicht verändert. Phase 1 wurde nicht begonnen.
