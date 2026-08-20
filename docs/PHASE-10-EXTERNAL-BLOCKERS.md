# Phase 10 – externe Produktionsblocker

Status: 2026-08-11.

Für eine öffentliche Windows-Freigabe fehlen:

- ein organisationsseitig verwaltetes Windows-Code-Signing-Zertifikat samt
  sicherem Signierdienst und Zeitstempelrichtlinie,
- neue freigegebene Produktionsschlüssel für Launcher- und S9Lab-Komponenten,
- offizielle HTTPS-Origins/Backend-APIs für die gesperrten Kanäle,
- ein realer Upgrade-Kandidat mit einer Version nach 1.0.8,
- organisationsseitige Veröffentlichung, Support- und Incident-Freigaben.

Der lokal erzeugte NSIS-Kandidat bleibt deshalb ein unsigniertes
Diagnoseartefakt. Installations-/Wartungs-/Deinstallations-, Cleanroom-, Hash-,
Rollback-, Performance- und Sicherheitstests können unabhängig davon lokal
abgeschlossen werden. Es wird weder ein Testzertifikat noch eine erfundene
Domain als Produktionsersatz verwendet.
