# Phase 10 – Installation, Wartung und Deinstallation

## Isolationsmodell

`TEST-NSIS-LIFECYCLE.ps1` akzeptiert nur einen per SHA-256 gebundenen,
unsignierten Diagnoseinstaller und verlangt die ausdrückliche Option
`-AllowUnsignedDiagnosticInstaller`. Vor jedem Start werden HKCU und HKLM auf
eine bestehende S9Lab-Installation geprüft. Bei einem Treffer wird nichts
verändert.

Der Test installiert als `CurrentUser` in einen neuen GUID-Unterordner des
System-TEMP. `/NS` verhindert Startmenü- und Desktop-Verknüpfungen. Das
Programm wird nicht gestartet. Anschließend werden Programmversion,
Hauptprogramm, Uninstaller und HKCU-Uninstall-Eintrag geprüft.

## Wartungs-/Upgradepfad

Die Produktversion muss laut Masterplan 1.0.8 bleiben. Daher kann in diesem
Quellstand kein semantischer Upgradepfad von einer niedrigeren Produktversion
ehrlich erzeugt werden. Geprüft wird der von NSIS/Tauri bereitgestellte
In-place-`/UPDATE`-Wartungspfad derselben Version: Installation bleibt
vollständig und das Hauptprogramm hashidentisch. Ein echtes 1.0.8→Folgeversion-
Upgrade ist ein späteres Release-Gate.

## Deinstallation und Aufräumen

Der echte erzeugte Uninstaller läuft still im selben CurrentUser-Kontext. Der
Test wartet begrenzt auf Selbstlöschung und verlangt, dass Hauptprogramm,
Uninstaller und Uninstall-Registryeintrag verschwinden. Nur der zuvor absolut
aufgelöste Sandboxpfad unter System-TEMP wird anschließend rekursiv entfernt.
Fehler lösen zunächst eine Aufräumdeinstallation aus; sie werden nie als Erfolg
maskiert.
