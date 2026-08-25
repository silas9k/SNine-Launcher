# Phase 8 – Testmatrix und Gate

Status: bestandener kombinierter Phase-8/9-Checkpointlauf vom 2026-08-11.

| Bereich | lokaler Nachweis |
|---|---|
| Microsoft-Basis | selektierte bestätigte Identität wird nur als Anzeigename übernommen |
| Payload | feste Profilmetadaten, Inhaltslisten und Shell-Einstellungen |
| Ausschlüsse | keine Tokens, Welten, Logs, Accounts oder beliebigen Pfade |
| Revision | kanonisches JSON und SHA-256-gebundene lokale Revision |
| Zwei Geräte | deterministischer Drei-Wege-Merge mit gemeinsamen und einseitigen Änderungen |
| Konflikt | vollständige manuelle Lokal-/Remote-Auswahl zwingend |
| Provider | ohne Konfiguration kein Link, Pull, Push oder Netzwerkfallback |
| IPC/UI | nur Zusammenfassung; Link sichtbar gesperrt; Desktop und 390 px geprüft |
| Guard | `check:phase8` plus drei Node-Regressionen |

## Ausgeführtes Gate

- Rust-Format, `cargo check --locked` und Clippy mit `-D warnings`: bestanden.
- gezielte Cloud-Sync-Rusttests: 3 bestanden.
- vollständiger Rustlauf: 223 bestanden, 0 fehlgeschlagen.
- `npm test`: 84 Node- und 39 UI-Tests bestanden.
- Produktionsbuild: 1.638 Module; Shell-JS 403,19 kB, getrennt geladener
  Renderer 515,39 kB und CSS 75,93 kB vor gzip.
- Browser: fünf responsive Theme-/Locale-Fälle einschließlich Axe bestanden;
  zusätzliche Sichtprüfung von Accounts bei 1.280 px und 390 px ohne Überlauf.
- Performance-Harness: Shell-ready 9,0 ms, interaktiv 43,3 ms,
  Navigation p95 3,6 ms/maximal 13,2 ms und behaltenes Heapdelta 11,42 MiB.

Die gemessenen Grenzen sind 3.000 ms, 100 ms und 30 MiB. Der Browser-Harness
ersetzt keinen nativen Tauri-Kaltstart. Externe Nachweisgrenzen stehen in
`PHASE-8-EXTERNAL-BLOCKERS.md`.
