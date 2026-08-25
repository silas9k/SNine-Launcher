# Phase 2 – Designsystem und Tokens

## Grundrichtung

Das Interface ist ein eigenständiges S9Lab-Design: modern, ruhig, hochwertig und mit dezenten Minecraft-/Gaming-Akzenten. Es kopiert keine NoRisk-Oberfläche. Pixelartige beziehungsweise monospaced Typografie wird nur für Branding und kurze Akzente verwendet; Fließtext nutzt einen lokalen Systemschrift-Stack.

## Eine Produktions-CSS-Kette

```text
src/styles/index.css
├── tokens.css
├── base.css
├── components.css
└── layout.css
```

Alte konkurrierende Produktions-Stylesheets und CSS-Backups wurden entfernt. Außerhalb von `tokens.css` sind keine festen Hex-, RGB-, HSL- oder RGBA-Farbwerte erlaubt.

## Semantische Token-Gruppen

| Gruppe | Beispiele |
|---|---|
| Basisfarben | `--color-neutral-*`, Status- und Akzentgrundfarben |
| Hintergründe | `--background-base`, `--background-raised` |
| Oberflächen | `--surface-primary`, `--surface-secondary`, `--surface-interactive`, `--surface-overlay` |
| Text | `--text-primary`, `--text-secondary`, `--text-muted` |
| Rahmen | `--border-default`, `--border-strong` |
| Akzent/Fokus | `--color-accent`, `--color-on-accent`, Hover, Pressed, Fokus |
| Status | Erfolg, Warnung, Fehler, Information |
| Abstände | `--space-1` bis `--space-12`, `--density-space` |
| Radien | XS bis XL und rund |
| Schatten | Karten-, Floating- und Fokus-Schatten |
| Bewegung | Dauer, Easing und vollständige Reduced-Motion-Abschaltung |
| Ebenen | Titelzeile, Navigation, Overlay, Dialog, Toast |
| Layout | Titelleistenhöhe, Navigationsbreite, Control-Höhe, Content-Maximum |

## Themes

Unterstützt werden:

- System
- Hell
- Dunkel
- Hoher Kontrast

Alle Themes verwenden dieselben Komponenten und semantischen Tokens. Das Systemtheme reagiert auf Änderungen von `prefers-color-scheme`. Hoher Kontrast verwendet klare Flächen, starke Rahmen und keine dekorativen Schatten.

## Akzentfarben

`resolveAccentPalette` prüft eine Eingabe im Format `#RRGGBB` und erzeugt:

- sicheren Akzent,
- Textfarbe auf dem Akzent,
- Hover-Zustand,
- Pressed-Zustand,
- Fokusfarbe,
- Kontrastwerte gegen helle und dunkle Flächen.

Mindestwerte:

- UI-Kontrast gegen helle und dunkle Oberfläche: 3:1
- Text auf Akzent: 4,5:1

Eine formal ungültige Farbe wird abgelehnt. Eine gültige, aber ungeeignete Farbe wird schrittweise in einen sicheren Bereich verschoben und dem Nutzer mit Ein- und Ausgabewert erklärt. Die tatsächlich gerenderte Oberfläche verwendet immer die validierte Palette.

## Dichte und Navigation

- **Komfortabel:** größere Abstände, vollständige Beschriftungen.
- **Kompakt:** reduzierte Abstände, weiterhin ausreichend große Bedienelemente.
- **Erweiterte Navigation:** Icons plus Labels.
- **Kompakte Navigation:** Icons, vollständige Screenreader-Namen und Tooltips über `title`.

## Lokale Hintergründe

- Ruhig
- Raster
- Terrain

Alle Varianten entstehen ausschließlich über lokale CSS-Tokens und Gradienten. Es werden keine Remote-Bilder geladen.

## Bewegungen

Wenn Nutzer „Reduzierte Animationen“ aktivieren oder das Betriebssystem `prefers-reduced-motion: reduce` meldet, werden nicht notwendige Animationen und Übergänge vollständig auf `none` gesetzt. Sie werden nicht lediglich verlangsamt.
