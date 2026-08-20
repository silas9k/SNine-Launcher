# Phase 9 – Sicherheit und Privatsphäre

Die Bühne lädt ausschließlich integrierte Vorschau-Assets. Quellguard und Tests
weisen HTTP(S)-URLs, `fetch`, WebSocket und aktivierten Zoom im Spieler-/
Cosmetic-Pfad ab. Ein Accountfehler fällt auf den Namen S9Lab zurück; fremde
Spieler, frei eingegebene Nametags oder Suchpfade existieren nicht.

Der Cosmetics-Bereich behauptet keinen Besitz. Cape, Wings und Halo sind als
lokale Vorschau-Assets gekennzeichnet. Eine spätere Besitzanzeige darf erst mit
einer bestätigten S9Lab-Identität und serverseitig verifizierter Entitlement-
Antwort aktiviert werden.

WebGL-Ausfall wird abgefangen. Pixelratio und Resize sind begrenzt, Zoom/Pan
sind abgeschaltet, reduzierte Bewegung wird respektiert und die große
Renderer-Abhängigkeit ist vom Shell-Bundle getrennt.
