# Phase 2 – Unabhängige Windows-Nachprüfung

## Voraussetzungen

- Windows-MSVC-Entwicklungsumgebung
- Node.js Hauptversion 24 und npm mit effektiver Registry `https://registry.npmjs.org/`
- Rust-Toolchain mit echtem Windows-MSVC-Host
- Tauri-/NSIS-Voraussetzungen
- lokaler Chromium-, Chrome- oder Edge-Browser für Browserprüfungen

Es werden keine Signierschlüssel benötigt. Der Build bleibt lokal und unsigniert.

## Empfohlener vollständiger Lauf

```powershell
.\VERIFY-PHASE2-V1.0.3-WINDOWS.ps1 -ExpectedSha256 "<SHA-256 aus unabhängigem Übergabekanal>"
```

Das Skript arbeitet ausschließlich mit `S9Lab-Launcher-Phase2-v1.0.3-final-source.zip` und der gleichnamigen `.sha256`-Datei. Der verpflichtende Parameter `-ExpectedSha256` ist der unabhängige Vertrauensanker: ZIP und SHA-Datei müssen beide exakt damit übereinstimmen. Erst danach wird entpackt. Das Skript erzeugt ein neues GUID-Prüfverzeichnis und überschreibt keinen vorhandenen Quellbaum. Optional können `-ZipPath`, `-ChecksumPath` und `-BrowserPath` angegeben werden.

Für Rust-Erfolgs-Fixtures setzt der Prüfer `TEMP` und `TMP` auf einen separaten kurzen, kollisionsfreien `s9v-*`-Ordner direkt unter dem normalen System-TEMP. Eine UTF-16-Budgetprüfung stellt vor dem Testlauf sicher, dass die unveränderte Produktionsgrenze 247 und ein gültiger relativer Erfolgsweg von 117 Einheiten zusammenpassen. Alle acht veränderten Umgebungsvariablen werden in einem äußeren `try/finally` auf ihren exakten vorherigen Zustand zurückgesetzt; zuvor fehlende Variablen werden wieder entfernt. Diagnoseartefakte dürfen liegen bleiben.

Vor dem Entpacken bricht es ab, wenn Node nicht Hauptversion 24 ist, `rustc -vV` keinen `*-pc-windows-msvc`-Host meldet oder die effektive npm-Registry nicht exakt dem öffentlichen kanonischen Wert entspricht. Danach führt es nacheinander aus:

1. Registry-, Quellpaket- und Workflow-Guard vor der Installation,
2. öffentliches `npm ci`, Frontend-, Browser-, Layout-, Accessibility- und Performance-Prüfungen,
3. Rust-Formatierung, Cargo Check und Clippy,
4. drei vollständige parallele Rust-Gesamtläufe,
5. lokalen Tauri-/NSIS-Diagnosebuild,
6. Prüfung auf genau einen Installer mit Produktversion `1.0.8` und Authenticode-Status `NotSigned`.

`VERIFY-PHASE2-WINDOWS.ps1` bleibt als Kompatibilitätswrapper erhalten. `MEASURE-PHASE2-WINDOWS.ps1` ergänzt native Prozessbaum- und Fensterbereitschaftsmessungen.

## Verbindliche Bewertung

- Ein erzeugter NSIS-Installer ist ohne Authenticode ausschließlich ein lokaler Test-Build.
- Es wird kein MSI-Ergebnis behauptet; die aktuelle Windows-Bundle-Konfiguration ist bewusst NSIS-only.
- Der Workflow verwendet keine Signierschlüssel, veröffentlicht keine Releases und lädt keine Artefakte hoch.
- Phase-1 und Phase-2 müssen in jedem der drei vollständigen parallelen Rust-Läufe grün bleiben.
- Erst ein vollständig grüner Skriptlauf mit drei normalen parallelen Gesamtläufen zu jeweils 41/41 Tests schließt das technische Phase-2-v1.0.3-Gate; manuelle Screenreader- und Tastaturprüfung bleiben zusätzliche Freigabepunkte.
