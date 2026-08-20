# Phase 9 – integrierter 3D-Spieler

Status: lokaler Checkpointstand vom 2026-08-11.

## Bühne

`PlayerStage` ersetzt den bisherigen Platzhalter direkt in der mittleren Spalte
der Hauptseite. Die Bühne besitzt keinen unnötigen Kartenrahmen und bleibt
zwischen Profilübersicht und Start-/Statusbereich Teil des bestehenden
Home-Grids. Auf schmalen Ansichten stapeln sich die drei Bereiche ohne
horizontalen Überlauf.

## Renderer und Assets

`skinview3d` wird dynamisch erst für die Bühne geladen. Skin und Cape stammen
aus eingebetteten lokalen SVG-Daten; es erfolgt kein Spieler-, Skin- oder
Cosmetic-Netzabruf. Die automatische Modellerkennung meldet Classic oder Slim.
Name und Icon beziehen sich nur auf das aktive lokale Basiskonto oder den
neutralen Namen S9Lab.

## Bedienung

Unterstützt werden freie Mausrotation, Pfeiltasten, Front, Rückseite,
Kamera-Reset, Skin-Layer, Cape, Wings, ohne Rückenelement, Halo sowie Idle-,
Walk- und Wave-Animation. Zoom und Verschieben sind deaktiviert. Beleuchtung,
Plattform und dezenter Halo grenzen die Figur ohne zusätzlichen Kasten ab.

## Zustände

Laden, bereit und Fallback werden sichtbar und semantisch gemeldet. Ohne WebGL
bleibt die bedienbare lokale Ersatzdarstellung stehen. Bei reduzierter Bewegung
werden Animationen ausgeschaltet. Der Renderer wird beim Verlassen vollständig
freigegeben und auf Größenänderungen begrenzt angepasst.
