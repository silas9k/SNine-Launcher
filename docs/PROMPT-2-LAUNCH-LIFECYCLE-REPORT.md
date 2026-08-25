# Prompt 2 – Minecraft Launch Lifecycle

Stand: 24. August 2026

## 1. Launch Lifecycle

Der kanonische Zustand liegt im Rust-Backend und wird pro Launch-ID geführt:

`preparing → checking-files → downloading → checking-files → starting → running → exited|failed`

`stopping` wird bei einem expliziten Stop gesetzt. Ein Launch wird vor Dateiprüfung oder Download reserviert. Damit kann derselbe Profilstart weder durch schnelle UI-Klicks noch durch konkurrierende IPC-Aufrufe doppelt gestartet werden. Pending-, laufende und die letzten 16 beendeten Sessions werden gemeinsam als Status geliefert. Jeder Status enthält Profil, Account, Start-/Endzeit, PID, Exit-Code und einen sicheren Fehlercode.

Das Backend sendet `minecraft-launch-status`. Frontend-Listener besitzen ein Remount-sicheres `unlisten`; Status-Polling bleibt nur als Recovery-Fallback aktiv.

## 2. Play Button

Der Startbutton bildet die echten Backend-Phasen ab und ist während aller aktiven Phasen gesperrt. Bei einem echten SNine-Download wird der vom Backend gelieferte Byte-Fortschritt als Prozentwert ergänzt. Fehler enden in „Erneut versuchen“, erfolgreiche Exits in `idle`. Ein synchroner Frontend-Guard und die Backend-Reservierung bilden zwei voneinander unabhängige Mehrfachstart-Sperren.

## 3. Process Tracking

Der bestehende `ProfileProcessManager` verwaltet jetzt zusätzlich vorbereitende Sessions. Nach dem Spawn werden PID und `running` gespeichert. Ein Hintergrundmonitor prüft den echten Child-Prozess alle 500 ms, sendet Terminalzustände und schreibt den Exit-Code in das Session-Log. Erfolgreicher Exit wird `exited`, ein nicht erfolgreicher Exit wird `failed` mit `runtime_process_crashed`. Explizites Stoppen wartet auf den Prozess und bewahrt den abgeschlossenen Status auf.

## 4. Log System

Das vorhandene `MinecraftLogWindow` und die vorhandene Session-Datei werden weiterverwendet. Das Fenster öffnet sich bereits bei `preparing`, liest stdout und stderr live aus derselben Launch-Datei und beendet Minecraft beim Schließen nicht. Beim erneuten Öffnen wird dieselbe Datei ab Offset 0 geladen. Eine neue Launch-ID erzeugt eine getrennte Datei und ein getrenntes Fenster.

Ergänzt wurden Event-basierter Prozessstatus, Polling-Fallback, Timestamp-Spalte, PID/Profilanzeige und Kopieren der gefilterten sichtbaren Logs. Suche, Level-Filter, Warnungs-/Fehlerfarben, Autoscroll, Autoscroll-Toggle, Ansicht leeren und Export bleiben erhalten.

## 5. Settings

`autoOpenMinecraftLog` ist Teil der atomar gespeicherten Rust-`ShellSettings`. Default ist `true`; Settings-Schema 3 migriert ältere Dateien auf `true`. Die vorhandene Settings-Seite verwendet den bestehenden `SettingsRow`- und `Toggle`-Baustein. Bei `false` startet Minecraft ohne automatisches Fenster; manuelles Öffnen bleibt unverändert möglich.

## 6. Fehlerbehandlung

Java-, Profil-, Runtime-, Download-, Verifikations- und Spawnfehler schließen die reservierte Session als `failed` ab. Der UI-Guard wird freigegeben, Logs bleiben vorhanden und ein erneuter Start ist möglich. Toasts enthalten nur kurze Benutzertexte; technische Codes und Details bleiben im Log. Tokens und Startargument-Geheimnisse werden weiterhin redigiert.

## 7. Geänderte Dateien

- `contracts/ipc-contracts.json`
- `src/lib/generated/ipc-contracts.ts`
- `src/lib/launchLifecycle.ts`
- `src/theme/types.ts`
- `src/app/shellStore.ts`
- `src/pages/HomePage.tsx`
- `src/pages/MinecraftLogWindow.tsx`
- `src/pages/SettingsPage.tsx`
- `src/pages/InstancesPage.tsx`
- `src/components/runtime/RuntimePanel.tsx`
- `src/i18n/messages.ts`
- `src/styles/index.css`
- `src-tauri/src/app/config.rs`
- `src-tauri/src/ipc/mod.rs`
- `src-tauri/src/minecraft/profile_launch.rs`
- `src-tauri/src/minecraft/service.rs`
- `src-tauri/src/snine_client_delivery.rs`
- `src-tauri/src/cloud_sync/service.rs`
- `src-tauri/src/mod.rs`
- `tests/unit/launch-lifecycle.test.ts`
- `docs/PROMPT-2-LAUNCH-LIFECYCLE-REPORT.md`

## 8. Tests

| Fall | Abdeckung | Ergebnis |
|---|---|---|
| A: Preparing → Starting → Running → Exit → Idle | Backend-State, Monitor, UI-Zuordnung | Implementiert; TypeScript/Lifecycle-Test bestanden |
| B: fünf schnelle Klicks | synchroner Guard + atomare Rust-Reservierung | Frontend-Test bestanden; Rust-Test vorhanden |
| C: Startfehler und Retry | `fail_pending`, Log-Historie, Guard-Freigabe | Lifecycle-Test bestanden; Rust-Test vorhanden |
| D: Crash | nicht-null/nonzero Exit → `failed` | Lifecycle-Test bestanden |
| E/F: Auto Log an/aus | persistentes Setting und bedingter Backend-Open | Contract/Build bestanden; Rust-Roundtrip-Test vorhanden |
| G: Log schließen | Fensterkommando ist vom Prozess-Stop getrennt | Codepfad geprüft |
| H: Log erneut öffnen | gleiche Launch-ID und persistente Logdatei | Codepfad geprüft |

Ausgeführt:

- Frontend-Produktionsbuild: bestanden
- gezielte Vitest-Suite: 8/8 bestanden
- komplette Vitest-Suite: 42/54 bestanden; 12 bereits bestehende, veraltete Home-/Navigation-Tests passen nicht zur in Prompt 1 vereinfachten Oberfläche
- Node-Suite: 89/90 bestanden; nur der bereits bekannte PowerShell-Parser-Test ist ohne `pwsh` nicht ausführbar
- IPC-Verträge: bestanden
- IPC-Codegenerierung: aktuell
- Rust-Static-Check: bestanden
- Phase-5-Runtime-Security: bestanden
- UTF-8/Secrets/Design-System/i18n: bestanden
- bestehende globale Security-/Visible-Text-Gates melden weiterhin die schon vor Prompt 2 vorhandene lokale Browser-Persistenz beziehungsweise Rohtexte

## 9. Build-Ergebnis und Blockaden

`tsc --noEmit` und der Vite-Produktionsbuild sind erfolgreich. `cargo check`, `cargo test` und `tauri build` konnten nicht ausgeführt werden, weil die bereitgestellte Umgebung kein `cargo` und kein `rustc` enthält. Die statischen Rust- und Security-Gates sind erfolgreich, ersetzen aber keinen nativen Windows-/Tauri-Lauf. Ein realer Microsoft-/Minecraft-End-to-End-Start benötigt außerdem eine Windows-Testmaschine mit gültigem Minecraft-Account und Netzwerkzugriff.
