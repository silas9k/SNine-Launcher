# Phase 2 – Visuelle Regression

## Zweck

Die visuellen Referenzen prüfen die freigegebene App-Shell in den verbindlichen Größen, Sprachen, Themes und Dichten. Sie dienen als Phase-2-Baseline und enthalten keine echten Profile, Accounts, Downloads oder späteren Fachfunktionen.

## Pflichtfälle

| Datei | Größe | Theme | Sprache | Dichte | SHA-256 |
|---|---:|---|---|---|---|
| `900x600-dark-de-compact.png` | 900 × 600 | Dunkel | Deutsch | Kompakt | `5501eec13e388a78cf6fd5b2c0cb7d60f16175fd9b3fe80b288caa0c97187ceb` |
| `1280x720-light-de-comfortable.png` | 1280 × 720 | Hell | Deutsch | Komfortabel | `c372c8b5e83e2a6317ca914f99e918ef7adb4768355b89f9bc80b911ce2c15d8` |
| `1920x1080-contrast-en-comfortable.png` | 1920 × 1080 | Hoher Kontrast | Englisch | Komfortabel | `3f835ff07ccc96c5be7700a0af09534372d6322193615712802f667c1f3c4aca` |
| `1280x720-dark-en-compact.png` | 1280 × 720 | Dunkel | Englisch | Kompakt | `c871af884547e97d03e07e8617356db731736359030aad874ae56d4e5726c64e` |
| `640x900-system-en-comfortable.png` | 640 × 900 | System (hell) | Englisch | Komfortabel | `a6a4b498e80f106942697878ca77b799b4b468856f188a45dfd27db4c8feaf89` |

## Automatisch geprüfte Eigenschaften

Für jeden Pflichtfall werden zusätzlich zum Screenshot geprüft:

- kein Dokument- oder unbeabsichtigter Elementüberlauf,
- Task-Center vollständig innerhalb des Viewports,
- Dialogfokus und Schließen mit Escape,
- Fokus im Task-Center und Wiederherstellung,
- korrektes `lang`-Attribut und sichtbarer Produktname `S9Lab`,
- keine ernsthaften oder kritischen axe-core-Verstöße,
- reduzierte Animationen per Tastatur und tatsächliche Übergangsdauer `0s`,
- stabile Darstellung langer deutscher und englischer Texte,
- transparente, rahmen-, schatten- und rundungsfreie Vorschau-Wrapper per Computed Style,
- keine `ui-card`-Zuordnung der Vorschau,
- Spieler innerhalb der Vorschaugrenzen und ohne Überschneidung der Seitenbereiche.

## Ausführung

```powershell
$env:S9LAB_VISUAL_OUTPUT = "$PWD\artifacts\phase2-visuals"
npm run test:visual
```

Die Bildausgaben werden bewusst nicht in das Quellpaket aufgenommen. Sie werden als separates Übergabeartefakt bereitgestellt.

## Einordnung

Die zentrale Spielervorschau ist in Phase 2 v1.0.1 absichtlich frei in die Hauptfläche integriert. Ein technisch notwendiger Wrapper bleibt visuell transparent und ist nicht als Kasten erkennbar. Die Bilder behaupten weder einen fertigen 3D-Viewer noch Profile, Accounts oder Minecraft-Starts. Das fremde Referenzbild wurde nicht als Asset übernommen und seine Marken- oder Gestaltungselemente wurden nicht kopiert.
