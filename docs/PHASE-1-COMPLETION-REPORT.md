# S9Lab Phase 1 – Abschlussbericht v1.0.3

**Produktversion:** 1.0.8, unverändert  
**Korrekturstand:** Phase 1 v1.0.3  
**Phase 2:** nicht begonnen und nicht freigegeben

## 1. Anlass

Die unabhängige Windows-Prüfung von Phase 1 v1.0.2 bestätigte Frontend-Build, Rustfmt, `cargo check`, Clippy, Operations-Preflight, Hardlink-Erkennung und die erfolgreiche Erstellung einer echten Windows-Junction. S9Lab lehnte die Junction korrekt ab, klassifizierte sie jedoch wegen der Prüfungsreihenfolge als `path_symlink_forbidden` statt mit dem dokumentierten stabilen Code `path_reparse_point_forbidden`.

v1.0.3 korrigiert ausschließlich diese Windows-Fehlerklassifizierung, die zugehörigen Regressionstests, CI-Prüfungen und Dokumentation. Es wurden keine Endnutzerfunktionen und keine Phase-2-Inhalte ergänzt.

## 2. Verbindliche Fehlerklassifizierung

Für Dateisystemlinks gilt nun eindeutig:

| Plattform und Struktur | Stabiler Fehlercode |
|---|---|
| Windows-Junction | `path_reparse_point_forbidden` |
| Windows-Symlink | `path_reparse_point_forbidden` |
| Symlink auf Plattformen ohne Windows-Reparse-Point-Semantik | `path_symlink_forbidden` |
| Hardlink mit Linkanzahl größer als eins | `path_hardlink_forbidden` |

Die gemeinsame Windows-Klassifizierung ist beabsichtigt: Junctions und Windows-Symlinks werden durch Reparse-Point-Metadaten repräsentiert. Die Windows-Reparse-Point-Prüfung läuft deshalb vor der allgemeinen Symlink-Prüfung. Dadurch liefert eine verifizierte Junction unabhängig von der Darstellung durch `std::fs::FileType` immer denselben dokumentierten Code.

Hardlinks bleiben vollständig getrennt. Ihre Prüfung und ihr Fehlercode wurden nicht verändert.

## 3. Implementierte Korrekturen

### 3.1 Prüfungsreihenfolge

`validate_existing_entry` prüft jetzt in dieser Reihenfolge:

1. Windows-Reparse-Point,
2. allgemeiner Symlink,
3. Hardlink-Anzahl für Dateien.

Auf Nicht-Windows-Systemen liefert `is_reparse_point` weiterhin `false`, sodass dort die vorhandene Symlink-Klassifizierung erhalten bleibt.

### 3.2 Verifizierte Junction-Fixture

Die bestehende Windows-Fixture wurde in eine wiederverwendbare Testhilfe überführt. Sie verlangt weiterhin:

- erfolgreiche Junction-Erstellung über `New-Item -ItemType Junction -Path ... -Target ...`,
- sichere Übergabe von Link und Ziel über Umgebungsvariablen,
- vorhandenen Link und vorhandenes Ziel,
- `ReparsePoint`-Attribut,
- `LinkType=Junction`, sofern PowerShell dieses Feld bereitstellt,
- Erkennung als Reparse Point durch Rust.

Fehlschläge der Fixture-Erstellung führen weiterhin zum Testfehler und werden nicht übersprungen.

### 3.3 Regressionen

Zwei getrennte Windows-Tests sichern den stabilen Code:

- `classifies_verified_windows_junctions_with_the_stable_reparse_error` prüft die direkte Metadatenklassifizierung.
- `rejects_windows_directory_junctions_after_verified_fixture_creation` prüft die Ablehnung über die registrierte Pfadwurzel.

Beide verlangen exakt `path_reparse_point_forbidden`; ein allgemeines `is_err()` oder mehrere akzeptierte Codes werden nicht verwendet.

Die Bereinigung entfernt ausschließlich die Junction. Eine Markerdatei im Ziel wird danach erneut geprüft, bevor das Ziel separat entfernt wird.

### 3.4 Statische Prüfung und Windows-CI

Die Phase-1-Foundation-Prüfung kontrolliert jetzt zusätzlich:

- Vorhandensein des neuen stabilen Klassifikationstests,
- Reihenfolge `is_reparse_point` vor `file_type().is_symlink()`,
- Vorhandensein beider Junction-Regressionen im read-only Windows-CI-Workflow.

Der Workflow veröffentlicht keine Artefakte, besitzt nur Leserechte und verwendet keine Signierschlüssel.

## 4. Selbst ausgeführte Prüfungen für v1.0.3

| Prüfung | Ergebnis |
|---|---|
| `npm ci` | erfolgreich, 84 Pakete |
| `npm test` | erfolgreich |
| UTF-8-Prüfung | erfolgreich |
| Secret-Prüfung | erfolgreich |
| Sicherheitskonfiguration | erfolgreich |
| Versionsprüfung | erfolgreich; Produktversion 1.0.8 |
| IPC-Prüfung | erfolgreich; 20 Commands geprüft, 1 gemeinsam typisiert |
| statische Rust-Prüfung | erfolgreich; 36 Rust-Dateien geprüft |
| Phase-1-Foundation-Prüfung | erfolgreich |
| TypeScript-Prüfung | erfolgreich |
| Vite-Produktionsbuild | erfolgreich; 1.604 Module |
| ZIP-Integritätsprüfung | erfolgreich |
| Prüfung des entpackten Quellpakets auf verbotene Ordner, Schlüssel- und Backup-Dateien | erfolgreich |
| `npm ci`, `npm test` und `npm run build` aus dem erneut entpackten finalen Quellpaket | erfolgreich |

## 5. Nicht selbst ausgeführte Rust- und Windows-Prüfungen

In der aktuellen Umgebung ist keine Rust-Toolchain installiert. Daher wurden für v1.0.3 nicht selbst ausgeführt:

- `cargo fmt --all -- --check`,
- `cargo check --locked`,
- `cargo clippy --locked --all-targets -- -D warnings`,
- die beiden Junction-Tests,
- Hardlink- und Recovery-Test,
- vollständiges `cargo test --locked -- --nocapture`,
- Tauri-/NSIS-Windows-Build.

Diese Ergebnisse werden ausdrücklich nicht als selbst erfolgreich behauptet. Die genauen Windows-Befehle stehen in Abschnitt 8 und in `VERIFY-PHASE1-WINDOWS.ps1`.

## 6. Nutzerseitige Windows-Ergebnisse für v1.0.2

Vom Projektinhaber bereitgestellt und nicht als eigene Ausführung gekennzeichnet:

Erfolgreich waren:

- `npm ci`, `npm test`, TypeScript und Vite,
- `cargo fmt --all -- --check`,
- `cargo check --locked`,
- Clippy mit `-D warnings`,
- Operations-Preflight,
- Hardlink-Test,
- erfolgreiche Erstellung und Verifizierung der Junction-Fixture,
- tatsächliche Ablehnung der Junction durch S9Lab.

Der Junction-Test scheiterte ausschließlich an der inkonsistenten Klassifizierung `path_symlink_forbidden` statt `path_reparse_point_forbidden`. Dieser konkrete Ausgangsbefund wurde in v1.0.3 korrigiert, ersetzt aber nicht die erneute Windows-Prüfung des neuen Quellstands.

## 7. Sicherheit und Umfang

Unverändert:

- keine privaten oder neuen Signierschlüssel,
- keine kompromittierten Schlüssel wiederhergestellt,
- kein Remote-Updater,
- kein unsicheres HTTP,
- keine Secrets in Quellcode, SQLite, Logs oder Fixtures,
- keine externen CSS-Laufzeitimporte,
- keine CSP-Lockerung,
- keine Veröffentlichung,
- keine produktiven Änderungen,
- kein Installer wird als signiert oder veröffentlichungsbereit bezeichnet,
- Phase 2 wurde nicht begonnen.

## 8. Verbindliche Windows-Nachprüfung

```powershell
$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

Remove-Item -Recurse -Force node_modules, dist, src-tauri\target -ErrorAction SilentlyContinue

npm ci
npm test
npm run build

Push-Location src-tauri
cargo fmt --all -- --check
cargo check --locked
cargo clippy --locked --all-targets -- -D warnings

cargo test --locked classifies_verified_windows_junctions_with_the_stable_reparse_error -- --nocapture
cargo test --locked rejects_windows_directory_junctions_after_verified_fixture_creation -- --nocapture
cargo test --locked rejects_existing_hardlinks -- --nocapture
cargo test --locked crash_recovery_never_leaves_a_mixed_revision -- --nocapture
cargo test --locked -- --nocapture
Pop-Location

npm run tauri:build
```

Der Tauri-/NSIS-Build darf erst nach vollständig grünen vorherigen Prüfungen erfolgen. Ein erzeugter Installer ist ohne Authenticode weiterhin unsigniert und nicht veröffentlichungsbereit.

## 9. Offene Risiken

- Die erneute echte Windows-Ausführung der Rust-Regressionen für v1.0.3 steht aus.
- Windows-Symlinks werden bewusst gemeinsam mit Junctions als Reparse Points klassifiziert; dies ist dokumentiert und stabil, unterscheidet jedoch nicht nach Reparse-Tag.
- Handle-basierte Härtung gegen privilegierte Race Conditions bleibt eine spätere Sicherheitsverbesserung.
- Installer und EXE bleiben unsigniert.

## 10. Abschluss

Phase 1 v1.0.3 ist als korrigierter Quellstand vorbereitet. Phase 2 wurde nicht begonnen. Eine Fortsetzung erfolgt ausschließlich nach ausdrücklicher Freigabe.
