# Phase 10 – Windows-Testmatrix

Status: lokaler Phase-10-Checkpointlauf vom 2026-08-11; Cleanroom-Wiederholung
aus dem finalen Quellarchiv folgt als separates Endgate.

| Bereich | verbindlicher Nachweis |
|---|---|
| Quellbindung | ZIP-Name, unabhängiger SHA-256 und exakt ein Stammordner |
| Cleanroom | Quellsauberkeit und Registry-/Workflow-/Phasengates vor `npm ci` |
| Frontend | öffentliche npm-Registry, vollständige Node-/UI-Suite und Produktionsbuild |
| Browser | fünf Theme-/Locale-/Breitenfälle, Axe, Layout und Performance/Speicher |
| Rust | Format, Check, Clippy `-D warnings`, Windows-Spezialtests und Gesamtlauf dreimal |
| Recovery | Operations-, Profil-, Inhalts-, Backup-/Restore- und Cloud-Merge-Regressionen |
| NSIS | exakter Name/Version, NotSigned-Diagnosestatus, CurrentUser-Lifecycle |
| Installation | neuer TEMP-Zielpfad, keine bestehende S9Lab-Installation überschreiben |
| Wartung | identischer 1.0.8-`/UPDATE`-Pfad ohne beschädigte Hauptdatei |
| Deinstallation | Hauptdatei, Uninstaller und HKCU-Uninstall-Eintrag entfernt |
| Signatur | separater strikter Verifikator akzeptiert nur `Valid`; Diagnosebuild bleibt blockiert |
| Lieferung | vollständiges Git-Archiv, SHA-Datei, Hash-Delta, sicherer Apply/Rollback |

`VERIFY-PHASE10-WINDOWS.ps1` schreibt maschinenlesbare Evidenz. Der Installer-
Lifecycle ist opt-in und stoppt bei jeder vorbestehenden S9Lab-Registrierung.
## Lokaler Checkpoint

- PowerShell-Parser: alle vier neuen Liefer-/Windows-Skripte fehlerfrei.
- `npm test`: 87 Node- und 39 UI-Tests bestanden.
- Produktionsbuild: 1.638 Module; Shell-JS 403,19 kB, Renderer separat
  515,39 kB und CSS 75,93 kB vor gzip.
- Rust-Format, Check und Clippy mit `-D warnings`: bestanden.
- Tauri-/NSIS-Build: bestanden; 6.244.547 Bytes, SHA-256
  `2229EAB718FFAC3563DF6762B6BC1300A31F67E5874F4C8575537C4300C7F670`,
  Authenticode `NotSigned`.
- Lifecycle-Präflight: fail-closed bestanden; eine vorhandene systemweite
  S9Lab-1.0.8-Installation wurde erkannt und nicht verändert.

Der mutierende Lifecycle konnte auf diesem Rechner deshalb nicht sicher
ausgeführt werden. Windows Sandbox ist nicht installiert. Der frische
Windows-CI-Workflow aktiviert den Lifecycle verbindlich; lokal bleibt dieser
eine Nachweis ehrlich offen. Die finalen Cleanroom-Messwerte und Artefakthashes
werden in der externen Evidenz und dem Artefaktbericht ausgewiesen.
