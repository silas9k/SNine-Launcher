import { useReleaseText } from "../i18n/releaseUiText";
import { uiStorage } from "../lib/uiStorage";
import {
  ChevronDown,
  Eye,
  Gamepad2,
  Languages,
  MessageCircle,
  Palette,
  RotateCcw,
  Sparkles,
  Type,
  XCircle,
  type LucideIcon,
} from "lucide-react";
import { useState, type ReactNode } from "react";
import { useShellStore } from "../app/shellStore";
import { useI18n } from "../i18n/I18nProvider";
import type { LocaleSetting } from "../theme/types";
import {
  DEFAULT_LAUNCHER_BACKGROUND,
  loadLauncherBackground,
  resetLauncherBackground,
  saveLauncherBackground,
} from "../theme/launcherBackground";
import {
  DEFAULT_LAUNCHER_FONT,
  loadLauncherFont,
  resetLauncherFont,
  saveLauncherFont,
  type LauncherFontChoice,
} from "../theme/launcherFont";
import {
  DEFAULT_LAUNCHER_FONT_SCALE,
  loadLauncherFontScale,
  MAX_LAUNCHER_FONT_SCALE,
  MIN_LAUNCHER_FONT_SCALE,
  resetLauncherFontScale,
  saveLauncherFontScale,
} from "../theme/launcherFontSize";
import {
  DEFAULT_LAUNCHER_CORNER_RADIUS,
  loadLauncherCornerRadius,
  MAX_LAUNCHER_CORNER_RADIUS,
  MIN_LAUNCHER_CORNER_RADIUS,
  resetLauncherCornerRadius,
  saveLauncherCornerRadius,
} from "../theme/launcherCorners";
import {
  applyDiscordRpcPreference,
  loadLauncherPreferences,
  resetLauncherPreferences,
  saveLauncherPreferences,
  type LauncherPreferences,
} from "../theme/launcherPreferences";

const COLLAPSED_SECTIONS_STORAGE_KEY = "snine.launcher.settings.collapsedSections";
const DEFAULT_COLLAPSED_SECTIONS: Record<string, boolean> = {
  "launcher-game": true,
  "player-preview": true,
  integrations: true,
  appearance: true,
};

function formatPercent(value: number) {
  return `${Math.round(value * 100)}%`;
}

function formatPixels(value: number) {
  return `${Math.round(value)}px`;
}

function loadCollapsedSections(): Record<string, boolean> {
  if (typeof window === "undefined") return { ...DEFAULT_COLLAPSED_SECTIONS };
  try {
    const raw = uiStorage.getItem(COLLAPSED_SECTIONS_STORAGE_KEY);
    if (!raw) return { ...DEFAULT_COLLAPSED_SECTIONS };
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    return Object.fromEntries(
      Object.keys(DEFAULT_COLLAPSED_SECTIONS).map((key) => [key, typeof parsed?.[key] === "boolean" ? parsed[key] : DEFAULT_COLLAPSED_SECTIONS[key]]),
    ) as Record<string, boolean>;
  } catch {
    return { ...DEFAULT_COLLAPSED_SECTIONS };
  }
}

function saveCollapsedSectionsState(next: Record<string, boolean>) {
  if (typeof window === "undefined") return;
  try {
    uiStorage.setItem(COLLAPSED_SECTIONS_STORAGE_KEY, JSON.stringify(next));
  } catch {
    // ignore storage write failures
  }
}

function CollapsibleSettingsSection({
  id,
  icon: Icon,
  title,
  description,
  collapsed,
  onToggle,
  children,
}: {
  id: string;
  icon: LucideIcon;
  title: string;
  description: string;
  collapsed: boolean;
  onToggle: () => void;
  children: ReactNode;
}) {
  const contentId = `snine-settings-section-${id}`;

  return (
    <section className={`snine-settings-section${collapsed ? " is-collapsed" : ""}`}>
      <header>
        <Icon aria-hidden="true" />
        <div className="snine-settings-section__heading-copy">
          <h2>{title}</h2>
          <p>{description}</p>
        </div>
        <button
          type="button"
          className="snine-settings-section__toggle"
          onClick={onToggle}
          aria-expanded={!collapsed}
          aria-controls={contentId}
          aria-label={`${collapsed ? "Ausklappen" : "Einklappen"}: ${title}`}
          title={collapsed ? "Kategorie ausklappen" : "Kategorie einklappen"}
        >
          <ChevronDown aria-hidden="true" />
        </button>
      </header>
      <div id={contentId} className="snine-settings-section__content" hidden={collapsed}>
        {children}
      </div>
    </section>
  );
}

function SettingsRow({ icon: Icon, title, description, children, slider = false }: {
  icon: LucideIcon;
  title: string;
  description: string;
  children: ReactNode;
  slider?: boolean;
}) {
  return (
    <article className={`snine-settings-card${slider ? " snine-settings-card--slider" : ""}`}>
      <div className="snine-settings-card__icon"><Icon aria-hidden="true" /></div>
      <div className="snine-settings-card__copy">
        <strong>{title}</strong>
        <span>{description}</span>
      </div>
      {children}
    </article>
  );
}

function Toggle({ checked, label, onChange }: { checked: boolean; label: string; onChange: (checked: boolean) => void }) {
  return (
    <label className="snine-settings-toggle" title={label}>
      <input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} aria-label={label} />
      <span aria-hidden="true" />
    </label>
  );
}

export function SettingsPage() {
  const rt = useReleaseText();
  const { t } = useI18n();
  const settings = useShellStore((state) => state.settings);
  const saveSettings = useShellStore((state) => state.saveSettings);
  const resetSettings = useShellStore((state) => state.resetSettings);
  const loading = useShellStore((state) => state.loading);
  const [preferences, setPreferences] = useState<LauncherPreferences>(() => loadLauncherPreferences());
  const [backgroundColor, setBackgroundColor] = useState(() => loadLauncherBackground());
  const [fontChoice, setFontChoice] = useState<LauncherFontChoice>(() => loadLauncherFont());
  const [fontScale, setFontScale] = useState(() => loadLauncherFontScale());
  const [cornerRadius, setCornerRadius] = useState(() => loadLauncherCornerRadius());
  const [collapsedSections, setCollapsedSections] = useState<Record<string, boolean>>(() => loadCollapsedSections());

  const toggleSection = (id: string) => {
    setCollapsedSections((current) => {
      const next = { ...current, [id]: !current[id] };
      saveCollapsedSectionsState(next);
      return next;
    });
  };

  const resetCollapsedSections = () => {
    const next = { ...DEFAULT_COLLAPSED_SECTIONS };
    saveCollapsedSectionsState(next);
    setCollapsedSections(next);
  };

  const update = <K extends keyof typeof settings>(key: K, value: (typeof settings)[K]) => {
    void saveSettings({ ...settings, [key]: value });
  };

  const updatePreference = <K extends keyof LauncherPreferences>(key: K, value: LauncherPreferences[K]) => {
    const saved = saveLauncherPreferences({ ...preferences, [key]: value });
    setPreferences(saved);
    if (key === "discordRpc") {
      void applyDiscordRpcPreference(Boolean(value)).catch((error) => {
        console.warn("[SNine Launcher] Discord RPC toggle failed", error);
      });
    }
  };

  const resetAll = async () => {
    resetLauncherBackground();
    setBackgroundColor(DEFAULT_LAUNCHER_BACKGROUND);
    resetLauncherFont();
    setFontChoice(DEFAULT_LAUNCHER_FONT);
    resetLauncherFontScale();
    setFontScale(DEFAULT_LAUNCHER_FONT_SCALE);
    resetLauncherCornerRadius();
    setCornerRadius(DEFAULT_LAUNCHER_CORNER_RADIUS);
    const defaultPreferences = resetLauncherPreferences();
    setPreferences(defaultPreferences);
    resetCollapsedSections();
    void applyDiscordRpcPreference(defaultPreferences.discordRpc).catch(() => undefined);
    await resetSettings();
  };

  return (
    <section className="snine-settings-page" aria-label={t("settings.title")}>
      <div className="snine-settings-page__inner">
        <header className="snine-settings-page__heading">
          <div>
            <small>{rt("SNINE LAUNCHER / EINSTELLUNGEN")}</small>
            <h1>{rt("Launcher anpassen")}</h1>
            <p>{rt("Verhalten, Spieler-Vorschau, Integrationen und Design getrennt verwalten.")}</p>
          </div>
          <button type="button" onClick={() => void resetAll()} disabled={loading}>
            <RotateCcw aria-hidden="true" />
            {rt("ALLES ZURÜCKSETZEN")}
          </button>
        </header>

        <div className="snine-settings-sections">
          <CollapsibleSettingsSection
            id="launcher-game"
            icon={Gamepad2}
            title={rt("Launcher & Spiel")}
            description="Was beim Starten und Aktualisieren passieren soll."
            collapsed={Boolean(collapsedSections["launcher-game"])}
            onToggle={() => toggleSection("launcher-game")}
          >
            <div className="snine-settings-grid">
              <SettingsRow icon={Gamepad2} title={t("instances.showSnapshots")} description={t("instances.showSnapshotsDescription")}>
                <Toggle checked={settings.showMinecraftSnapshots} label={t("instances.showSnapshots")} onChange={(value) => update("showMinecraftSnapshots", value)} />
              </SettingsRow>
              <SettingsRow icon={Gamepad2} title={t("instances.showOld")} description={t("instances.showOldDescription")}>
                <Toggle checked={settings.showOldMinecraftVersions} label={t("instances.showOld")} onChange={(value) => update("showOldMinecraftVersions", value)} />
              </SettingsRow>
              <SettingsRow icon={XCircle} title={rt("Launcher schließen, wenn Minecraft startet")} description="Beendet den Launcher automatisch, sobald das Spiel wirklich läuft.">
                <Toggle checked={preferences.closeOnLaunch} label="Launcher nach Spielstart schließen" onChange={(value) => updatePreference("closeOnLaunch", value)} />
              </SettingsRow>
            </div>
          </CollapsibleSettingsSection>

          <CollapsibleSettingsSection
            id="player-preview"
            icon={Eye}
            title={rt("Spieler-Vorschau")}
            description="Lege fest, was am 3D-Modell im Launcher angezeigt wird."
            collapsed={Boolean(collapsedSections["player-preview"])}
            onToggle={() => toggleSection("player-preview")}
          >
            <div className="snine-settings-grid">
              <SettingsRow icon={Sparkles} title={rt("Cosmetics an der Vorschau anzeigen")} description="Blendet Cape, Wings, Bandana und andere Cosmetics am Modell ein oder aus.">
                <Toggle checked={preferences.showPreviewCosmetics} label="Cosmetics an Vorschau anzeigen" onChange={(value) => updatePreference("showPreviewCosmetics", value)} />
              </SettingsRow>
              <SettingsRow icon={Gamepad2} title={rt("Vorschau-Animationen")} description="Aktiviert die ruhige Idle-Bewegung des Skin-Modells.">
                <Toggle checked={preferences.previewAnimations} label="Vorschau-Animationen" onChange={(value) => updatePreference("previewAnimations", value)} />
              </SettingsRow>
            </div>
          </CollapsibleSettingsSection>

          <CollapsibleSettingsSection
            id="integrations"
            icon={MessageCircle}
            title={rt("Integrationen")}
            description="Verbindungen zu externen Apps kontrollieren."
            collapsed={Boolean(collapsedSections.integrations)}
            onToggle={() => toggleSection("integrations")}
          >
            <div className="snine-settings-grid">
              <SettingsRow icon={MessageCircle} title={rt("Discord Rich Presence")} description="Zeigt in Discord an, dass du den SNine Launcher verwendest.">
                <Toggle checked={preferences.discordRpc} label="Discord Rich Presence" onChange={(value) => updatePreference("discordRpc", value)} />
              </SettingsRow>
            </div>
          </CollapsibleSettingsSection>

          <CollapsibleSettingsSection
            id="appearance"
            icon={Palette}
            title={rt("Darstellung")}
            description="Sprache, Schrift, Hintergrund und Abstände des Launchers."
            collapsed={Boolean(collapsedSections.appearance)}
            onToggle={() => toggleSection("appearance")}
          >
            <div className="snine-settings-grid">
              <SettingsRow icon={Languages} title={t("settings.language")} description={t("settings.languageDescription")}>
                <select value={settings.locale} onChange={(event) => update("locale", event.target.value as LocaleSetting)} aria-label={t("settings.language")}>
                  <option value="system">{t("settings.language.system")}</option>
                  <option value="de">{t("settings.language.de")}</option>
                  <option value="en">{t("settings.language.en")}</option>
                </select>
              </SettingsRow>

              <SettingsRow icon={Type} title={t("launcher.settings.font")} description={t("launcher.settings.fontDescription")}>
                <select value={fontChoice} onChange={(event) => { const saved = saveLauncherFont(event.target.value as LauncherFontChoice); setFontChoice(saved); }} aria-label={t("launcher.settings.font")}>
                  <option value="minecraft">{t("launcher.settings.fontMinecraft")}</option>
                  <option value="launcher">{t("launcher.settings.fontLauncher")}</option>
                </select>
              </SettingsRow>

              <SettingsRow slider icon={Type} title={t("launcher.settings.fontSize")} description={t("launcher.settings.fontSizeDescription")}>
                <label className="snine-slider-control">
                  <input type="range" min={MIN_LAUNCHER_FONT_SCALE} max={MAX_LAUNCHER_FONT_SCALE} step={0.05} value={fontScale} onChange={(event) => { const saved = saveLauncherFontScale(Number(event.target.value)); setFontScale(saved); }} aria-label={t("launcher.settings.fontSize")} />
                  <span>{formatPercent(fontScale)}</span>
                </label>
              </SettingsRow>

              <SettingsRow icon={Palette} title={t("launcher.settings.backgroundColor")} description={t("launcher.settings.backgroundColorDescription")}>
                <label className="snine-background-picker" title={backgroundColor}>
                  <input type="color" value={backgroundColor} onChange={(event) => { const saved = saveLauncherBackground(event.target.value); setBackgroundColor(saved); }} aria-label={t("launcher.settings.backgroundColor")} />
                  <span>{backgroundColor.toUpperCase()}</span>
                </label>
              </SettingsRow>

              <SettingsRow slider icon={Palette} title={t("launcher.settings.cornerRadius")} description={t("launcher.settings.cornerRadiusDescription")}>
                <label className="snine-slider-control">
                  <input type="range" min={MIN_LAUNCHER_CORNER_RADIUS} max={MAX_LAUNCHER_CORNER_RADIUS} step={1} value={cornerRadius} onChange={(event) => { const saved = saveLauncherCornerRadius(Number(event.target.value)); setCornerRadius(saved); }} aria-label={t("launcher.settings.cornerRadius")} />
                  <span>{formatPixels(cornerRadius)}</span>
                </label>
              </SettingsRow>

              <SettingsRow icon={Sparkles} title={t("settings.motion")} description={t("settings.motionDescription")}>
                <Toggle checked={settings.reducedMotion} label={t("settings.motion")} onChange={(value) => update("reducedMotion", value)} />
              </SettingsRow>
            </div>
          </CollapsibleSettingsSection>
        </div>
      </div>
    </section>
  );
}
