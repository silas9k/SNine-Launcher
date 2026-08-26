import { useOptionalI18n } from "./I18nProvider";

const RELEASE_UI_TEXT = {
  "PLAYER WIRD GELADEN": {
    "de": "PLAYER WIRD GELADEN",
    "en": "PLAYER LOADING"
  },
  "DRAG · ROTATE&nbsp;&nbsp; / &nbsp;&nbsp;DOUBLE CLICK · FRONT&nbsp;&nbsp; / &nbsp;&nbsp;SCROLL · ZOOM": {
    "de": "DRAG · ROTATE  /  DOUBLE CLICK · FRONT  /  SCROLL · ZOOM",
    "en": "DRAG · ROTATE  /  DOUBLE CLICK · FRONT  /  SCROLL · ZOOM"
  },
  "SNine Setup": {
    "de": "SNine Setup",
    "en": "SNine Setup"
  },
  "SNINE LAUNCHER": {
    "de": "SNINE LAUNCHER",
    "en": "SNINE LAUNCHER"
  },
  "Launcher Navigation": {
    "de": "Launcher-Navigation",
    "en": "Launcher navigation"
  },
  "SKINS": {
    "de": "SKINS",
    "en": "SKINS"
  },
  "CAPES": {
    "de": "CAPES",
    "en": "CAPES"
  },
  "CAPE GUIDELINES": {
    "de": "CAPE-RICHTLINIEN",
    "en": "CAPE GUIDELINES"
  },
  "Schließen": {
    "de": "Schließen",
    "en": "Close"
  },
  "BEFORE YOU UPLOAD": {
    "de": "VOR DEM UPLOAD",
    "en": "BEFORE YOU UPLOAD"
  },
  "Bitte lies und akzeptiere die Regeln für Custom Capes.": {
    "de": "Bitte lies und akzeptiere die Regeln für Custom Capes.",
    "en": "Please read and accept the rules for custom capes."
  },
  "Keine urheberrechtlich geschützten Capes von anderen Clients, Marken oder Spielen.": {
    "de": "Keine urheberrechtlich geschützten Capes von anderen Clients, Marken oder Spielen.",
    "en": "Do not upload copyrighted capes from other clients, brands, or games."
  },
  "Keine expliziten, beleidigenden oder diskriminierenden Inhalte.": {
    "de": "Keine expliziten, beleidigenden oder diskriminierenden Inhalte.",
    "en": "No explicit, offensive, or discriminatory content."
  },
  "Keine politischen oder extremistischen Inhalte.": {
    "de": "Keine politischen oder extremistischen Inhalte.",
    "en": "No political or extremist content."
  },
  "Keine Werbung, Scam-Links oder absichtlich irreführenden Motive.": {
    "de": "Keine Werbung, Scam-Links oder absichtlich irreführenden Motive.",
    "en": "No advertising, scam links, or intentionally misleading designs."
  },
  "Du musst das Recht haben, die hochgeladene Grafik zu verwenden.": {
    "de": "Du musst das Recht haben, die hochgeladene Grafik zu verwenden.",
    "en": "You must have the right to use the uploaded image."
  },
  "Die PNG muss im 2:1-Format vorliegen und darf maximal 512×256 Pixel groß sein.": {
    "de": "Die PNG muss im 2:1-Format vorliegen und darf maximal 512×256 Pixel groß sein.",
    "en": "The PNG must use a 2:1 aspect ratio and may be at most 512×256 pixels."
  },
  "Verstöße können zu einer temporären oder permanenten Upload-Sperre führen.": {
    "de": "Verstöße können zu einer temporären oder permanenten Upload-Sperre führen.",
    "en": "Violations may result in a temporary or permanent upload restriction."
  },
  "AKZEPTIEREN & WEITER": {
    "de": "AKZEPTIEREN & WEITER",
    "en": "ACCEPT & CONTINUE"
  },
  "CUSTOM CAPE": {
    "de": "CUSTOM CAPE",
    "en": "CUSTOM CAPE"
  },
  "Cape hochladen": {
    "de": "Cape hochladen",
    "en": "Upload cape"
  },
  "Cape Vorschau": {
    "de": "Cape-Vorschau",
    "en": "Cape preview"
  },
  "CAPE NAME": {
    "de": "CAPE-NAME",
    "en": "CAPE NAME"
  },
  "Mein Cape": {
    "de": "Mein Cape",
    "en": "My Cape"
  },
  "VORLAGE": {
    "de": "VORLAGE",
    "en": "TEMPLATE"
  },
  "NUR CAPE": {
    "de": "NUR CAPE",
    "en": "CAPE ONLY"
  },
  "CAPE + ELYTRA": {
    "de": "CAPE + ELYTRA",
    "en": "CAPE + ELYTRA"
  },
  "VORLAGE HERUNTERLADEN": {
    "de": "VORLAGE HERUNTERLADEN",
    "en": "DOWNLOAD TEMPLATE"
  },
  "Offizielles SNine Template · 512×256 PNG": {
    "de": "Offizielles SNine Template · 512×256 PNG",
    "en": "Official SNine template · 512×256 PNG"
  },
  "Die Datei wird vor dem Upload lokal geprüft.": {
    "de": "Die Datei wird vor dem Upload lokal geprüft.",
    "en": "The file is checked locally before upload."
  },
  "Nach dem Upload prüft ein SNine-Admin dein Cape über Discord.": {
    "de": "Nach dem Upload prüft ein SNine-Admin dein Cape über Discord.",
    "en": "After the upload, an SNine admin will review your cape via Discord."
  },
  "ABBRECHEN": {
    "de": "ABBRECHEN",
    "en": "CANCEL"
  },
  "HOCHLADEN": {
    "de": "HOCHLADEN",
    "en": "UPLOAD"
  },
  "Ansicht wählen": {
    "de": "Ansicht wählen",
    "en": "Choose view"
  },
  "VORNE": {
    "de": "VORNE",
    "en": "FRONT"
  },
  "RÜCKSEITE": {
    "de": "RÜCKSEITE",
    "en": "BACK"
  },
  "Ziehe zum Drehen, Mausrad zum Zoomen. Doppelklick setzt die Ansicht zurück.": {
    "de": "Ziehe zum Drehen, Mausrad zum Zoomen. Doppelklick setzt die Ansicht zurück.",
    "en": "Drag to rotate, scroll to zoom. Double-click resets the view."
  },
  "STATUS": {
    "de": "STATUS",
    "en": "STATUS"
  },
  "TYP": {
    "de": "TYP",
    "en": "TYPE"
  },
  "AKTIV": {
    "de": "AKTIV",
    "en": "ACTIVE"
  },
  "VERWENDUNGEN": {
    "de": "VERWENDUNGEN",
    "en": "USES"
  },
  "ABLEGEN": {
    "de": "ABLEGEN",
    "en": "UNEQUIP"
  },
  "AUSRÜSTEN": {
    "de": "AUSRÜSTEN",
    "en": "EQUIP"
  },
  "SCHLIESSEN": {
    "de": "SCHLIESSEN",
    "en": "CLOSE"
  },
  "SNINE LAUNCHER / COSMETICS": {
    "de": "SNINE LAUNCHER / COSMETICS",
    "en": "SNINE LAUNCHER / COSMETICS"
  },
  "Capes": {
    "de": "Capes",
    "en": "Capes"
  },
  "Entdecke kostenlose Community-Capes, lade dein eigenes hoch und schau dir jedes Cape direkt auf deinem Skin im Launcher an.": {
    "de": "Entdecke kostenlose Community-Capes, lade dein eigenes hoch und schau dir jedes Cape direkt auf deinem Skin im Launcher an.",
    "en": "Discover free community capes, upload your own, and preview every cape directly on your skin in the launcher."
  },
  "AKTIV:": {
    "de": "AKTIV:",
    "en": "ACTIVE:"
  },
  "Kein Minecraft-Account aktiv": {
    "de": "Kein Minecraft-Account aktiv",
    "en": "No active Minecraft account"
  },
  "Melde dich zuerst im Launcher mit deinem Minecraft-Account an.": {
    "de": "Melde dich zuerst im Launcher mit deinem Minecraft-Account an.",
    "en": "Sign in to the launcher with your Minecraft account first."
  },
  "CAPES WERDEN GELADEN...": {
    "de": "CAPES WERDEN GELADEN...",
    "en": "LOADING CAPES..."
  },
  "Favorit": {
    "de": "Favorit",
    "en": "Favorite"
  },
  "von": {
    "de": "von",
    "en": "by"
  },
  "ANSEHEN": {
    "de": "ANSEHEN",
    "en": "VIEW"
  },
  "MINECRAFT AKTIV": {
    "de": "MINECRAFT AKTIV",
    "en": "MINECRAFT ACTIVE"
  },
  "Vanilla Minecraft": {
    "de": "Vanilla Minecraft",
    "en": "Vanilla Minecraft"
  },
  "Keine Vanilla-Capes gefunden": {
    "de": "Keine Vanilla-Capes gefunden",
    "en": "No vanilla capes found"
  },
  "Hier erscheinen die offiziellen Capes deines Microsoft-/Minecraft-Accounts.": {
    "de": "Hier erscheinen die offiziellen Capes deines Microsoft-/Minecraft-Accounts.",
    "en": "Your official Microsoft/Minecraft account capes will appear here."
  },
  "SNINE // LOADOUT": {
    "de": "SNINE // LOADOUT",
    "en": "SNINE // LOADOUT"
  },
  "COSMETICS": {
    "de": "COSMETICS",
    "en": "COSMETICS"
  },
  "Dein ausgerüsteter SNine-Look. Live vom Backend, mit lokalem Snapshot-Fallback wenn die Verbindung fehlt.": {
    "de": "Dein ausgerüsteter SNine-Look. Live vom Backend, mit lokalem Snapshot-Fallback wenn die Verbindung fehlt.",
    "en": "Your equipped SNine look. Live from the backend, with a local snapshot fallback when the connection is unavailable."
  },
  "SYNC NOW": {
    "de": "JETZT SYNCHRONISIEREN",
    "en": "SYNC NOW"
  },
  "PLAYER": {
    "de": "SPIELER",
    "en": "PLAYER"
  },
  "DATA SOURCE": {
    "de": "DATENQUELLE",
    "en": "DATA SOURCE"
  },
  "EQUIPPED": {
    "de": "AUSGERÜSTET",
    "en": "EQUIPPED"
  },
  "Equipped cosmetics": {
    "de": "Ausgerüstete Cosmetics",
    "en": "Equipped cosmetics"
  },
  "LOADOUT SYNC": {
    "de": "LOADOUT-SYNC",
    "en": "LOADOUT SYNC"
  },
  "IDs, Modelle und Texturen werden geladen.": {
    "de": "IDs, Modelle und Texturen werden geladen.",
    "en": "IDs, models, and textures are being loaded."
  },
  "PLAYER RENDER": {
    "de": "SPIELER-RENDER",
    "en": "PLAYER RENDER"
  },
  "NO LOADOUT": {
    "de": "KEIN LOADOUT",
    "en": "NO LOADOUT"
  },
  "SNINE LAUNCHER / EINSTELLUNGEN": {
    "de": "SNINE LAUNCHER / EINSTELLUNGEN",
    "en": "SNINE LAUNCHER / SETTINGS"
  },
  "Launcher anpassen": {
    "de": "Launcher anpassen",
    "en": "Customize launcher"
  },
  "Verhalten, Spieler-Vorschau, Integrationen und Design getrennt verwalten.": {
    "de": "Verhalten, Spieler-Vorschau, Integrationen und Design getrennt verwalten.",
    "en": "Manage behavior, player preview, integrations, and appearance separately."
  },
  "ALLES ZURÜCKSETZEN": {
    "de": "ALLES ZURÜCKSETZEN",
    "en": "RESET ALL"
  },
  "Launcher & Spiel": {
    "de": "Launcher & Spiel",
    "en": "Launcher & Game"
  },
  "Launcher schließen, wenn Minecraft startet": {
    "de": "Launcher schließen, wenn Minecraft startet",
    "en": "Close launcher when Minecraft starts"
  },
  "Spieler-Vorschau": {
    "de": "Spieler-Vorschau",
    "en": "Player preview"
  },
  "Cosmetics an der Vorschau anzeigen": {
    "de": "Cosmetics an der Vorschau anzeigen",
    "en": "Show cosmetics in preview"
  },
  "Vorschau-Animationen": {
    "de": "Vorschau-Animationen",
    "en": "Preview animations"
  },
  "Integrationen": {
    "de": "Integrationen",
    "en": "Integrations"
  },
  "Discord Rich Presence": {
    "de": "Discord Rich Presence",
    "en": "Discord Rich Presence"
  },
  "Darstellung": {
    "de": "Darstellung",
    "en": "Appearance"
  },
  "SNINE LAUNCHER / SPIELER": {
    "de": "SNINE LAUNCHER / SPIELER",
    "en": "SNINE LAUNCHER / PLAYER"
  },
  "Deine Skin-Sammlung": {
    "de": "Deine Skin-Sammlung",
    "en": "Your skin collection"
  },
  "Importiere Minecraft-Skins, prüfe sie in 3D und wechsle dein Aussehen direkt im Launcher.": {
    "de": "Importiere Minecraft-Skins, prüfe sie in 3D und wechsle dein Aussehen direkt im Launcher.",
    "en": "Import Minecraft skins, preview them in 3D, and change your appearance directly in the launcher."
  },
  "SKIN HINZUFÜGEN": {
    "de": "SKIN HINZUFÜGEN",
    "en": "ADD SKIN"
  },
  "Skins durchsuchen...": {
    "de": "Skins durchsuchen...",
    "en": "Search skins..."
  },
  "Suche leeren": {
    "de": "Suche leeren",
    "en": "Clear search"
  },
  "SKINS GESPEICHERT": {
    "de": "SKINS GESPEICHERT",
    "en": "SKINS SAVED"
  },
  "MICROSOFT": {
    "de": "MICROSOFT",
    "en": "MICROSOFT"
  },
  "· ACCOUNT-SKIN": {
    "de": "· ACCOUNT-SKIN",
    "en": "· ACCOUNT SKIN"
  },
  "Neuen Skin hinzufügen": {
    "de": "Neuen Skin hinzufügen",
    "en": "Add new skin"
  },
  "PNG hochladen oder über Spielernamen importieren": {
    "de": "PNG hochladen oder über Spielernamen importieren",
    "en": "Upload a PNG or import using a player name"
  },
  "Arm-Modell wechseln": {
    "de": "Arm-Modell wechseln",
    "en": "Switch arm model"
  },
  "Skin löschen": {
    "de": "Skin löschen",
    "en": "Delete skin"
  },
  "Kein Skin passt zu „": {
    "de": "Kein Skin passt zu „",
    "en": "No skin matches “"
  },
  "NEUER EINTRAG": {
    "de": "NEUER EINTRAG",
    "en": "NEW ENTRY"
  },
  "Skin hinzufügen": {
    "de": "Skin hinzufügen",
    "en": "Add skin"
  },
  "ANZEIGENAME": {
    "de": "ANZEIGENAME",
    "en": "DISPLAY NAME"
  },
  "Zum Beispiel: Mein Main Skin": {
    "de": "Zum Beispiel: Mein Main Skin",
    "en": "For example: My Main Skin"
  },
  "SPIELERNAME ODER DIREKTE PNG-URL": {
    "de": "SPIELERNAME ODER DIREKTE PNG-URL",
    "en": "PLAYER NAME OR DIRECT PNG URL"
  },
  "silasO5xe oder https://.../skin.png": {
    "de": "silasO5xe oder https://.../skin.png",
    "en": "silasO5xe or https://.../skin.png"
  },
  "ODER": {
    "de": "ODER",
    "en": "OR"
  },
  "64×64 oder klassisch 64×32 Pixel": {
    "de": "64×64 oder klassisch 64×32 Pixel",
    "en": "64×64 or classic 64×32 pixels"
  },
  "ARM-MODELL": {
    "de": "ARM-MODELL",
    "en": "ARM MODEL"
  },
  "KLASSISCH": {
    "de": "KLASSISCH",
    "en": "CLASSIC"
  },
  "SCHLANK": {
    "de": "SCHLANK",
    "en": "SLIM"
  }
} as const;

export type ReleaseUiTextKey = keyof typeof RELEASE_UI_TEXT;

export function useReleaseText() {
  const i18n = useOptionalI18n();
  const locale = i18n?.locale ?? "en";

  return (key: ReleaseUiTextKey): string => RELEASE_UI_TEXT[key][locale];
}
