# Phase 2 – Accessibility-Bericht

## Umgesetzt

- Skip-Link zum Hauptinhalt
- fokussierbarer scrollbarer Hauptbereich
- logische Überschriftenstruktur
- vollständige Beschriftungen der Navigation
- Screenreader-Namen für Icon-Buttons
- sichtbare `:focus-visible`-Markierungen
- Tastaturbedienung der Navigation
- Pfeiltastensteuerung für Tabs
- Tastaturöffnung und -navigation in Dropdown-Menüs
- Dialogfokus beim Öffnen
- Fokusfalle in Dialogen
- Escape zum Schließen geeigneter Dialoge
- Fokuswiederherstellung nach Dialogschluss
- ARIA-Live-Bereich für wichtige Shell-Zustände
- Text und Icons zusätzlich zu Statusfarben
- ausreichend große Controls
- Hochkontrastmodus
- `prefers-reduced-motion`
- benutzerdefinierte vollständige Bewegungsreduktion

## Automatisierte Tests

### Unit/Komponente

- axe-core gegen die gerenderte App-Shell
- Dialogfokus und Escape
- Navigationslabels und Seitenwechsel
- Tabs mit Pfeiltasten
- Dropdown mit Tastatur und Fokuswiederherstellung

### Browser

Für vier kombinierte Pflichtfälle werden geprüft:

- keine ernsthaften oder kritischen WCAG-2-A/AA-/2.1-AA-Verstöße laut axe-core,
- Dialogfokus,
- Escape-Verhalten,
- Task-Center innerhalb des Viewports,
- Tastaturaktivierung der Bewegungsreduktion,
- tatsächliche Übergangsdauer `0s`,
- korrektes Dokument-Lang-Attribut,
- kein horizontaler Überlauf.

## Bekannte Grenze

Automatisierte Accessibility-Analyse ersetzt keine manuelle Prüfung mit NVDA, Narrator und realer Tastaturbedienung. Diese manuelle Windows-Prüfung bleibt ein Freigabepunkt vor einem öffentlichen stabilen Release.
