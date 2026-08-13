import { useCallback, useEffect, useMemo, useState, type KeyboardEvent } from "react";
import { ArrowRight, LibraryBig, Play, Search, ShieldCheck, Sparkles } from "lucide-react";
import { useI18n } from "../i18n/I18nProvider";
import { Button, EmptyState, ErrorState, SearchField, Skeleton } from "../components/ui";
import { RuntimePanel } from "../components/runtime/RuntimePanel";
import { profileCommands } from "../lib/profileCommands";
import type { Phase4Profile } from "../lib/generated/ipc-contracts";
import { useWorkspaceStore } from "../app/workspaceStore";
import { PlayerStage } from "../components/player/PlayerStage";
import { useShellStore } from "../app/shellStore";
import { BrandMark } from "../components/brand/BrandMark";

type ProfileLoadState = "loading" | "ready" | "error";

export function HomePage() {
  const { t } = useI18n();
  const [profiles, setProfiles] = useState<Phase4Profile[]>([]);
  const [loadState, setLoadState] = useState<ProfileLoadState>("loading");
  const [query, setQuery] = useState("");
  const selectedProfileId = useWorkspaceStore((state) => state.selectedProfileId);
  const selectProfile = useWorkspaceStore((state) => state.selectProfile);
  const reconcileProfiles = useWorkspaceStore((state) => state.reconcileProfiles);
  const setPage = useShellStore((state) => state.setPage);

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
      <section className="home-hero" aria-labelledby="home-title">
        <div className="home-hero__copy">
          <p className="page-eyebrow"><Sparkles aria-hidden="true" />{t("home.heroKicker")}</p>
          <h1 id="home-title">{t("home.title")}</h1>
          <p>{t("home.subtitle")}</p>
          <div className="home-hero__actions">
            <Button
              variant="primary"
              onClick={() => selectedProfile
                ? document.getElementById("runtime-control")?.scrollIntoView({
                    behavior: document.documentElement.dataset.reducedMotion === "true" ? "auto" : "smooth",
                    block: "start",
                  })
                : setPage("library")}
            >
              <Play aria-hidden="true" />
              {t(selectedProfile ? "home.heroAction" : "home.heroCreate")}
            </Button>
            <Button variant="ghost" onClick={() => setPage("library")}>
              <LibraryBig aria-hidden="true" />
              {t("home.heroLibrary")}
            </Button>
          </div>
          <div className="home-hero__trust"><ShieldCheck aria-hidden="true" /><span>{t("home.heroSecurity")}</span></div>
        </div>
        <div className="home-hero__art" aria-hidden="true">
          <span className="home-hero__orbit home-hero__orbit--one" />
          <span className="home-hero__orbit home-hero__orbit--two" />
          <BrandMark />
          <strong>{t("brand.name")}</strong>
          <small>{t("brand.tagline")}</small>
        </div>
        <div className="home-hero__profile">
          <span>{t("home.profilePanel")}</span>
          <strong>{selectedProfile?.displayName ?? t("home.heroNoProfile")}</strong>
          <small>{selectedProfile
            ? t("library.revision", { revision: selectedProfile.activeRevisionId.slice(-8) })
            : t("home.noProfilesDescription")}</small>
          <ArrowRight aria-hidden="true" />
        </div>
      </section>
      <div className="home-layout">
        <section className="home-panel home-profiles" aria-busy={loadState === "loading"}>
          <header><h2>{t("home.profilePanel")}</h2></header>
          <SearchField label={t("home.profileSearch")} placeholder={t("home.profileSearchPlaceholder")} value={query} disabled={loadState !== "ready" || profiles.length === 0} onChange={(event) => setQuery(event.currentTarget.value)} />
          {profileContent}
        </section>
        <PlayerStage />
        <RuntimePanel profile={selectedProfile} />
      </div>
    </div>
  );
}
