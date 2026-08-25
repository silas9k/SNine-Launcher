# Phase 10 – Release, Quellpaket und Rollback

## Erzeugung

`CREATE-FINAL-DELIVERY.ps1` arbeitet nur auf einem sauberen Git-Commit. Das
vollständige Quellpaket wird mit `git archive` erzeugt und erhält genau den
Stammordner `S9Lab-Launcher-v1.0.8-final-source`. Eine separate SHA-256-Datei
bindet Name und Inhalt.

Das Delta ist an den ursprünglichen Baseline-Commit
`e1412af46abfcd6dc401d4d97c5c3c402ba1491b` und den Zielcommit gebunden. Jede
geänderte oder gelöschte Datei besitzt einen Basishash; jede neue oder geänderte
Datei Zielhash und Größe.

## Sicheres Anwenden

`APPLY-S9LAB-DELTA.ps1` prüft vor der ersten Mutation:

- Format, Commitbindung und vollständigen ZIP-Inhalt,
- Traversal, absolute Pfade, ADS/Colon, Steuerzeichen und Case-Kollisionen,
- Reparse Points im Zielpfad,
- sauberen Basiscommit bei einem Git-Ziel,
- alle Basis-, Payload-, Größen- und Zielhashes.

Erst danach werden vorhandene Dateien in einen neuen TEMP-Backupbaum kopiert.
Ein Teilfehler entfernt neue Dateien und stellt geänderte/gelöschte Dateien
zurück. Es gibt keine rekursive Löschung im Benutzerprojekt.

## Binärrollback

Der Launcher-Updater bleibt ohne signierte Produktionsquelle deaktiviert. Ein
öffentlicher Binärrollback ist daher nicht freigegeben. Lokal stehen Git-
Checkpoints, vollständiges Quellarchiv, Baseline-gebundenes Delta sowie die
profilbezogenen Phase-7-Backups/Restores zur Verfügung. Private Schlüssel,
Signierung, Upload und Veröffentlichung sind kein Teil dieser Pipeline.
