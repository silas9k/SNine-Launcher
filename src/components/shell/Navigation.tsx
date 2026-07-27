import {
  CircleUserRound,
  Compass,
  Download,
  Home,
  Library,
  ListChecks,
  Menu,
  RefreshCcw,
  ScrollText,
  Settings,
  Sparkles,
  X,
  type LucideIcon,
} from "lucide-react";
import { useShellStore, type ShellPage } from "../../app/shellStore";
import { useI18n } from "../../i18n/I18nProvider";
import { IconButton } from "../ui";
import logo from "../../assets/logo.png";
import type { TranslationKey } from "../../i18n/messages";

interface Item { page: ShellPage; key: TranslationKey; icon: LucideIcon }
const primary: Item[] = [
  { page: "home", key: "nav.home", icon: Home },
  { page: "library", key: "nav.library", icon: Library },
  { page: "discover", key: "nav.discover", icon: Compass },
  { page: "cosmetics", key: "nav.cosmetics", icon: Sparkles },
];
const secondary: Item[] = [
  { page: "tasks", key: "nav.tasks", icon: Download },
  { page: "updates", key: "nav.updates", icon: RefreshCcw },
  { page: "accounts", key: "nav.accounts", icon: CircleUserRound },
  { page: "settings", key: "nav.settings", icon: Settings },
  { page: "diagnostics", key: "nav.diagnostics", icon: ScrollText },
];

function NavGroup({ label, items }: { label: string; items: Item[] }) {
  const { t } = useI18n();
  const page = useShellStore((state) => state.page);
  const setPage = useShellStore((state) => state.setPage);
  const setMobileOpen = useShellStore((state) => state.setMobileNavigationOpen);
  return (
    <nav className="shell-nav__group" aria-label={label}>
      {items.map((item) => {
        const Icon = item.icon;
        const text = t(item.key);
        return <button key={item.page} className="shell-nav__item" aria-current={page === item.page ? "page" : undefined} aria-label={text} title={text} onClick={() => { setPage(item.page); setMobileOpen(false); }}><Icon aria-hidden="true" /><span>{text}</span></button>;
      })}
    </nav>
  );
}

export function Navigation() {
  const { t } = useI18n();
  const settings = useShellStore((state) => state.settings);
  const saveSettings = useShellStore((state) => state.saveSettings);
  const mobileOpen = useShellStore((state) => state.mobileNavigationOpen);
  const setMobileOpen = useShellStore((state) => state.setMobileNavigationOpen);
  const expanded = settings.navigationMode === "expanded";
  return (
    <>
      {mobileOpen ? <button className="shell-nav__scrim" aria-label={t("app.close")} onClick={() => setMobileOpen(false)} /> : null}
      <aside className={`shell-nav ${mobileOpen ? "shell-nav--mobile-open" : ""}`} aria-label={t("navigation.primary")}>
        <div className="shell-nav__brand">
          <img src={logo} alt="" aria-hidden="true" />
          <strong>{t("app.name")}</strong>
          <IconButton className="shell-nav__mobile-close" label={t("app.close")} onClick={() => setMobileOpen(false)}><X aria-hidden="true" /></IconButton>
        </div>
        <NavGroup label={t("navigation.primary")} items={primary} />
        <div className="shell-nav__separator" />
        <NavGroup label={t("navigation.secondary")} items={secondary} />
        <div className="shell-nav__footer">
          <button className="shell-nav__item" aria-label={expanded ? t("navigation.compact") : t("navigation.expand")} title={expanded ? t("navigation.compact") : t("navigation.expand")} onClick={() => void saveSettings({ ...settings, navigationMode: expanded ? "compact" : "expanded" })}>
            {expanded ? <ListChecks aria-hidden="true" /> : <Menu aria-hidden="true" />}
            <span>{expanded ? t("navigation.compact") : t("navigation.expand")}</span>
          </button>
        </div>
      </aside>
    </>
  );
}
