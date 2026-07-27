import { useEffect, useMemo, useState } from "react";
import { CircleOff, Cuboid, Search, ShieldCheck } from "lucide-react";
import { useI18n } from "../i18n/I18nProvider";
import { Badge, Button, Card, EmptyState, SearchField } from "../components/ui";
import { profileCommands } from "../lib/profileCommands";
import type { Phase4Profile } from "../lib/generated/ipc-contracts";

export function HomePage() {
  const { t } = useI18n();
  const [profiles, setProfiles] = useState<Phase4Profile[]>([]);
  const [selectedProfileId, setSelectedProfileId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  useEffect(() => {
    void profileCommands.list().then((items) => {
      const active = items.filter((profile) => profile.lifecycleState === "active");
      setProfiles(active);
      setSelectedProfileId((current) => current ?? active[0]?.id ?? null);
    }).catch(() => undefined);
  }, []);
  const selectedProfile = profiles.find((profile) => profile.id === selectedProfileId) ?? null;
  const visibleProfiles = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    return profiles.filter((profile) => !normalized || profile.displayName.toLocaleLowerCase().includes(normalized));
  }, [profiles, query]);
  return (
    <div className="page page--home">
      <header className="page-heading"><div><p className="page-eyebrow">{t("app.name")}</p><h1>{t("home.title")}</h1><p>{t("home.subtitle")}</p></div></header>
      <div className="home-layout">
        <Card className="home-panel home-profiles">
          <header><h2>{t("home.profilePanel")}</h2></header>
          <SearchField label={t("home.profileSearch")} placeholder={t("home.profileSearchPlaceholder")} value={query} disabled={profiles.length === 0} onChange={(event) => setQuery(event.currentTarget.value)} />
          {profiles.length === 0 ? <EmptyState icon={<Search />} label={t("empty.previewLabel")} title={t("home.noProfilesTitle")} description={t("home.noProfilesDescription")} /> : <div className="home-profile-list" role="listbox" aria-label={t("home.profilePanel")}>{visibleProfiles.map((profile) => <button key={profile.id} role="option" aria-selected={profile.id === selectedProfileId} onClick={() => setSelectedProfileId(profile.id)}><strong>{profile.displayName}</strong><span>{t("library.revision", { revision: profile.activeRevisionId.slice(-8) })}</span></button>)}</div>}
        </Card>
        <section className="home-panel player-stage" data-preview-surface="integrated">
          <header><h2>{t("home.stageTitle")}</h2><Badge tone="info">{t("home.stageBadge")}</Badge></header>
          <div className="player-stage__viewport" aria-label={t("home.stageUnavailable")}>
            <div className="player-stage__halo" aria-hidden="true" />
            <div className="player-placeholder" aria-hidden="true"><span className="player-placeholder__head" /><span className="player-placeholder__body" /><span className="player-placeholder__arm player-placeholder__arm--left" /><span className="player-placeholder__arm player-placeholder__arm--right" /><span className="player-placeholder__leg player-placeholder__leg--left" /><span className="player-placeholder__leg player-placeholder__leg--right" /></div>
            <div className="player-stage__platform" aria-hidden="true" />
            <span>{t("home.stageUnavailable")}</span>
          </div>
          <p>{t("home.stageDescription")}</p>
          <div className="player-stage__views"><Button disabled>{t("home.stageFront")}</Button><Button disabled>{t("home.stageBack")}</Button></div>
        </section>
        <Card className="home-panel home-status">
          <header><h2>{t("home.statusTitle")}</h2><ShieldCheck aria-hidden="true" /></header>
          {selectedProfile ? <div className="home-status__selection"><ShieldCheck aria-hidden="true" /><strong>{selectedProfile.displayName}</strong><p>{t("home.selectedProfileReady")}</p></div> : <div className="home-status__empty"><CircleOff aria-hidden="true" /><strong>{t("home.noSelection")}</strong><p>{t("home.noSelectionDescription")}</p></div>}
          <dl>
            <div><dt>{t("home.statusMinecraft")}</dt><dd>{t("home.notAvailable")}</dd></div>
            <div><dt>{t("home.statusLoader")}</dt><dd>{t("home.notAvailable")}</dd></div>
            <div><dt>{t("home.statusMods")}</dt><dd>{t("home.notAvailable")}</dd></div>
            <div><dt>{t("home.statusUpdates")}</dt><dd>{t("home.notAvailable")}</dd></div>
          </dl>
          <Button variant="primary" disabled aria-describedby="launch-unavailable"><Cuboid aria-hidden="true" />{t("home.launch")}</Button>
          <small id="launch-unavailable">{t("home.launchUnavailable")}</small>
        </Card>
      </div>
    </div>
  );
}
