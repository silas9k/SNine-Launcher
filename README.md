# SNine Launcher

<img src="src/assets/snine-logo.png" alt="SNine Launcher Logo" width="96" height="96">

Moderner Desktop-Launcher für Minecraft auf Basis von **Tauri 2**, **Rust**, **React** und **TypeScript**. Die sichtbare Produktmarke ist **SNine Launcher**; bestehende interne S9Lab-Formate und Datenpfade bleiben aus Kompatibilitätsgründen stabil.

Projekt: [github.com/silas9k/S9Lab-Launcher](https://github.com/silas9k/S9Lab-Launcher)

## Aktueller Entwicklungsstand

Dieser Quellstand enthält den lokal maximal erreichbaren Entwicklungsstand der
**Phasen 1 bis 10** des verbindlichen S9Lab-Masterplans v1.1. Die Produktversion
bleibt 1.0.8:

- Phase-1-Plattform-, Speicher- und Operationskern bleibt erhalten.
- Eine eigenständige, kompakte SNine-App-Shell bündelt Profile, Inhalte, Laufzeit, Updates, Accounts und Cosmetics.
- Deutsch und Englisch werden über ein typisiertes Übersetzungssystem bereitgestellt.
- System-, Hell-, Dunkel- und Hochkontrastmodus verwenden dieselben Design-Tokens.
- Akzentfarbe, Dichte, Navigation, Hintergrund und reduzierte Animationen werden atomar über Rust gespeichert.
- Responsive Layout-, Tastatur-, Dialog-, Accessibility-, visuelle und Browser-Performance-Prüfungen sind vorhanden.
- Phase 3 ergänzt Microsoft Device Code Login, Java-Besitzprüfung, OS-Credential-Store, sichere Mehrfachaccountverwaltung und eine fail-closed Offline-Richtlinie.
- Phase 4 ergänzt isolierte Profile, eigenständige Kopien veränderlicher Daten, atomare Manifest-/Lock-Revisionen, Archiv/Papierkorb sowie konservative Cache-Quarantäne.
- Phase 5 ergänzt verifizierte Vanilla-, Fabric- und NeoForge-Runtimes, kontrollierte Java-Auflösung, Starts und die modulare fail-closed S9Lab-Client-Komponente.
- Phase 6 ergänzt Modrinth-Inhalte, sichere lokale Dateien und MRPACKs, Resolver, reproduzierbare Locks sowie das versionierte S9Lab-Profilformat.
- Phase 7 ergänzt getrennte Updatekanäle, Vorschau, Richtlinien, Backups, Restore und atomaren Profilrollback.
- Phase 8 enthält die lokale, datenminimierte Zwei-Geräte-Sync-/Konfliktarchitektur; ohne freigegebenes S9Lab-Backend bleibt die Verknüpfung sichtbar deaktiviert.
- Phase 9 enthält den integrierten boxlosen 3D-Spieler mit lokalen Skin-/Cape-/Wings-/Halo-Assets, Animation, Rotation, Kamera und Fallback.
- Phase 10 ergänzt den vollständigen Windows-Cleanroom, einen isolierten NSIS-Lifecycle, SHA-gebundene Delta-/Quellpakete und Release-/Rollback-Nachweise.

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
.\VERIFY-PHASE10-WINDOWS.ps1 `
  -ZipPath ".\S9Lab-Launcher-v1.0.8-final-source.zip" `
  -ExpectedSha256 "<SHA-256 aus unabhängigem Übergabekanal>" `
  -EvidencePath ".\phase10-evidence.json" `
  -ExerciseInstallerLifecycle
```

Das Skript prüft das neu entpackte Quellpaket vor der Installation von
Abhängigkeiten, installiert ausschließlich über die öffentliche npm-Registry,
führt alle Frontend-, Browser-, Accessibility-, Performance-, Rust- und
Windows-Regressionen aus, baut den unsignierten NSIS-Kandidaten und kann dessen
Current-User-Installations-, Wartungs- und Deinstallationspfad isoliert prüfen.

Eine Windows-Freigabe wird erst nach einem vollständig grünen Lauf auf einem echten Windows-MSVC-System erteilt. Das ältere Phase-2-Skript bleibt ausschließlich für den eingefrorenen Phase-2-Stand erhalten.

## Dokumentation

Phasenspezifische Architektur-, Sicherheits-, Migrations-, Blocker- und
Testnachweise liegen unter `docs/`. Für die Übergabe sind insbesondere
`PHASE-10-WINDOWS-TEST-MATRIX.md`, `PHASE-10-INSTALL-UPGRADE-UNINSTALL.md`,
`PHASE-10-RELEASE-AND-ROLLBACK.md` und `FINAL-COMPLETION-REPORT.md` maßgeblich.

Die öffentliche Produktversion bleibt **1.0.8**.
