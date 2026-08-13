import { useEffect } from "react";
import { applyShellTheme } from "./theme/applyTheme";
import { LoaderCircle } from "lucide-react";
import { AppErrorBoundary } from "./app/ErrorBoundary";
import { useShellStore } from "./app/shellStore";
import { Navigation } from "./components/shell/Navigation";
import { TaskCenter } from "./components/shell/TaskCenter";
import { TitleBar } from "./components/shell/TitleBar";
import { Toasts } from "./components/shell/Toasts";
import { Button, ConfirmDialog } from "./components/ui";
import { I18nProvider, useI18n } from "./i18n/I18nProvider";
import { HomePage } from "./pages/HomePage";
import { AccountsPage } from "./pages/AccountsPage";
import { LibraryPage } from "./pages/LibraryPage";
import { PlaceholderPage } from "./pages/PlaceholderPage";
import { SettingsPage } from "./pages/SettingsPage";
import { UpdatesPage } from "./pages/UpdatesPage";
import { CosmeticsPage } from "./pages/CosmeticsPage";
import { DiscoverPage } from "./pages/DiscoverPage";

function ShellContent() {
  const { t } = useI18n();
  const page = useShellStore((state) => state.page);
  const initialized = useShellStore((state) => state.initialized);
  const loading = useShellStore((state) => state.loading);
  const bootstrap = useShellStore((state) => state.bootstrap);
  const dialog = useShellStore((state) => state.dialog);
  const setDialog = useShellStore((state) => state.setDialog);
  const resetSettings = useShellStore((state) => state.resetSettings);
  const settings = useShellStore((state) => state.settings);

  useEffect(() => { void bootstrap(); }, [bootstrap]);
  useEffect(() => {
    const dark = window.matchMedia("(prefers-color-scheme: dark)");
    const motion = window.matchMedia("(prefers-reduced-motion: reduce)");
    const refresh = () => applyShellTheme(settings);
    dark.addEventListener("change", refresh);
    motion.addEventListener("change", refresh);
    return () => { dark.removeEventListener("change", refresh); motion.removeEventListener("change", refresh); };
  }, [settings]);

  const content = page === "home"
    ? <HomePage />
    : page === "settings"
      ? <SettingsPage />
      : page === "accounts"
        ? <AccountsPage />
        : page === "library"
          ? <LibraryPage />
          : page === "updates"
            ? <UpdatesPage />
          : page === "discover"
            ? <DiscoverPage />
          : page === "cosmetics"
            ? <CosmeticsPage />
          : <PlaceholderPage page={page} />;
  return (
    <AppErrorBoundary>
      <a className="skip-link" href="#main-content">{t("app.skipToContent")}</a>
      <div className="app-shell">
        <TitleBar />
        <Navigation />
        <main id="main-content" className="shell-content" tabIndex={0} aria-busy={!initialized || loading}>
          {!initialized ? <div className="shell-loading" role="status"><LoaderCircle className="ui-spin" aria-hidden="true" /><h1>{t("app.loading")}</h1><p>{t("app.loadingDescription")}</p></div> : content}
        </main>
        <TaskCenter />
        <Toasts />
        <div className="shell-live-region sr-only" aria-live="polite">{loading ? t("app.loading") : ""}</div>
        <ConfirmDialog open={dialog === "reset-settings"} title={t("settings.resetTitle")} description={t("settings.resetDescription")} confirmLabel={t("app.reset")} cancelLabel={t("app.cancel")} loading={loading} onClose={() => setDialog(null)} onConfirm={() => void resetSettings()} />
      </div>
    </AppErrorBoundary>
  );
}

export default function App() {
  const locale = useShellStore((state) => state.settings.locale);
  return <I18nProvider localeSetting={locale}><ShellContent /></I18nProvider>;
}
