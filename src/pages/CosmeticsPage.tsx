import { CloudOff, Feather, Orbit, ShieldCheck, Sparkles } from "lucide-react";
import { Badge, Card, Status } from "../components/ui";
import { useI18n } from "../i18n/I18nProvider";
import type { TranslationKey } from "../i18n/messages";

export function CosmeticsPage() {
  const { t } = useI18n();
  const items = [
    { key: "cape", icon: ShieldCheck, name: "player.cape", description: "cosmetics.capeDescription" },
    { key: "wings", icon: Feather, name: "player.wings", description: "cosmetics.wingsDescription" },
    { key: "halo", icon: Orbit, name: "player.halo", description: "cosmetics.haloDescription" },
  ] as const;
  return <div className="page cosmetics-page">
    <header className="page-heading"><div><p className="page-eyebrow">{t("cosmetics.eyebrow")}</p><h1>{t("page.cosmetics.title")}</h1><p>{t("cosmetics.description")}</p></div><Badge tone="info">{t("cosmetics.localPreview")}</Badge></header>
    <Status tone="warning" label={t("cosmetics.ownershipUnavailable")}><CloudOff aria-hidden="true" />{t("cosmetics.ownershipDescription")}</Status>
    <section className="cosmetics-grid" aria-label={t("cosmetics.collection") }>
      {items.map(({ key, icon: Icon, name, description }) => <Card className="cosmetic-item" key={key}><span className="cosmetic-item__visual"><Icon aria-hidden="true" /></span><div><h2>{t(name as TranslationKey)}</h2><p>{t(description as TranslationKey)}</p></div><Badge tone="accent"><Sparkles aria-hidden="true" />{t("cosmetics.previewAsset")}</Badge></Card>)}
    </section>
    <p className="cosmetics-note">{t("cosmetics.stageHint")}</p>
  </div>;
}
