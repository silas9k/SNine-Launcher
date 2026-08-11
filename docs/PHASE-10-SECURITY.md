# Phase 10 – Sicherheits- und Releasegrenzen

Der Windows-Cleanroom bindet das Quellarchiv vor dem Entpacken an einen
unabhängigen SHA-256. Registry-, Quellsauberkeits-, Workflow- und alle
Phasengates laufen vor der öffentlichen npm-Installation. Abhängigkeiten werden
nur aus `https://registry.npmjs.org/` anhand der Lockdatei installiert.

Der Verifikator verlangt Node 24, Windows-MSVC-Rust, aktive MSVC-Umgebung,
Rustfmt und Clippy. Temporäre npm-/Cargo-/Runtime-Pfade sind auf neu erzeugte
Verzeichnisse begrenzt und die ursprüngliche Umgebung wird wiederhergestellt.

Der Diagnoseinstaller muss exakt `NotSigned` sein. Das ist keine
Freigabeeigenschaft, sondern verhindert eine falsche Signaturbehauptung. Für
eine Produktion liefert `VERIFY-WINDOWS-SIGNATURES.ps1` das separate fail-closed
Gate: Nur Authenticode `Valid` mit vorhandenem Signerzertifikat wird akzeptiert.

Workflow und lokale Skripte besitzen keinen Upload-, Release-, Push- oder
Signierpfad. Geheimnisse, private Schlüssel und erfundene Produktionsendpunkte
sind weiterhin verboten.
