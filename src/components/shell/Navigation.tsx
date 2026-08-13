import { useEffect, useLayoutEffect, useRef, useState } from "react";
import {
  CircleUserRound,
  Compass,
  Download,
  MonitorPlay,
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
import { BrandMark } from "../brand/BrandMark";
import type { TranslationKey } from "../../i18n/messages";

interface Item { page: ShellPage; key: TranslationKey; icon: LucideIcon }
const primary: Item[] = [
  { page: "home", key: "nav.home", icon: Home },
  { page: "library", key: "nav.library", icon: Library },
  { page: "discover", key: "nav.discover", icon: Compass },
  { page: "cosmetics", key: "nav.cosmetics", icon: Sparkles },
  { page: "instances", key: "nav.instances", icon: MonitorPlay },
];
const secondary: Item[] = [
  { page: "tasks", key: "nav.tasks", icon: Download },
  { page: "updates", key: "nav.updates", icon: RefreshCcw },
  { page: "accounts", key: "nav.accounts", icon: CircleUserRound },
  { page: "settings", key: "nav.settings", icon: Settings },
  { page: "diagnostics", key: "nav.diagnostics", icon: ScrollText },
];
const allItems = [...primary, ...secondary];
const mobileNavigationQuery = "(max-width: 860px)";

function useMobileNavigation(): boolean {
  const [mobile, setMobile] = useState(() => window.matchMedia(mobileNavigationQuery).matches);

  useEffect(() => {
    const media = window.matchMedia(mobileNavigationQuery);
    const update = () => setMobile(media.matches);
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);

  return mobile;
}

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
        return <button type="button" key={item.page} className="shell-nav__item" aria-current={page === item.page ? "page" : undefined} aria-label={text} title={text} onClick={() => { setPage(item.page); setMobileOpen(false); }}><Icon aria-hidden="true" /><span>{text}</span></button>;
      })}
    </nav>
  );
}

export function Navigation() {
  const { t } = useI18n();
  const settings = useShellStore((state) => state.settings);
  const page = useShellStore((state) => state.page);
  const saveSettings = useShellStore((state) => state.saveSettings);
  const mobileOpen = useShellStore((state) => state.mobileNavigationOpen);
  const setMobileOpen = useShellStore((state) => state.setMobileNavigationOpen);
  const expanded = settings.navigationMode === "expanded";
  const mobile = useMobileNavigation();
  const drawerOpen = mobile && mobileOpen;
  const panel = useRef<HTMLElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);
  const pageWhenOpened = useRef<ShellPage | null>(null);
  const currentItem = allItems.find((item) => item.page === page) ?? primary[0];

  useEffect(() => {
    if (!mobile && mobileOpen) setMobileOpen(false);
  }, [mobile, mobileOpen, setMobileOpen]);

  useLayoutEffect(() => {
    if (!drawerOpen) return;
    previousFocus.current = document.activeElement as HTMLElement;
    pageWhenOpened.current = page;
    const focusable = [...(panel.current?.querySelectorAll<HTMLElement>("button:not([disabled]), a[href], [tabindex]:not([tabindex='-1'])") ?? [])];
    focusable[0]?.focus();

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        setMobileOpen(false);
        return;
      }
      if (event.key !== "Tab" || focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };

    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      const restoreTarget = previousFocus.current;
      if (useShellStore.getState().page === pageWhenOpened.current) {
        requestAnimationFrame(() => restoreTarget?.focus());
      }
    };
  }, [drawerOpen, page, setMobileOpen]);

  return (
    <>
      {drawerOpen ? <button type="button" className="shell-nav__scrim" aria-label={t("app.close")} onClick={() => setMobileOpen(false)} /> : null}
      <aside
        ref={panel}
        className={`shell-nav ${drawerOpen ? "shell-nav--mobile-open" : ""}`}
        aria-label={t("navigation.primary")}
        aria-hidden={mobile && !drawerOpen ? true : undefined}
        inert={mobile ? !drawerOpen : undefined}
      >
        <div className="shell-nav__brand">
          <BrandMark aria-hidden="true" />
          <span className="shell-nav__wordmark"><strong>{t("brand.name")}</strong><small>{t("brand.product")}</small></span>
          <IconButton className="shell-nav__mobile-close" label={t("app.close")} onClick={() => setMobileOpen(false)}><X aria-hidden="true" /></IconButton>
        </div>
        <NavGroup label={t("navigation.primary")} items={primary} />
        <div className="shell-nav__separator" />
        <NavGroup label={t("navigation.secondary")} items={secondary} />
        <div className="shell-nav__footer">
          <button type="button" className="shell-nav__item" aria-label={expanded ? t("navigation.compact") : t("navigation.expand")} title={expanded ? t("navigation.compact") : t("navigation.expand")} onClick={() => void saveSettings({ ...settings, navigationMode: expanded ? "compact" : "expanded" })}>
            {expanded ? <ListChecks aria-hidden="true" /> : <Menu aria-hidden="true" />}
            <span>{expanded ? t("navigation.compact") : t("navigation.expand")}</span>
          </button>
        </div>
      </aside>
      <span className="sr-only" aria-live="polite" aria-atomic="true">
        {t("nav.current", { page: t(currentItem.key) })}
      </span>
    </>
  );
}
