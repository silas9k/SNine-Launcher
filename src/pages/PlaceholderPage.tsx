import { Download, ScrollText, type LucideIcon } from "lucide-react";
import type { ShellPage } from "../app/shellStore";
import { Card, EmptyState } from "../components/ui";
import { useI18n } from "../i18n/I18nProvider";
import type { TranslationKey } from "../i18n/messages";

interface Copy { title: TranslationKey; description: TranslationKey; emptyTitle: TranslationKey; emptyDescription: TranslationKey; icon: LucideIcon }
const copy: Record<Exclude<ShellPage, "home" | "settings" | "accounts" | "library" | "updates" | "cosmetics" | "discover">, Copy> = {
  tasks: { title: "page.tasks.title", description: "page.tasks.description", emptyTitle: "tasks.emptyTitle", emptyDescription: "tasks.emptyDescription", icon: Download },
  diagnostics: { title: "page.diagnostics.title", description: "page.diagnostics.description", emptyTitle: "page.diagnostics.emptyTitle", emptyDescription: "page.diagnostics.emptyDescription", icon: ScrollText },
};

export function PlaceholderPage({ page }: { page: Exclude<ShellPage, "home" | "settings" | "accounts" | "library" | "updates" | "cosmetics" | "discover"> }) {
  const { t } = useI18n();
  const item = copy[page];
  const Icon = item.icon;
  return <div className="page"><header className="page-heading"><div><p className="page-eyebrow">{t("app.name")}</p><h1>{t(item.title)}</h1><p>{t(item.description)}</p></div></header><Card className="placeholder-card"><EmptyState icon={<Icon />} label={t("empty.previewLabel")} title={t(item.emptyTitle)} description={t(item.emptyDescription)} /></Card></div>;
}
