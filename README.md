# S9Lab Launcher

Desktop-Launcher für den S9Lab Minecraft Client auf Basis von **Tauri 2**, **Rust**, **React** und **TypeScript**.

## Aktueller Entwicklungsstand

Dieser Quellstand enthält den kumulativen Entwicklungsstand **Phase 3 und Phase 4** des verbindlichen S9Lab-Masterplans v1.1. Die Produktversion bleibt 1.0.8:

- Phase-1-Plattform-, Speicher- und Operationskern bleibt erhalten.
- Eine neue eigenständige S9Lab-App-Shell ersetzt die bisherigen parallelen Oberflächen.
- Deutsch und Englisch werden über ein typisiertes Übersetzungssystem bereitgestellt.
- System-, Hell-, Dunkel- und Hochkontrastmodus verwenden dieselben Design-Tokens.
- Akzentfarbe, Dichte, Navigation, Hintergrund und reduzierte Animationen werden atomar über Rust gespeichert.
- Responsive Layout-, Tastatur-, Dialog-, Accessibility-, visuelle und Browser-Performance-Prüfungen sind vorhanden.
- Die vorhandene Spielervorschau ist ohne Card, Rahmen oder Panel transparent in die zentrale Hintergrundfläche eingebettet; ein vollständiger 3D-Viewer ist weiterhin nicht enthalten.
- Phase 3 ergänzt Microsoft Device Code Login, Java-Besitzprüfung, OS-Credential-Store, sichere Mehrfachaccountverwaltung und eine fail-closed Offline-Richtlinie.
- Phase 4 ergänzt isolierte Profile, eigenständige Kopien veränderlicher Daten, atomare Manifest-/Lock-Revisionen, Archiv/Papierkorb sowie konservative Cache-Quarantäne.

Vanilla/Fabric/NeoForge-Installation, echte Minecraft-Starts, Modrinth, das neue Update-System und der vollständige Cosmetic-Viewer gehören ausdrücklich zu Phase 5 oder später. Phase 5 ist in diesem Stand nicht begonnen.

## Sicherheitsstatus

Dieser Stand ist **kein veröffentlichungsfähiger Release**.

- Der Remote-Updater bleibt deaktiviert.
- Kompromittierte Update-Schlüssel sind nicht enthalten und dürfen nicht wiederhergestellt werden.
- Produktive HTTP-Endpunkte sind verboten.
- Secrets dürfen nicht in Quellcode, SQLite, Logs oder Testdaten liegen.
- Externe CSS-, Font- und CDN-Laufzeitimporte sind verboten.
- Ein lokal erzeugter NSIS-Installer bleibt unsigniert und darf nicht verteilt werden.

## Lokale Frontend-Prüfung

```powershell
npm ci
npm test
npm run build
```

Zusätzliche Browserprüfungen mit lokal installiertem Edge, Chrome oder Chromium:

```powershell
$env:S9LAB_BROWSER_PATH = "C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
npm run test:browser
npm run test:performance
```

## Vollständige Windows-Nachprüfung

```powershell
.\VERIFY-PHASE3-PHASE4-WINDOWS.ps1 -ExpectedSha256 "<SHA-256 aus unabhängigem Übergabekanal>"
```

Das Skript prüft den kumulativen Phase-3-/Phase-4-Cleanroom einschließlich Frontend, Browser, Security-Gates, Migrationen und dreier normal paralleler Rust-Gesamtläufe. Erst danach wird ein lokaler, weiterhin unsignierter Tauri-/NSIS-Diagnosebuild erstellt.

Eine Windows-Freigabe wird erst nach einem vollständig grünen Lauf auf einem echten Windows-MSVC-System erteilt. Das ältere Phase-2-Skript bleibt ausschließlich für den eingefrorenen Phase-2-Stand erhalten.

## Dokumentation

- `docs/PHASE-1-WINDOWS-VERIFICATION-ADDENDUM.md`
- `docs/PHASE-2-IMPLEMENTATION-PLAN.md`
- `docs/PHASE-2-ARCHITECTURE.md`
- `docs/PHASE-2-DESIGN-SYSTEM.md`
- `docs/PHASE-2-NAVIGATION-AND-PAGES.md`
- `docs/PHASE-2-I18N.md`
- `docs/PHASE-2-ACCESSIBILITY.md`
- `docs/PHASE-2-PERFORMANCE.md`
- `docs/PHASE-2-TEST-MATRIX.md`
- `docs/PHASE-2-v1.0.3-CORRECTIONS.md`
- `docs/PHASE-2-v1.0.3-CHANGED-FILES.md`
- `docs/PHASE-3-ARCHITECTURE.md`
- `docs/PHASE-3-MIGRATIONS.md`
- `docs/PHASE-3-SECURITY.md`
- `docs/PHASE-3-TEST-MATRIX.md`
- `docs/PHASE-4-ARCHITECTURE.md`
- `docs/PHASE-4-MIGRATIONS.md`
- `docs/PHASE-4-SECURITY.md`
- `docs/PHASE-4-CACHE-GC.md`
- `docs/PHASE-4-TEST-MATRIX.md`
- `docs/PHASE-3-PHASE-4-WINDOWS-VERIFICATION.md`

Die öffentliche Produktversion bleibt **1.0.8**.
