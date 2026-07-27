# Phase 2 v1.0.1 – Isolierte Registry-Korrektur

## Verbindlicher Eingang und Schutzumfang

Der Eingang `S9Lab-Launcher-Phase2-v1.0.1-final-source.zip` wurde vor Änderungen mit SHA-256 `7876a34dcf5bbf3846d0598d89b1ce21ba7b0345b127c25c8bba2717d3ce28a9` bestätigt.

Browser-Cleanup, Windows-Prüfer, UI, CSS, Assets, Produkt- und Rust-Dateien blieben unverändert. Die fünf Browserbilder bleiben bytegleich. Phase 3 wurde nicht begonnen; es wurde nichts signiert oder veröffentlicht.

## Registry-Guard

- Jeder String unter `package.json:scripts` wird nach dem JSON-Parsing separat als Shell-/CLI-Text geprüft.
- Ein nichtkanonischer Pfad wird mit der konkreten Exaktwertverletzung abgelehnt; die kanonische Root-Variante bleibt zulässig.
- Die Windows-Form `set "NPM_CONFIG_REGISTRY=WERT"` wird einschließlich des schließenden Anführungszeichens in `.cmd` und `.bat` ausgewertet.
- Separate Positiv- und Negativtests sowie isolierte `inspectProject`-Projekte verhindern, dass eine andere Verletzung den geprüften Fall verdeckt.

- Quoted und unquoted `--registry`-CLI-Werte werden vollständig ausgewertet.
- `NPM_CONFIG_REGISTRY` wird unabhängig von der Position in JSON-Scripts, PowerShell-, Shell-, CMD- und BAT-Zeilen erkannt.
- YAML-Block- und Flow-Mappings werden erkannt.
- Registryähnliche protocol-relative URLs in ausgeliefertem Text werden abgelehnt.
- `inspectProject` prüft alle kleinen, gültig UTF-8-dekodierbaren Nicht-Binärdateien statt einer unvollständigen Extension-Allowlist.
- End-to-End-Tests erzeugen temporäre Projekte mit realen `.sh`-, `.bash`-, `.zsh`-, `.cmd`-, `.bat`- und extensionlosen Scriptdateien.
- Synthetische verbotene URLs werden aus Teilstrings aufgebaut.

Die frühere Aussage, sämtliche Shellvarianten seien bereits erkannt worden, war falsch. Sie gilt erst mit den neuen End-to-End-Tests als nachgewiesen.

## Workflow-Guard

- Der eigene YAML-Zeilenparser wurde vollständig entfernt.
- Workflows werden mit dem öffentlichen Paket `yaml` als YAML 1.2 in eine Datenstruktur geparst.
- Eine aus `yaml` erzeugte Bundle-Kopie samt Lizenz liegt unter `scripts/vendor/`, damit der Guard bereits vor `npm ci` ausführbar ist.
- YAML-Parsefehler, fehlende `jobs`-Strukturen sowie nicht auswertbare Jobs, Steps oder `run`-Felder schlagen fail-closed fehl.
- Für jedes aufgelöste `run`, das `npm ci` ausführt, werden zwei separate frühere Guard-Schritte im selben Job verlangt.
- Guard-`run.trim()` muss exakt dem Einzelbefehl entsprechen; ein `if`-Feld oder `continue-on-error: true` macht den Guard ungültig.
- Kommentare, `|| true`, Zusatzbefehle, gefaltete Umgehungen und Guards aus anderen Jobs zählen nicht.
- Tests decken inline `if`, inline `continue-on-error`, gefaltetes `npm ci`, gequotete Jobnamen, Flow-Mappings und Parsefehler ab.

## Freigabestatus

Es gab keinen Windows-Lauf. Cargo Check, Clippy, drei vollständige Windows-MSVC-Rust-Gesamtläufe und Tauri/NSIS bleiben offen. Es wird keine Windows-Freigabe behauptet.
