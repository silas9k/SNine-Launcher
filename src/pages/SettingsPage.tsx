import { Check, Palette, RotateCcw, ShieldCheck } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useShellStore } from "../app/shellStore";
import { Badge, Button, Card, SelectField, Switch, TextField } from "../components/ui";
import { useI18n } from "../i18n/I18nProvider";
import { resolveAccentPalette } from "../theme/accent";
import type { Appearance, BackgroundVariant, Density, LocaleSetting, NavigationMode } from "../theme/types";

export function SettingsPage() {
  const { t } = useI18n();
  const settings = useShellStore((state) => state.settings);
  const saveSettings = useShellStore((state) => state.saveSettings);
  const setDialog = useShellStore((state) => state.setDialog);
  const loading = useShellStore((state) => state.loading);
  const [accentDraft, setAccentDraft] = useState(settings.accentColor);
  const palette = useMemo(() => resolveAccentPalette(accentDraft), [accentDraft]);

  useEffect(() => {
    setAccentDraft(settings.accentColor);
  }, [settings.accentColor]);

  const update = <K extends keyof typeof settings>(key: K, value: (typeof settings)[K]) => {
    void saveSettings({ ...settings, [key]: value });
  };

  const accentMessage = !palette.valid
    ? t("settings.accentInvalid")
    : palette.adjusted
      ? t("settings.accentAdjusted", { input: accentDraft, output: palette.accent })
      : t("settings.accentValid");

  return (
    <div className="page settings-page">
      <header className="page-heading"><div><p className="page-eyebrow">{t("app.name")}</p><h1>{t("settings.title")}</h1><p>{t("settings.description")}</p></div><Button onClick={() => setDialog("reset-settings")}><RotateCcw aria-hidden="true" />{t("app.reset")}</Button></header>
      <div className="settings-layout">
        <div className="settings-sections">
          <Card className="settings-section">
            <header><Palette aria-hidden="true" /><div><h2>{t("settings.appearanceSection")}</h2><p>{t("settings.appearanceDescription")}</p></div></header>
            <div className="settings-grid">
              <SelectField label={t("settings.appearance")} description={t("settings.appearanceDescription")} value={settings.appearance} onChange={(event) => update("appearance", event.target.value as Appearance)}>
                <option value="system">{t("settings.appearance.system")}</option><option value="light">{t("settings.appearance.light")}</option><option value="dark">{t("settings.appearance.dark")}</option><option value="contrast">{t("settings.appearance.contrast")}</option>
              </SelectField>
              <SelectField label={t("settings.background")} description={t("settings.backgroundDescription")} value={settings.backgroundVariant} onChange={(event) => update("backgroundVariant", event.target.value as BackgroundVariant)}>
                <option value="calm">{t("settings.background.calm")}</option><option value="grid">{t("settings.background.grid")}</option><option value="terrain">{t("settings.background.terrain")}</option>
              </SelectField>
            </div>
            <div className="accent-control">
              <TextField label={t("settings.accent")} description={t("settings.accentDescription")} value={accentDraft} onChange={(event) => setAccentDraft(event.target.value)} error={palette.valid ? undefined : accentMessage} maxLength={7} spellCheck={false} />
              <div className="accent-control__result" role="status"><span className="accent-swatch" style={{ backgroundColor: palette.accent }} aria-hidden="true" /><span>{accentMessage}</span><Button variant="primary" disabled={!palette.valid || loading} onClick={() => { setAccentDraft(palette.accent); void saveSettings({ ...settings, accentColor: palette.accent }, "settings.saved"); }}><Check aria-hidden="true" />{t("app.save")}</Button></div>
            </div>
          </Card>

          <Card className="settings-section">
            <header><ShieldCheck aria-hidden="true" /><div><h2>{t("settings.layoutSection")}</h2><p>{t("settings.navigationDescription")}</p></div></header>
            <div className="settings-grid">
              <SelectField label={t("settings.density")} description={t("settings.densityDescription")} value={settings.density} onChange={(event) => update("density", event.target.value as Density)}>
                <option value="compact">{t("settings.density.compact")}</option><option value="comfortable">{t("settings.density.comfortable")}</option>
              </SelectField>
              <SelectField label={t("settings.navigation")} description={t("settings.navigationDescription")} value={settings.navigationMode} onChange={(event) => update("navigationMode", event.target.value as NavigationMode)}>
                <option value="compact">{t("settings.navigation.compact")}</option><option value="expanded">{t("settings.navigation.expanded")}</option>
              </SelectField>
            </div>
            <Switch label={t("settings.motion")} description={t("settings.motionDescription")} checked={settings.reducedMotion} onChange={(event) => update("reducedMotion", event.target.checked)} />
          </Card>

          <Card className="settings-section">
            <header><div><h2>{t("settings.languageSection")}</h2><p>{t("settings.languageDescription")}</p></div></header>
            <SelectField label={t("settings.language")} description={t("settings.languageDescription")} value={settings.locale} onChange={(event) => update("locale", event.target.value as LocaleSetting)}>
              <option value="system">{t("settings.language.system")}</option><option value="de">{t("settings.language.de")}</option><option value="en">{t("settings.language.en")}</option>
            </SelectField>
          </Card>
        </div>

        <Card className="settings-preview">
          <div className="settings-preview__visual"><span /><span /><span /></div>
          <Badge tone="success">{t("settings.previewBadge")}</Badge>
          <h2>{t("settings.previewTitle")}</h2>
          <p>{t("settings.previewDescription")}</p>
          <Button variant="primary" disabled>{t("app.confirm")}</Button>
        </Card>
      </div>
    </div>
  );
}
