# Phase 2 – Architekturübersicht

## Ziel

Phase 2 ersetzt die alte, fragmentierte Präsentationsschicht durch eine einzige S9Lab-App-Shell. Der Phase-1-Kern für Plattformpfade, SQLite, Operationen, Pfadsicherheit, Downloads und Cache bleibt erhalten. Es werden keine Fachfunktionen späterer Phasen vorgezogen.

## Frontend-Schichten

```text
App
├── AppErrorBoundary
├── I18nProvider
├── ShellContent
│   ├── TitleBar
│   ├── Navigation
│   ├── Page Region
│   ├── TaskCenter
│   ├── Toasts
│   └── ConfirmDialog
├── Shell Store
├── Theme/Accent Resolver
└── Typed IPC Client
```

### `src/app`

- `shellStore.ts`: ausschließlich Shell-Zustand und Shell-Aktionen.
- `ErrorBoundary.tsx`: kontrollierter fataler UI-Zustand.

Der Store enthält nur aktive Seite, Shell-Einstellungen, Dialoge, Benachrichtigungen sowie Task-Center- und Navigationszustand. Profile, Accounts, Inhalte und andere dauerhafte Fachdaten werden nicht im Store oder in Browser-Speichern persistiert.

### `src/components/shell`

- eigene Titelleiste
- primäre und sekundäre Navigation
- Task-Center
- Toast-Region

### `src/components/ui`

Wiederverwendbare Controls für Buttons, Icon-Buttons, Felder, Suche, Auswahl, Checkbox, Switch, Tabs, Karten, Badges, Status, Tooltip, Dropdown, Dialog, Bestätigung, Fortschritt, Skeleton, Leerzustand, Fehlerzustand und Tabellen.

### `src/pages`

- Startseite mit Profilspalte, abgegrenzter Spielerbühne und Statusspalte
- ehrliche Vorschau-/Leerzustände für noch nicht implementierte Bereiche
- Einstellungen für den freigegebenen Phase-2-Umfang

Die Spielerbühne ist ausschließlich ein responsiver Layoutplatzhalter. Sie lädt keinen Skin und rendert keine Wings, Halos oder Cosmetics.

## Persistenz

`settings.json` bleibt die maßgebliche Quelle für globale Launcher-Einstellungen. Phase 2 ergänzt beziehungsweise verwendet:

- `appearance`
- `locale`
- `accent_color`
- `ui_density`
- `navigation_mode`
- `background_variant`
- `reduced_motion`

Die Speicherung erfolgt durch Rust:

```text
React Setting
→ typisierter IPC-Request
→ Rust-Validierung
→ temporäre Datei im registrierten Einstellungsordner
→ Flush/Sync
→ atomarer Austausch
→ typisierte Antwort
```

Im Browser-Harness wird ausschließlich ein flüchtiger In-Memory-Adapter verwendet. Produktionsdaten werden weder in `localStorage` noch in `sessionStorage` abgelegt.

## IPC

Neue gemeinsam typisierte Commands:

| Command | Eingabe | Ausgabe |
|---|---|---|
| `phase2_shell_bootstrap` | keine | `Phase2ShellBootstrap` |
| `phase2_save_shell_settings` | `ShellSettings` | `Phase2ShellBootstrap` |

Beide Commands:

- erlauben nur das Hauptfenster,
- verwenden stabile `TypedIpcError`-Objekte,
- prüfen Werte in Rust,
- greifen nur über registrierte Phase-1-Pfade auf Dateien zu,
- werden aus `contracts/ipc-contracts.json` für TypeScript erzeugt und automatisch abgeglichen.

## Plattformgrenze

Der Windows-spezifische atomare Dateiaustausch liegt in `platform::atomic_replace`. Andere Plattformen verwenden denselben Aufruf über eine getrennte Implementierung. Eine prozessweite Mutex-Sperre schützt konkurrierende Schreibvorgänge auf die Launcher-Einstellungen vom temporären Schreiben über Flush/Sync und Replace bis zum Fehler-Cleanup; die eindeutigen `create_new`-Temporärdateien und der atomare Plattform-Replace bleiben erhalten. Die Sperre schützt Prozesskonkurrenz; eine Abschwächung der atomaren Dateispeicherung findet nicht statt. Test- und Browserläufe schreiben nicht in reale Nutzerdaten.

## Bewusst nicht umgesetzt

- Microsoft-Anmeldung
- echte Profile oder Minecraft-Starts
- Modrinth und Modpack-Editor
- Cloud
- neues Update-System
- vollständiger 3D-Viewer
- Shop, Coins, Freunde oder Community
