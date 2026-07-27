# Phase 2 v1.0.2 – Windows-Gate-Korrekturen

## Verbindliche Basis

`S9Lab-Launcher-Phase2-v1.0.1-final-source.zip` wurde vor Änderungen mit SHA-256 `e2dddc534bb730eab7885b2181726c97ae3f616df9e48a3c6838874bf220ac9b` bestätigt. Die Produktversion bleibt `1.0.8`. Phase 3 wurde nicht begonnen; es wird nichts signiert oder veröffentlicht.

## Nutzerseitiger Windows-Befund

Der Nutzer meldete auf echtem Windows 11 alle JavaScript-, Browser-, Build-, Format-, Check- und Clippy-Stufen als erfolgreich. Der erste normale parallele Rust-Gesamtlauf endete mit 34 bestandenen und 7 fehlgeschlagenen Tests. Eine Ursache war die gleichzeitige `MoveFileExW`-Ersetzung derselben Settings-Datei; sechs weitere Fehler entstanden ausschließlich durch den unter dem langen Cleanroom-Pfad verschachtelten Runtime-TEMP. Dies sind Nutzerresultate, keine eigenen Windows-Ergebnisse.

## Settings-Schreibkollision

- Eine statische prozessweite `Mutex<()>` serialisiert Settings-Schreibvorgänge.
- Der Lock bleibt über Verzeichniserstellung, eindeutiges `create_new`-Tempfile, vollständiges Schreiben, `sync_all`, atomaren Replace und Fehler-Cleanup gehalten.
- Ein vergifteter Lock erzeugt stabil `settings_write_lock_poisoned`.
- Keine Sleeps, keine Wiederholungsschleife für Windows-Sharingfehler und keine Abschwächung von `platform::atomic_replace` wurden eingeführt.
- Der bestehende Paralleltest behält seine Parallelität und prüft vollständiges JSON, ein definiertes abschließendes Writer-Ergebnis, keine `.settings-*.tmp`, Cleanup nach fehlgeschlagenem Replace und den typisierten Poison-Fehler.
- Die Rust-Testanzahl bleibt dadurch bei 41.

## Windows-Prüfer

- Neuer verbindlicher Prüfer: `VERIFY-PHASE2-V1.0.2-WINDOWS.ps1`.
- Runtime-TEMP liegt als kurzer `s9v-<12 Hexzeichen>`-Ordner direkt unter System-TEMP und nicht unter dem langen GUID-Cleanroom.
- Vor den Tests wird das Budget aus TEMP, Fixture-Overhead 40, relativem Erfolgsweg 117 und unverändertem Maximum 247 geprüft.
- `TEMP`, `TMP`, `NPM_CONFIG_CACHE`, `NPM_CONFIG_REGISTRY`, `CARGO_HOME`, `S9LAB_BROWSER_PATH`, `S9LAB_VISUAL_OUTPUT` und `S9LAB_PERFORMANCE_OUTPUT` werden vorab samt Set/Unset-Zustand gespeichert und im äußeren `finally` exakt wiederhergestellt.
- Der vollständige Gate-Befehl bleibt dreimal `cargo test --locked -- --nocapture`; keine Testserialisierung.
- Vier Node-Regressionen sichern die Struktur des Prüfers statisch ab.

## Freigabe

Eigene Cleanroom-Prüfungen auf dem Implementierungshost: alle drei Vorinstallations-Guards, öffentliches `npm ci` mit 188 Paketen, 58/58 Node-Tests, 12/12 Vitest-Tests und Vite-Build mit 1.603 Modulen erfolgreich. Browser und Performance konnten ohne lokales Chromium nicht wiederholt werden; eine Cargo-/Windows-MSVC-Toolchain ist nicht vorhanden.

Der vollständige v1.0.2-Windows-MSVC-Lauf ist bis zur erneuten Nutzerprüfung offen. Keine Windows-Freigabe wird behauptet.
