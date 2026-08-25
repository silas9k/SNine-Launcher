# Phase 5 – externe Produktionsblocker

Status: 2026-08-01. Diese Liste enthält ausschließlich Eingaben, die nicht sicher
aus dem lokalen Quellstand erfunden werden dürfen.

## Offizieller S9Lab-Komponentenprovider

Für eine produktiv verfügbare modulare S9Lab-Komponente fehlen:

1. eine offiziell freigegebene HTTPS-Origin unter S9Lab-Kontrolle,
2. ein neuer, nicht kompromittierter Ed25519-Produktionsschlüssel,
3. der zugehörige öffentliche Schlüssel mit stabiler Key-ID für den Launcherbuild,
4. ein serverseitig erzeugter und signierter Produktionskatalog,
5. kompatible, aus dem kontrollierten Provider ausgelieferte Fabric- und
   NeoForge-JARs.

Der private Schlüssel darf weder an Codex noch in dieses Repository, in normale
Backups oder in Buildartefakte gelangen. Er wird ausschließlich in der externen
Signierumgebung verwendet. Der Launcher benötigt nur den freigegebenen öffentlichen
Schlüssel.

Bis diese Werte vorliegen, ist der korrekte Produktzustand:

```text
capabilityId = s9lab.components
state        = unconfigured
```

Die UI bleibt deaktiviert und darf keine Testdomain, Raw-URL oder Testsignatur als
Produktionsersatz anbieten.

Der typisierte URL-freie Komponenten-Katalog und seine kompatibilitätsgefilterte
UI-Auswahl sind lokal implementiert. Sie ersetzen diese fehlenden
Produktionswerte nicht: Ohne konfigurierte Origin und Public Key liefert der
Rust-Kern eine leere Liste mit `unconfigured`, ohne das Netzwerk aufzurufen.

## NeoForge-Ausführungsgrenze

Der verifizierte Offline-Parser und der hashgebundene Installationsplan sind lokal
implementiert. Die untersuchte reale Installerform liefert für benötigte
Clientprozessoren jedoch nicht durchgehend vertrauenswürdige Output-Hashes und
verlangt für mindestens eine Aufgabe Netzwerkzugriff. Beides widerspricht dem
fail-closed Ausführungsmodell.

Das ist kein fehlender Schlüssel oder frei erfindbarer Produktionswert. Der
Launcher darf diese Lücke weder mit berechneten Sollwerten aus einer bereits
ausgeführten untrusted Operation noch mit unkontrolliertem Netzwerkzugriff
schließen. Eine sichere spätere Freigabe benötigt eine offiziell überprüfbare
Outputbindung und einen kompatiblen Ausführungsweg.

Unabhängig davon fehlt lokal noch eine Windows-Prozesssandbox, die
Netzwerkfreiheit, Prozessbaum-Timeout und eine exakte Schreib-Positivliste
garantiert. Dieser Teil ist eine interne Restarbeit und kein externer Blocker.

## Nicht externe Restarbeiten

- Managed Java benötigt noch eine kontrollierte, hashgebundene
  Beschaffungs- und Aktivierungskette. Bis dahin bleibt es ehrlich deaktiviert.
- Reale authentifizierte Laufzeit-End-to-End-Nachweise sind Prüfaufgaben und keine
  externen Produktkonfigurationen.

Die frühere Projektionslücke und die alphabetische Classpath-Umsortierung sind
bereits behoben: Revisionszeiger und Runtime-Projektion werden gemeinsam
transaktional aktualisiert, und der Classpath behält die aufgelöste
Metadatenreihenfolge. Auch die verwaisten Legacy-Quellen und gebündelten JARs sind
entfernt. Windows-Starts werden inzwischen vor dem ersten ausführbaren Instruktions-
schritt einem Kill-on-close-Job-Object zugeordnet. Diese Punkte dürfen nicht länger
als offene Blocker geführt werden.
