import { useEffect } from "react";
import { LoaderCircle } from "lucide-react";
import { AppErrorBoundary } from "./app/ErrorBoundary";
import { useShellStore } from "./app/shellStore";
import { TitleBar } from "./components/shell/TitleBar";
import { Toasts } from "./components/shell/Toasts";
import { I18nProvider, useI18n } from "./i18n/I18nProvider";
import { HomePage } from "./pages/HomePage";
import { SettingsPage } from "./pages/SettingsPage";
import { SkinsPage } from "./pages/SkinsPage";
import { CapesPage } from "./pages/CapesPage";
import { MinecraftLogWindow } from "./pages/MinecraftLogWindow";
import { applyShellTheme } from "./theme/applyTheme";
import { applyLauncherBackground } from "./theme/launcherBackground";
import { applyLauncherFont } from "./theme/launcherFont";
import { applyLauncherFontScale } from "./theme/launcherFontSize";
import { applyLauncherCornerRadius } from "./theme/launcherCorners";
import { applyDiscordRpcPreference, loadLauncherPreferences } from "./theme/launcherPreferences";

function ShellContent() {
  const { t } = useI18n();
  const initialized = useShellStore((state) => state.initialized);
  const loading = useShellStore((state) => state.loading);
  const bootstrap = useShellStore((state) => state.bootstrap);
  const settings = useShellStore((state) => state.settings);
  const page = useShellStore((state) => state.page);

  useEffect(() => {
    applyLauncherBackground();
    void applyLauncherFont();
    applyLauncherFontScale();
    applyLauncherCornerRadius();
    void applyDiscordRpcPreference(loadLauncherPreferences().discordRpc).catch((error) => {
      console.warn("[SNine Launcher] Discord RPC preference could not be applied", error);
    });
  }, []);
  useEffect(() => { void bootstrap(); }, [bootstrap]);
  useEffect(() => {
    const dark = window.matchMedia("(prefers-color-scheme: dark)");
    const motion = window.matchMedia("(prefers-reduced-motion: reduce)");
    const refresh = () => applyShellTheme(settings);
    dark.addEventListener("change", refresh);
    motion.addEventListener("change", refresh);
    return () => {
      dark.removeEventListener("change", refresh);
      motion.removeEventListener("change", refresh);
    };
  }, [settings]);

  return (
    <AppErrorBoundary>
      <div className="app-shell app-shell--home-only">
        <TitleBar />
        <main id="main-content" className="shell-content shell-content--home-only" tabIndex={0} aria-busy={!initialized || loading}>
          {!initialized ? (
            <div className="shell-loading" role="status">
              <LoaderCircle className="ui-spin" aria-hidden="true" />
              <h1>{t("app.loading")}</h1>
            </div>
          ) : page === "settings" ? (
            <SettingsPage />
          ) : page === "skins" ? (
            <SkinsPage />
          ) : page === "capes" ? (
            <CapesPage />
          ) : (
            <HomePage />
          )}
        </main>
        <Toasts />
      </div>
    </AppErrorBoundary>
  );
}

function MainLauncherApp() {
  const locale = useShellStore((state) => state.settings.locale);
  return <I18nProvider localeSetting={locale}><ShellContent /></I18nProvider>;
}

export default function App() {
  const params = typeof window !== "undefined" ? new URLSearchParams(window.location.search) : null;
  return params?.get("snineWindow") === "minecraftLogs" ? <MinecraftLogWindow /> : <MainLauncherApp />;
}
