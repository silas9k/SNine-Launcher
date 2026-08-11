# Phase 9 – Testmatrix und Gate

Status: bestandener kombinierter Phase-8/9-Checkpointlauf vom 2026-08-11.

| Bereich | lokaler Nachweis |
|---|---|
| lokale Assets | Skin und Cape als Data-Assets, kein externer Abruf |
| Ausrüstung | Cape, Elytra/Wings, Halo und kein Rückenelement |
| Animation | Idle, Walk, Wave; bei Reduced Motion deaktiviert |
| Kamera | freie Rotation, Tastatur, Front, Rückseite, Reset; kein Zoom/Pan |
| Identität | nur aktives lokales Konto oder S9Lab; Classic/Slim und Icon |
| Zustände | Loading, Ready, fehlendes WebGL als Fallback |
| UI | echte mittlere Bühne ohne Kartenrahmen; Desktop und 390 px ohne Überlauf |
| Besitzgrenze | nur lokale Vorschau, kein unbestätigtes Entitlement |
| Guard | `check:phase9`, drei Node- und zwei UI-Regressionen |

## Ausgeführtes Gate

- Rust-Format, Check und Clippy: bestanden; Gesamtlauf 223/223.
- `npm test`: 84 Node- und 39 UI-Tests bestanden; darin zwei gezielte
  Player-Interaktionstests.
- Produktionsbuild: 1.638 Module; Renderer durch dynamischen Import vom
  403,19-kB-Shell-Bundle getrennt.
- Browser-Harness: fünf Theme-/Locale-/Breitenfälle einschließlich Axe grün.
- Sichtprüfung: Bühne, Cosmetics und Cloud-Karte bei Desktop und 390 px ohne
  horizontalen Überlauf; WebGL bereit und Steuerungen aktiv.
- Performance: Shell-ready 9,0 ms, interaktiv 43,3 ms, Navigation p95 3,6 ms
  und behaltenes Heapdelta 11,42 MiB innerhalb der festgelegten Grenzen.

Chromium meldete einmal eine treiberspezifische Shader-Bias-Warnung aus Three.js,
aber keinen Anwendungsfehler. Die Darstellung und alle Gates blieben grün.
