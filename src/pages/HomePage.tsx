import { useCallback, useEffect, useMemo, useState, type KeyboardEvent } from "react";
import { Search } from "lucide-react";
import { useI18n } from "../i18n/I18nProvider";
import { Badge, Button, EmptyState, ErrorState, SearchField, Skeleton } from "../components/ui";
import { RuntimePanel } from "../components/runtime/RuntimePanel";
import { profileCommands } from "../lib/profileCommands";
import type { Phase4Profile } from "../lib/generated/ipc-contracts";
import { useWorkspaceStore } from "../app/workspaceStore";

type ProfileLoadState = "loading" | "ready" | "error";

export function HomePage() {
  const { t } = useI18n();
  const [profiles, setProfiles] = useState<Phase4Profile[]>([]);
  const [loadState, setLoadState] = useState<ProfileLoadState>("loading");
  const [query, setQuery] = useState("");
  const selectedProfileId = useWorkspaceStore((state) => state.selectedProfileId);
  const selectProfile = useWorkspaceStore((state) => state.selectProfile);
  const reconcileProfiles = useWorkspaceStore((state) => state.reconcileProfiles);

  const loadProfiles = useCallback(async () => {
    setLoadState("loading");
    try {
      const items = await profileCommands.list();
      const active = items.filter((profile) => profile.lifecycleState === "active");
      setProfiles(active);
      reconcileProfiles(active.map((profile) => profile.id));
      setLoadState("ready");
    } catch {
      setLoadState("error");
    }
  }, [reconcileProfiles]);

  useEffect(() => {
    void loadProfiles();
  }, [loadProfiles]);

  const selectedProfile = profiles.find((profile) => profile.id === selectedProfileId) ?? null;
  const visibleProfiles = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    return profiles.filter((profile) => !normalized || profile.displayName.toLocaleLowerCase().includes(normalized));
  }, [profiles, query]);

  const tabbableProfileId = visibleProfiles.some((profile) => profile.id === selectedProfileId)
    ? selectedProfileId
    : visibleProfiles[0]?.id ?? null;

  const onProfileListKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    const options = [...event.currentTarget.querySelectorAll<HTMLButtonElement>("[role='option']")];
    if (options.length === 0) return;
    event.preventDefault();
    const focused = options.indexOf(document.activeElement as HTMLButtonElement);
    const selected = options.findIndex((option) => option.dataset.profileId === selectedProfileId);
    const current = focused >= 0 ? focused : Math.max(0, selected);
    const next = event.key === "Home"
      ? 0
      : event.key === "End"
        ? options.length - 1
        : (current + (event.key === "ArrowDown" ? 1 : -1) + options.length) % options.length;
    const option = options[next];
    selectProfile(option.dataset.profileId ?? null);
    option.focus();
  };

  const profileContent = loadState === "loading"
    ? (
        <div className="home-profile-loading" role="status" aria-label={t("app.loading")}>
          <Skeleton height="3.5rem" />
          <Skeleton height="3.5rem" />
          <Skeleton height="3.5rem" />
        </div>
      )
    : loadState === "error"
      ? <ErrorState title={t("app.unavailableTitle")} description={t("error.internal_error")} action={<Button onClick={() => void loadProfiles()}>{t("app.retry")}</Button>} />
      : profiles.length === 0
        ? <EmptyState icon={<Search />} label={t("empty.previewLabel")} title={t("home.noProfilesTitle")} description={t("library.emptyDescription")} />
        : visibleProfiles.length === 0
          ? <EmptyState icon={<Search />} label={t("empty.previewLabel")} title={t("library.emptyTitle")} description={t("library.emptyDescription")} />
          : (
              <div className="home-profile-list" role="listbox" aria-label={t("home.profilePanel")} onKeyDown={onProfileListKeyDown}>
                {visibleProfiles.map((profile) => (
                  <button
                    type="button"
                    key={profile.id}
                    role="option"
                    data-profile-id={profile.id}
                    aria-selected={profile.id === selectedProfileId}
                    tabIndex={profile.id === tabbableProfileId ? 0 : -1}
                    onFocus={() => selectProfile(profile.id)}
                    onClick={() => selectProfile(profile.id)}
                  >
                    <strong>{profile.displayName}</strong>
                    <span>{t("library.revision", { revision: profile.activeRevisionId.slice(-8) })}</span>
                  </button>
                ))}
              </div>
            );

  return (
    <div className="page page--home">
      <header className="page-heading page-heading--compact"><div><p className="page-eyebrow">{t("app.name")}</p><h1>{t("home.title")}</h1></div></header>
      <div className="home-layout">
        <section className="home-panel home-profiles" aria-busy={loadState === "loading"}>
          <header><h2>{t("home.profilePanel")}</h2></header>
          <SearchField label={t("home.profileSearch")} placeholder={t("home.profileSearchPlaceholder")} value={query} disabled={loadState !== "ready" || profiles.length === 0} onChange={(event) => setQuery(event.currentTarget.value)} />
          {profileContent}
        </section>
        <section className="home-panel player-stage" data-preview-surface="integrated">
          <header><h2>{t("home.stageTitle")}</h2><Badge tone="info">{t("home.stageBadge")}</Badge></header>
          <div className="player-stage__viewport" role="img" aria-label={t("home.stageUnavailable")}>
            <div className="player-stage__halo" aria-hidden="true" />
            <div className="player-placeholder" aria-hidden="true"><span className="player-placeholder__head" /><span className="player-placeholder__body" /><span className="player-placeholder__arm player-placeholder__arm--left" /><span className="player-placeholder__arm player-placeholder__arm--right" /><span className="player-placeholder__leg player-placeholder__leg--left" /><span className="player-placeholder__leg player-placeholder__leg--right" /></div>
            <div className="player-stage__platform" aria-hidden="true" />
            <span>{t("home.stageUnavailable")}</span>
          </div>
          <div className="player-stage__views"><Button disabled>{t("home.stageFront")}</Button><Button disabled>{t("home.stageBack")}</Button></div>
        </section>
        <RuntimePanel profile={selectedProfile} />
      </div>
    </div>
  );
}
