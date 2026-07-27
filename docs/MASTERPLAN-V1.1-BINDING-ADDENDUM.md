# S9Lab-Masterplan v1.1 – verbindlicher Nachtrag

Status: freigegeben. Dieser Nachtrag ergänzt Version 1.1 und ist bei Widersprüchen maßgeblich.

## Entwicklungsphasen, Dauer, Abhängigkeiten und kritischer Pfad

| Phase | Geschätzte Dauer | Abhängigkeiten | Vorführbares Ergebnis | Freigabe-Gate | Kritischer Pfad |
|---|---:|---|---|---|---|
| 0 – Sicherheitsbereinigung, Vertrauenswechsel und grüner Build | 2–3 Wochen | keine | Bereinigter, reproduzierbarer und lokal baubarer Sicherheitsstand ohne aktive kompromittierte Updatekette | Frontend- und Rust-Gates grün; keine privaten Schlüssel; keine produktiven HTTP-Endpunkte; alter Updater deaktiviert; Vertrauenswechsel dokumentiert | Ja – blockiert alle weiteren Phasen |
| 1 – Plattform-, Speicher- und Operationskern | 3–4 Wochen | Phase 0 | Transaktionale Demo mit simuliertem Abbruch und konsistenter Wiederherstellung | Datenhoheit, IPC, Journaling, Staging, Hash- und Pfadprüfungen bestanden | Ja |
| 2 – Designsystem, Navigation, i18n und Performance-Harness | 3–5 Wochen | Phase 0; Teile parallel zu Phase 1 | Vollständige App-Shell in Deutsch/Englisch, Hell/Dunkel/Hochkontrast | i18n-, Accessibility-, Responsive- und Startzeit-Gates bestanden | Teilweise – UI kann parallel entstehen, Integration benötigt Phase 1 |
| 3 – Sichere Microsoft-Grundauthentifizierung | 3–5 Wochen | Phasen 0 und 1 | Device-Code-Login, Besitzprüfung, sichere Speicherung, Abmeldung | Keine Tokens in UI/SQLite/Logs; Besitzprüfung und Widerruf getestet | Ja – muss vor echten Minecraft-Starts fertig sein |
| 4 – Profile, Instanzen und unveränderlicher Cache | 4–6 Wochen | Phase 1; UI aus Phase 2 | Isolierte Profile, Revisionen, Archiv/Papierkorb, konservative Cache-Bereinigung | Keine gemeinsam veränderlichen Dateien; Manifest/Lock atomar; 1.000 Profile performant | Ja |
| 5 – Minecraft, Java, Modloader und minimal signierter S9Lab-Client-Resolver | 6–9 Wochen | Phasen 3 und 4; UI-Integration aus Phase 2 | Authentifizierter Start von Vanilla, Fabric und NeoForge; modularer S9Lab Client wird kontrolliert aufgelöst, heruntergeladen und geprüft | Loader-Testmatrix grün; Client-Artefakt nur über kontrollierten Provider, HTTPS, Größenlimit, Hash und gültige Signatur; kein vollständiges Update-System erforderlich | Ja – Abschluss des MVP |
| 6 – Modrinth, Modpack-Editor und Profilformat | 5–7 Wochen | Phasen 4 und 5 | Reproduzierbarer Import/Export mit Abhängigkeits- und Konfliktauflösung | Provider-, Import-, Hash-, ZIP-Bomb- und Performance-Gates bestanden | Ja für stabile Version |
| 7 – Vollständige Updates, Backups, Rollback und Migration | 5–7 Wochen | Phasen 5 und 6 | Updatevorschau, Wiederherstellungspunkt, Rollback und geführte Migration | Atomare Update-/Migrationsfehlerfälle bestanden; Signatur- und Authenticode-Gates grün | Ja |
| 8 – S9Lab-Konto und Cloud-Synchronisierung | 4–6 Wochen | Phasen 4, 6 und Backend-APIs | Zwei-Geräte-Sync mit Drei-Wege-Konfliktauflösung | Keine lokalen/geheimen Dateien im Sync; Revisions- und Sitzungs-Gates bestanden | Parallel zu Teilen von Phase 7, aber erforderlich für stabile Cloud-Funktion |
| 9 – 3D-Spieler und bereits besessene Cosmetics | 4–6 Wochen | Phase 2, Accountdaten aus Phase 3/8, Preview-Assets | Skin, Nametag, Icon, Cape, Wings, Halo und Animationen ohne Zoom | Rendering-, Fallback-, Accessibility- und Performance-Gates bestanden | Parallel möglich; stabile Version benötigt fertige Assets |
| 10 – Windows-Stabilisierung und öffentlicher Release | 4–6 Wochen | Phasen 0–9 im freigegebenen Releaseumfang | Signierter Release Candidate auf endgültiger Windows-Testmatrix | Keine kritischen Sicherheits-/Datenverlustfehler; Release-, Performance- und Signatur-Gates bestanden | Ja |

Der voraussichtliche kritische Pfad lautet: **0 → 1 → 3 → 4 → 5 → 6 → 7 → 10**. Phase 2 läuft teilweise parallel zu Phase 1; Phase 8 und Phase 9 können nach ihren jeweiligen Voraussetzungen parallelisiert werden.

## Minimaler sicherer S9Lab-Client-Mechanismus in Phase 5

Phase 5 enthält bereits einen kleinen, eigenständigen Vertrauensmechanismus für die modulare S9Lab-Client-Komponente. Er ist nicht vom vollständigen Update-System der Phase 7 abhängig.

Erforderlich sind mindestens:

- kontrollierter S9Lab-Provider ohne importierbare Raw-URLs,
- ausschließlich HTTPS über freigegebene Domains,
- signiertes Komponentenmanifest mit Komponenten-ID, Version, Minecraft-Version, Loader, Dateigröße und SHA-256,
- Verifikation über einen im Launcher verankerten, nicht kompromittierten öffentlichen Schlüssel,
- festes Größenlimit und Download nach Staging,
- Prüfung von Signatur, Hash, JAR-Grundformat und Zielpfad vor Aktivierung,
- atomarer Einbau in das Profil,
- verständliches Scheitern ohne Veränderung des bisherigen Profilzustands.

Phase 7 ergänzt diesen Mechanismus um Änderungsansicht, Updatekanäle, automatische Richtlinien, Wiederherstellungspunkte, Backups, vollständigen Rollback und Migration.

## Umgang mit kompromittierten Schlüsseln

Kompromittierte private Schlüssel müssen aus allen produktiven Systemen, Repositories, Build-Artefakten und normalen Backups entfernt werden. Sie dürfen nicht in Entwicklerarchiven oder gewöhnlichen Sicherungen verbleiben.

Falls eine Kopie zwingend für die Sicherheitsuntersuchung benötigt wird, ist nur eine einzelne Beweiskopie zulässig. Diese muss:

- außerhalb produktiver und normaler Backup-Systeme liegen,
- stark verschlüsselt sein,
- in einer streng isolierten Beweisablage verwahrt werden,
- durch rollenbasierte Zugriffe und Mehrpersonenfreigabe geschützt sein,
- mit Zugriffprotokoll, Aufbewahrungsfrist und dokumentiertem Löschtermin versehen sein,
- niemals für Signierung, Builds oder Tests verwendet werden.

Im lokalen Phase-0-Arbeitsstand werden keine Beweiskopien angelegt. Die unveränderte vom Nutzer bereitgestellte Eingabedatei bleibt außerhalb des bereinigten Ausgabeprojekts und wird nicht als produktives Artefakt weitergegeben.
