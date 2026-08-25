# Default-Mods / NoRisk-style recommendation plan

## Ziel

Neue Profile sollen sofort mit einer kleinen, sicheren und kompatiblen Grundauswahl an Mods starten. Die Auswahl muss den aktiven Runtime-Kontext respektieren und nur kompatible Bundles vorschlagen, die in das bestehende Sicherheitsmodell passen.

## Grundsatz

- Default-Mods sind Vorschläge, keine Umgehung der Dependency- und Sicherheitsvalidierung.
- Nur Loader/Version-Kombinationen mit verifizierter Kompatibilität werden angeboten.
- Die Auswahl bleibt konservativ und klein: wenige grundlegende Performance-, QoL- und Stabilitätsmods.
- Die Vorschläge sollen direkt im Profil-/Content-Editor sichtbar sein und optional einsetzbar sein.

## Architektur

### 1. Empfohlungs-Definition

Eine kleine, zentrale Datenstruktur definiert für jedes bekannte Setup:

- Minecraft-Version
- Loader (`fabric`, `vanilla`, `neoforge`)
- Projektliste mit `projectId`, `label`, `reason`, `priority`
- feste Sicherheits- und kompatible Auswahlregeln

### 2. Vorschau im UI

Im Profil-/Content-Editor erscheint ein Abschnitt:

- "Recommended safe defaults"
- Liste mit kurzen Begründungen
- Toggle pro Vorschlag oder Batch-Aktion
- Nur aktivierbar, wenn die Runtime/Version bereits gesetzt ist

### 3. Installationsfluss

- Vorschläge werden nur als echte Modrinth-Projekte installiert, wenn sie kompatibel und aufgelöst sind.
- Die Installation läuft über den bestehenden `contentCommands.install`-Pfad und nicht über einen separaten, unsicheren Pfad.
- Wenn die sichere Auflösung fehlschlägt, bleibt der alte Profilzustand aktiv.

## Empfohlene Safe Defaults

### Fabric

- Sodium
- Iris
- Lithium
- Fabric API
- Mod Menu

### Vanilla

- Sodium
- Lithium
- Fabric API (nur falls Loader als Fabric gewählt)
- kleine Optimierungsbundle nur bei kompatibler Version

### NeoForge

- Embeddium
- Oculus
- Lazy DFU
- NeoForge-kompatible Performance- und QoL-Pakete nach Verifizierung

## Umsetzungsschritte

1. Dateistruktur für Vorschlagsdaten anlegen.
2. Vorschlagslogik an den aktiven Runtime-Kontext koppeln.
3. Banner/Abschnitt im ContentEditor ergänzen.
4. Batch-Installations- und Einzelinstallations-Button ergänzen.
5. Sicherheits- und Kompatibilitäts-Testrunde ausführen.

## Qualitätsregeln

- Keine ungesicherte Download-URL oder unvalidierte Mod-ID.
- Keine Vorschläge ohne nachvollziehbare Loader-/Version-Kompatibilität.
- Keine Vorschläge, die gegen die Profil-/Target-Sicherheitsregeln verstoßen.
- Standard-Mods müssen ebenfalls die bestehende Revisionslogik respektieren.

## Verifikation

- UI-Tests für den Vorschau-Abschnitt
- install-flow tests für ein virtuelles recommended bundle
- Rust/Runtime-Tests für API-Sicherheits- und Versions-Gates
