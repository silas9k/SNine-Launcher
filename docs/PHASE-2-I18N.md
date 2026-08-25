# Phase 2 – Internationalisierung

## Sprachen

- Deutsch (`de`)
- Englisch (`en`)

Beim ersten Start kann `system` verwendet werden. Deutsch wird bei einer deutschen Systemsprache gewählt, ansonsten Englisch. Die Auswahl ist jederzeit änderbar und wird über Rust atomar gespeichert.

## Typisierung

`src/i18n/messages.ts` definiert das deutsche Wörterbuch als maßgebliche Schlüsselmenge. `TranslationKey` wird daraus abgeleitet. Das englische Wörterbuch muss exakt dieselben Schlüssel besitzen.

Aktueller Umfang: **152 Schlüssel je Sprache**.

## Funktionen

- typisierte Übersetzungsschlüssel
- Parameterinterpolation wie `{page}`
- Pluralformen über `Intl.PluralRules`
- Zahlenformatierung über `Intl.NumberFormat`
- Datum/Uhrzeit über `Intl.DateTimeFormat`
- sichtbarer Entwicklungsfallback `⟦missing.key⟧`
- dynamisches `lang`-Attribut am Dokument

## Automatische Prüfungen

`check-i18n.mjs` prüft:

- Schlüsselparität,
- Parameterparität,
- fehlende Werte,
- bekannte falsche ASCII-Umlautersetzungen in deutschen sichtbaren Texten.

`check-visible-text.mjs` analysiert TSX über den TypeScript-AST und lehnt sichtbare Rohtexte sowie nicht übersetzte `alt`-, `aria-label`-, `placeholder`- und `title`-Attribute ab.

Die bestehende UTF-8-Prüfung erkennt Mojibake und fehlerhafte Kodierungen. Technische IDs, Pfade und Dateinamen bleiben unverändert ASCII-basiert.
