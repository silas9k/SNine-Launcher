import { Home, Minus, Settings as SettingsIcon, Square, X, Shirt } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { IconButton } from "../ui";
import { useI18n } from "../../i18n/I18nProvider";
import { BrandMark } from "../brand/BrandMark";
import { useShellStore } from "../../app/shellStore";

function invokeWindow(command: string): void {
  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) void invoke(command);
}

export function TitleBar() {
  const { t } = useI18n();
  const page = useShellStore((state) => state.page);
  const setPage = useShellStore((state) => state.setPage);

  return (
    <header className="shell-titlebar home-only-titlebar">
      <div className="home-only-titlebar__brand" data-tauri-drag-region>
        <span className="home-only-titlebar__logo"><BrandMark aria-hidden="true" /></span>
        <div><strong>SNINE LAUNCHER</strong></div>
      </div>

      <nav className="home-only-titlebar__tabs" aria-label="Launcher Navigation">
        <button type="button" className={page === "home" ? "is-active" : ""} onClick={() => setPage("home")}>
          <Home aria-hidden="true" />
          <span>{t("nav.home").toUpperCase()}</span>
        </button>
        <button type="button" className={page === "settings" ? "is-active" : ""} onClick={() => setPage("settings")}>
          <SettingsIcon aria-hidden="true" />
          <span>{t("nav.settings").toUpperCase()}</span>
        </button>
        <button type="button" className={page === "skins" ? "is-active" : ""} onClick={() => setPage("skins")}>
          <Shirt aria-hidden="true" />
          <span>SKINS</span>
        </button>
      </nav>

      <div className="home-only-titlebar__drag" data-tauri-drag-region onDoubleClick={() => invokeWindow("window_toggle_maximize")} />

      <div className="home-only-titlebar__controls">
        <IconButton label={t("titlebar.minimize")} onClick={() => invokeWindow("window_minimize")}><Minus aria-hidden="true" /></IconButton>
        <IconButton label={t("titlebar.maximize")} onClick={() => invokeWindow("window_toggle_maximize")}><Square aria-hidden="true" /></IconButton>
        <IconButton className="home-only-titlebar__close" label={t("titlebar.close")} onClick={() => invokeWindow("window_close")}><X aria-hidden="true" /></IconButton>
      </div>
    </header>
  );
}
