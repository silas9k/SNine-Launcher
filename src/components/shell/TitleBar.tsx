import { Download, Minus, PanelLeft, Square, X } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import logo from "../../assets/logo.png";
import { useI18n } from "../../i18n/I18nProvider";
import { IconButton } from "../ui";
import { useShellStore } from "../../app/shellStore";

function invokeWindow(command: string): void {
  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) void invoke(command);
}

export function TitleBar() {
  const { t } = useI18n();
  const setMobileNavigationOpen = useShellStore((state) => state.setMobileNavigationOpen);
  const setTaskCenterOpen = useShellStore((state) => state.setTaskCenterOpen);
  return (
    <header className="shell-titlebar" data-tauri-drag-region onDoubleClick={() => invokeWindow("window_toggle_maximize")}>
      <div className="shell-titlebar__brand" data-tauri-drag-region>
        <IconButton className="shell-titlebar__menu" label={t("navigation.expand")} onClick={() => setMobileNavigationOpen(true)}><PanelLeft aria-hidden="true" /></IconButton>
        <img src={logo} alt="" aria-hidden="true" />
        <div data-tauri-drag-region><strong>{t("app.name")}</strong><span>{t("titlebar.subtitle")}</span></div>
      </div>
      <div className="shell-titlebar__drag" data-tauri-drag-region />
      <div className="shell-titlebar__actions"><IconButton label={t("tasks.open")} onClick={() => setTaskCenterOpen(true)}><Download aria-hidden="true" /></IconButton></div>
      <div className="shell-titlebar__controls">
        <IconButton label={t("titlebar.minimize")} onClick={() => invokeWindow("window_minimize")}><Minus aria-hidden="true" /></IconButton>
        <IconButton label={t("titlebar.maximize")} onClick={() => invokeWindow("window_toggle_maximize")}><Square aria-hidden="true" /></IconButton>
        <IconButton className="shell-titlebar__close" label={t("titlebar.close")} onClick={() => invokeWindow("window_close")}><X aria-hidden="true" /></IconButton>
      </div>
    </header>
  );
}
