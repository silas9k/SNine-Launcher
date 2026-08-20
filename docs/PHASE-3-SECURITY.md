# Phase 3: Sicherheitsmodell

## Authentifizierung

- Public-Client Device Code Flow gegen den Microsoft-Consumers-Endpunkt.
- `offline_access` dient nur der kontrollierten Erneuerung im Rust-Prozess.
- Keine Redirects, feste Connect-/Request-Zeitgrenzen und nur HTTPS-Endpunkte.
- Die anzeigbare Verifikations-URL muss Microsoft beziehungsweise Microsoft Online gehören, HTTPS verwenden und darf weder Zugangsdaten noch einen expliziten Port enthalten.
- Polling behandelt `authorization_pending`, begrenztes `slow_down`, Ablehnung, Ablauf und Abbruch typisiert.

## Besitz und Identität

Eine Sitzung wird erst persistiert, wenn der Minecraft-Login erfolgreich war, `/entitlements/mcstore` exakt ein bekanntes Java-Entitlement liefert und anschließend ein gültiges Minecraft-Profil aufgelöst wurde. Lookalike-Entitlements werden abgelehnt. Bei Refresh muss die normalisierte Profil-ID mit dem gespeicherten Account übereinstimmen.

## Secrets und Logs

- Secrets existieren nur im Rust-Speicher und im OS Credential Store.
- Der Frontendvertrag enthält keine Token- oder Device-Secret-Felder.
- SQLite enthält nur einen zufälligen opaken Vault-Verweis.
- Audit-Logs werden vor dem Schreiben und erneut beim Lesen gegen Bearer-, Access-, Refresh-, Device-, Identity- und RPS-Material redigiert.
- Fehler externer Dienste enthalten Dienstkennung und HTTP-Status, niemals Antwortkörper oder Token.

## Revoke/Logout

Lokales Logout löscht zuerst den Credential-Eintrag und anschließend transaktional die Metadaten. Falls die Metadatenlöschung fehlschlägt, wird das zuvor gelesene Secret wiederhergestellt. Fehlende oder beschädigte Vault-Einträge setzen den Account stabil auf `relogin-required`.

Eine serverseitige globale Microsoft-Account- oder Consent-Widerrufsfunktion ist keine lokale Launcher-Operation und wurde nicht vorgetäuscht. Phase 3 widerruft die lokale Session vollständig und erzwingt bei fehlenden Credentials eine neue Anmeldung.

