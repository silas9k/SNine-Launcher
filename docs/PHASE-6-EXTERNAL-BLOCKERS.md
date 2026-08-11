# Phase 6 – externe Produktionsblocker und Nachweisgrenzen

Status: 2026-08-11. Diese Liste trennt fehlende externe Eingaben von interner
Restarbeit und noch ausstehenden Tests.

## Keine neuen Phase-6-Produktionsgeheimnisse

Die Modrinth-Integration benötigt weder privaten Schlüssel noch
Repository-Geheimnis oder frei konfigurierbare Produktionsdomain. API- und
Downloadursprung sind im Rust-Kern kontrolliert. Es darf kein alternativer
Raw-URL-, Spiegel- oder HTTP-Fallback ergänzt werden.

Für die lokal implementierbaren Phase-6-Kernfunktionen fehlt damit kein neuer
Produktionswert, den Codex erfinden müsste.

## Externe Verfügbarkeit für reale Nachweise

Reale Provider- und MRPACK-End-to-End-Tests setzen voraus:

- erreichbare offizielle Modrinth-API und -CDN,
- repräsentative, weiterhin veröffentlichte Projekte, Packversionen und Dateien
  sowie
- eine Testumgebung, in der öffentliche Downloads zulässig sind.

Ausfall, Ratenbegrenzung oder Änderung des externen Dienstes berechtigt nicht zum
Lockern von Host-, Redirect-, Identitäts-, Größen- oder Hashprüfungen. In diesem
Fall muss die UI einen Fehlerzustand anzeigen und die aktive Profilrevision
behalten.

Diese Netzwerkabhängigkeit ist eine Nachweisgrenze, aber kein fehlendes
S9Lab-Produktionsgeheimnis.

## Fortbestehende Phase-5-Blocker

Folgende Phase-5-Grenzen bleiben bestehen, wenn ein Phase-6-Profil sie
referenziert:

- Für den offiziellen S9Lab-Komponentenprovider fehlen weiterhin die
  freigegebene Produktions-Origin und der neue öffentliche Produktionsschlüssel.
- NeoForge-Prozessorausführung bleibt ohne vertrauenswürdige Outputbindung und
  geeigneten sicheren Ausführungsweg fail-closed.
- Managed Java besitzt noch keine kontrollierte, hashgebundene
  Beschaffungsstrecke.

Ein importiertes Profil darf diese Capabilities nicht mit Testwerten verfügbar
machen. Kann seine Laufzeit deshalb nicht sicher installiert werden, muss der
Import scheitern, ohne ein bestehendes Profil zu überschreiben.

## Nicht externe Restarbeiten

Der lokale Phase-6-Gesamtlauf, die statischen Sicherheitsgates, lokale MRPACK-,
Mehrpack-, Packupdate-, Override-, Profilformat- und Duplizierungsregressionen
sowie Browser-, Accessibility- und Chromium-Performance-Nachweise sind auf dem
Checkpointstand bestanden.

Noch nicht durch ein reales End-to-End belegt sind die native Dateiauswahl und
echte Modrinth-Suche, -Downloads und -Packinstallation. Das ist eine offen
dokumentierte Integrationsgrenze, keine fehlende externe Freigabe und keine
Erlaubnis, die implementierten Prüfungen zu lockern.

Updatekanäle, Backups, Wiederherstellungspunkte und geführte Migration sind
bewusst Phase 7 und ebenfalls keine externen Phase-6-Blocker.
