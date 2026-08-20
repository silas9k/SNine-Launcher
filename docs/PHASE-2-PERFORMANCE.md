# Phase 2 – Performance-Harness und Messbericht

## Zielwerte

| Messwert | Ziel |
|---|---:|
| Kalter App-Start bis Shell bereit | höchstens 3.000 ms |
| App-Shell ohne 3D-Viewer im Leerlauf | höchstens 220 MiB |
| Dauerhafter Speicherzuwachs nach 100 Seitenwechseln | unter 30 MiB |
| Visuelle Navigationsreaktion, p95 | höchstens 100 ms |

## Browser-Harness

`scripts/run-performance-harness.mjs` misst reproduzierbar:

- interne Markierung `s9lab.app.start-to-shell-ready`,
- erste Interaktivitätsmarkierung,
- 100 Seitenwechsel,
- p95 und Maximum der Navigation,
- JS-Heap vor und nach erzwungener Garbage Collection.

### Selbst gemessener finaler Lauf

Messzeitpunkt: 17. Juli 2026. Umgebung: lokaler Headless-Chromium-Browser-Harness in der Implementierungsumgebung. Diese Werte sind **nicht** gleichbedeutend mit einem nativen Tauri-Kaltstart oder dem vollständigen WebView2-Prozessspeicher.

| Messwert | Ergebnis |
|---|---:|
| Shell bereit | 90,4 ms |
| Interaktivitätsmarkierung | 221,7 ms |
| Navigation, 99 verwertbare Samples, p95 | 81,0 ms |
| Navigation Maximum | 128,5 ms |
| JS-Heap vorher | 2,17 MiB |
| JS-Heap nachher | 3,42 MiB |
| Dauerhafter Delta | 1,25 MiB |

Alle im Browser-Harness direkt erzwungenen Ziele wurden erreicht. Maßgeblich für das Navigationsziel ist p95; der einzelne Maximalwert von 128,5 ms ist transparent ausgewiesen.

## Native Windows-Messung

`MEASURE-PHASE2-WINDOWS.ps1` dokumentiert:

- Windows-Version,
- CPU,
- logische Prozessoren,
- RAM,
- Zeit bis ein sichtbares reagierendes Hauptfenster vorhanden ist,
- Working Set und privaten Speicher des gesamten erkannten Launcher-/WebView2-Prozessbaums.

Der Fensterwert ist ein reproduzierbarer Windows-Proxy. Für die endgültige Produktfreigabe muss die interne Shell-Bereitschaft zusätzlich auf dokumentierter Referenzhardware mit einem vollständigen Tauri-Instrumentierungsweg korreliert werden.

In der Implementierungsumgebung wurde kein nativer Windows-/Tauri-Messwert erfunden oder behauptet.
