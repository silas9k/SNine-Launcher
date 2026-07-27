# Phase 0 – Bestandsaufnahme und Ausführungsplan

## Bestandsaufnahme vor Änderungen

- Frontend-Build reproduzierbar fehlgeschlagen: `SkinStudio.tsx` verweist auf den nicht vorhandenen IPC-Command `uploadPlayerSkin`.
- Fünf private Updater-Schlüssel lagen als ungetrackte, ignorierte Dateien im Projektordner.
- Der Tauri-Updater war mit einem als kompromittiert zu behandelnden öffentlichen Schlüssel und einem aktiven Release-Endpunkt konfiguriert.
- Der Launcher enthielt zusätzlich einen nicht signierten Remote-Updater für die S9Lab-Client-JAR.
- Backend-Standard und CSP erlaubten die produktive HTTP-IP `31.70.89.55:25614`; Shop, Freunde, Coins und Secret-Reward nutzten direkte Frontend-/Backend-Aufrufe.
- Mehrere sichtbare Texte und Rust-Fehlermeldungen enthielten Mojibake (`Ã`, `â€¦`).
- Versionsangaben waren in Frontend und Rust mehrfach fest codiert und widersprüchlich.
- Google Fonts wurden zur Laufzeit geladen.
- Das bereitgestellte Projekt enthielt bereits zahlreiche lokale, nicht committete Änderungen. Phase 0 arbeitet auf diesem Zustand und schreibt keine Git-Historie um.
- In der Ausführungsumgebung war Rust/Cargo nicht installiert; eine Installation war wegen fehlender Netzwerkauflösung nicht möglich.

## Lokale Maßnahmen

1. Originalarbeitsstand separat sichern; keine Git-Historie verändern.
2. Private Schlüssel aus dem bereinigten Projekt entfernen und Ignore-/Secret-Prüfregeln verschärfen.
3. Kompromittierten Launcher-Updater vollständig deaktivieren und alte Aktivierungs-/Signierskripte entfernen.
4. Nicht signierten S9Lab-Client-Remote-Updater deaktivieren; bis Phase 5 nur die gebündelte lokale Komponente verwenden.
5. Unsichere HTTP-Backendintegration und nicht beschlossene Freunde-/Shop-/Coins-Funktionen aus dem aktiven Build entfernen.
6. CSP härten und Laufzeit-Google-Fonts entfernen.
7. Frontend-Buildfehler beseitigen, ohne einen Dummy-IPC-Command einzuführen.
8. Mojibake korrigieren und automatische UTF-8-Prüfung ergänzen.
9. Versionswerte zentralisieren und automatische Versionsprüfung ergänzen.
10. Release-Workflow in einen reinen Verifikationsworkflow ohne Veröffentlichung umwandeln.
11. Frontend-Build, statische Sicherheitsprüfungen, Tests und – soweit lokal möglich – Rust-Prüfungen ausführen.
12. Bereinigtes Projekt ohne `.git`, IDE-Dateien, Buildausgaben, private Schlüssel und normale Backups paketieren.

## Externe Maßnahmen – nicht ohne ausdrückliche Freigabe

- Alte Schlüssel in produktiven Systemen, CI-Secrets und normalen Backups widerrufen beziehungsweise löschen.
- Sicherheitsuntersuchung und gegebenenfalls isolierte Beweisablage einrichten.
- Neuen Offline-Root-, delegierten Release- und Widerrufsprozess erzeugen.
- Windows-Code-Signing-Zertifikat beschaffen und geschützten Signierdienst einrichten.
- Bereits installierte Launcher inventarisieren und sicheren manuellen Vertrauenswechsel planen.
- Produktive HTTPS-Domain und Backend-Zertifikate bereitstellen.
- Produktive Updatefeeds, GitHub Releases oder Dienste ändern.
- Git-Historie oder externe Repositories bereinigen.
- Artefakte veröffentlichen oder Sitzungen widerrufen.
