import {
  Blocks,
  Boxes,
  CircleUserRound,
  Cloud,
  FolderArchive,
  Gauge,
  Home,
  Server,
  Settings,
  Sparkles,
  UserCog,
  type LucideIcon,
} from "lucide-react";
import { useShellStore, type ShellPage } from "../../app/shellStore";
import { useI18n } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";

interface Item { page: ShellPage; key: TranslationKey; icon: LucideIcon }
const items: Item[] = [
  { page: "home", key: "nav.home", icon: Home },
  { page: "profiles", key: "nav.profiles", icon: Boxes },
  { page: "cosmetics", key: "nav.cosmetics", icon: Sparkles },
  { page: "skins", key: "nav.skins", icon: UserCog },
  { page: "mods", key: "nav.mods", icon: Blocks },
  { page: "modpacks", key: "nav.modpacks", icon: FolderArchive },
  { page: "shaders", key: "nav.shaders", icon: Cloud },
  { page: "resourcepacks", key: "nav.resourcepacks", icon: FolderArchive },
  { page: "servers", key: "nav.servers", icon: Server },
  { page: "stats", key: "nav.stats", icon: Gauge },
  { page: "accounts", key: "nav.accounts", icon: CircleUserRound },
  { page: "settings", key: "nav.settings", icon: Settings },
];

export function Navigation() {
  const { t } = useI18n();
  const page = useShellStore((state) => state.page);
  const setPage = useShellStore((state) => state.setPage);

  return (
    <aside className="shell-nav zenith-nav" aria-label={t("navigation.primary")}>
      <nav className="zenith-nav__track">
        {items.map((item) => {
          const Icon = item.icon;
          const label = t(item.key);
          const active = page === item.page;
          return (
            <button
              type="button"
              key={item.page}
              className="zenith-nav__item"
              aria-current={active ? "page" : undefined}
              aria-label={label}
              title={label}
              onClick={() => setPage(item.page)}
            >
              <Icon aria-hidden="true" />
              <span>{label}</span>
            </button>
          );
        })}
      </nav>
    </aside>
  );
}
