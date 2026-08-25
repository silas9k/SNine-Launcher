import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Box, ChevronRight, CircleCheck, FolderOpen, Gamepad2, HardDrive, Pencil, Plus, RefreshCw, Search, Settings2, ShieldCheck, Square, Trash2, UserRound, X } from "lucide-react";
import { Badge, Button, Card, Checkbox, ConfirmDialog, Dialog, EmptyState, SelectField, Status, Switch, TextField } from "../components/ui";
import { useI18n } from "../i18n/I18nProvider";
import { authCommands } from "../lib/authCommands";
import { instanceCommands, DEFAULT_INSTANCE_SETTINGS, type InstanceSettings } from "../lib/instanceCommands";
import { profileCommands } from "../lib/profileCommands";
import { runtimeCommands } from "../lib/runtimeCommands";
import { useShellStore } from "../app/shellStore";
import type { Phase3AuthSnapshot, Phase4Profile, Phase5LaunchStatus, Phase5RuntimeCatalog, Phase5RuntimeIntent, Phase5RuntimeStatus } from "../lib/generated/ipc-contracts";

type SupportedLoader = "vanilla" | "fabric";
type LoaderChoice = SupportedLoader | "forge" | "neoforge" | "optifine";
type InstanceDraft = { name: string; accountId: string; minecraftVersion: string; loader: LoaderChoice; loaderVersion: string; settings: InstanceSettings; jvmArgumentsText: string };
type InstallStage = "idle" | "profile" | "settings" | "runtime" | "finishing";

const EMPTY_CATALOG: Phase5RuntimeCatalog = {
  minecraftVersions: [], fabricVersions: [], selectedMinecraftJavaMajor: null,
  neoforgeCapability: { capabilityId: "runtime.neoforge", state: "unconfigured", reasonCode: "runtime_neoforge_pipeline_unavailable" },
  s9labComponentCapability: { capabilityId: "s9lab.components", state: "unconfigured", reasonCode: "component_provider_origin_unconfigured" },
};

function defaultDraft(): InstanceDraft { return { name: "", accountId: "", minecraftVersion: "", loader: "vanilla", loaderVersion: "latest", settings: { ...DEFAULT_INSTANCE_SETTINGS }, jvmArgumentsText: "" }; }
function isOldRelease(version: string) { const match = /^(\d+)\.(\d+)/.exec(version); return Boolean(match && (Number(match[1]) < 1 || (Number(match[1]) === 1 && Number(match[2]) < 13))); }
function launchTone(state: Phase5LaunchStatus["state"]): "success" | "warning" | "error" | "info" | "neutral" { if (state === "running") return "success"; if (["preparing", "checking-files", "downloading", "starting"].includes(state)) return "info"; if (state === "stopping") return "warning"; if (state === "failed") return "error"; return "neutral"; }
function errorCode(error: unknown) { return error && typeof error === "object" && "code" in error ? String((error as { code: unknown }).code) : String(error); }

export function InstancesPage() {
  const { t, formatDate } = useI18n();
  const [profiles, setProfiles] = useState<Phase4Profile[]>([]);
  const [statuses, setStatuses] = useState<Record<string, Phase5RuntimeStatus>>({});
  const [settings, setSettings] = useState<Record<string, InstanceSettings>>({});
  const [launches, setLaunches] = useState<Phase5LaunchStatus[]>([]);
  const [catalog, setCatalog] = useState<Phase5RuntimeCatalog>(EMPTY_CATALOG);
  const [loaderCatalog, setLoaderCatalog] = useState<Phase5RuntimeCatalog>(EMPTY_CATALOG);
  const [auth, setAuth] = useState<Phase3AuthSnapshot | null>(null);
  const [selectedProfileId, setSelectedProfileId] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [catalogLoading, setCatalogLoading] = useState(true);
  const [metadataLoading, setMetadataLoading] = useState(false);
  const [metadataError, setMetadataError] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [installStage, setInstallStage] = useState<InstallStage>("idle");
  const [installStartedAt, setInstallStartedAt] = useState<number | null>(null);
  const [installSeconds, setInstallSeconds] = useState(0);
  const installGuard = useRef(false);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<{ tone: "success" | "warning" | "error" | "info"; text: string } | null>(null);
  const [draft, setDraft] = useState<InstanceDraft>(defaultDraft);
  const [createOpen, setCreateOpen] = useState(false);
  const [editProfile, setEditProfile] = useState<Phase4Profile | null>(null);
  const [deleteProfile, setDeleteProfile] = useState<Phase4Profile | null>(null);
  const shellSettings = useShellStore((state) => state.settings);
  const saveShellSettings = useShellStore((state) => state.saveSettings);
  const showSnapshots = shellSettings.showMinecraftSnapshots;
  const showOld = shellSettings.showOldMinecraftVersions;
  const versions = useMemo(() => catalog.minecraftVersions.filter((entry) => !(entry.releaseType === "snapshot" && !showSnapshots) && !(entry.releaseType === "release" && isOldRelease(entry.version) && !showOld)), [catalog.minecraftVersions, showOld, showSnapshots]);

  const refresh = useCallback(async () => {
    try {
      const [nextProfiles, nextLaunches, nextAuth] = await Promise.all([profileCommands.list(), runtimeCommands.launchStatuses(), authCommands.snapshot().catch(() => null)]);
      const activeProfiles = nextProfiles.filter((profile) => profile.lifecycleState === "active");
      const details = await Promise.all(activeProfiles.map(async (profile) => {
        const [runtime, instance] = await Promise.all([runtimeCommands.status(profile.id).catch(() => null), instanceCommands.settings(profile.id).catch(() => ({ ...DEFAULT_INSTANCE_SETTINGS }))]);
        return { profile, runtime, instance };
      }));
      setProfiles(activeProfiles); setLaunches(nextLaunches); if (nextAuth) setAuth(nextAuth);
      setSelectedProfileId((current) => current && activeProfiles.some((profile) => profile.id === current) ? current : activeProfiles[0]?.id ?? null);
      setStatuses(Object.fromEntries(details.filter((item) => item.runtime).map((item) => [item.profile.id, item.runtime])) as Record<string, Phase5RuntimeStatus>);
      setSettings(Object.fromEntries(details.map((item) => [item.profile.id, item.instance])));
    } catch (error) { setMessage({ tone: "error", text: `${t("instances.error")} (${errorCode(error)})` }); }
    finally { setLoading(false); }
  }, [t]);

  useEffect(() => {
    let disposed = false;
    setCatalogLoading(true);
    void runtimeCommands.catalog().then((value) => { if (!disposed) setCatalog(value); }).catch(() => { if (!disposed) setMessage({ tone: "error", text: t("instances.catalogError") }); }).finally(() => { if (!disposed) setCatalogLoading(false); });
    void refresh();
    return () => { disposed = true; };
  }, [refresh, t]);
  useEffect(() => {
    if (!draft.minecraftVersion) { setLoaderCatalog(EMPTY_CATALOG); setMetadataLoading(false); setMetadataError(false); return; }
    let disposed = false;
    setMetadataLoading(true); setMetadataError(false);
    void runtimeCommands.catalog(draft.minecraftVersion).then((value) => { if (!disposed) setLoaderCatalog(value); }).catch(() => { if (!disposed) { setLoaderCatalog(EMPTY_CATALOG); setMetadataError(true); } }).finally(() => { if (!disposed) setMetadataLoading(false); });
    return () => { disposed = true; };
  }, [draft.minecraftVersion]);
  useEffect(() => {
    if ((!createOpen && !editProfile) || draft.minecraftVersion || versions.length === 0) return;
    const initialVersion = versions.find((entry) => entry.releaseType === "release")?.version ?? versions[0].version;
    setDraft((current) => ({ ...current, minecraftVersion: initialVersion, name: current.name || `Minecraft ${initialVersion}` }));
  }, [createOpen, draft.minecraftVersion, editProfile, versions]);
  useEffect(() => {
    if (!installStartedAt) return;
    const update = () => setInstallSeconds(Math.max(0, Math.floor((Date.now() - installStartedAt) / 1000)));
    update(); const timer = window.setInterval(update, 1_000); return () => window.clearInterval(timer);
  }, [installStartedAt]);
  useEffect(() => { if (!launches.some((item) => ["preparing", "checking-files", "downloading", "starting", "running", "stopping"].includes(item.state))) return; const timer = window.setInterval(() => void refresh(), 2_000); return () => window.clearInterval(timer); }, [launches, refresh]);

  const filteredProfiles = useMemo(() => profiles.filter((profile) => profile.displayName.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase())), [profiles, search]);
  const selectedProfile = profiles.find((profile) => profile.id === selectedProfileId) ?? null;
  const updateDraftSettings = <K extends keyof InstanceSettings>(key: K, value: InstanceSettings[K]) => setDraft((current) => ({ ...current, settings: { ...current.settings, [key]: value } }));
  const openCreate = () => { const initialVersion = versions.find((entry) => entry.releaseType === "release")?.version ?? versions[0]?.version ?? ""; setAdvancedOpen(false); setDraft({ ...defaultDraft(), accountId: auth?.activeAccountId ?? "", minecraftVersion: initialVersion, name: initialVersion ? `Minecraft ${initialVersion}` : "" }); setCreateOpen(true); };
  const openEdit = (profile: Phase4Profile) => { const runtime = statuses[profile.id]?.runtime; const currentSettings = settings[profile.id] ?? { ...DEFAULT_INSTANCE_SETTINGS }; setAdvancedOpen(true); setDraft({ name: profile.displayName, accountId: profile.accountId ?? auth?.activeAccountId ?? "", minecraftVersion: runtime?.minecraftVersion ?? "", loader: (runtime?.loader.kind ?? "vanilla") as LoaderChoice, loaderVersion: runtime?.loader.loaderVersion ?? "latest", settings: { ...currentSettings }, jvmArgumentsText: currentSettings.jvmArguments.join("\n") }); setEditProfile(profile); };
  const normalizedSettings = (): InstanceSettings => ({ ...draft.settings, jvmArguments: draft.jvmArgumentsText.split(/\r?\n/).map((value) => value.trim()).filter(Boolean), customJavaExecutable: draft.settings.customJavaExecutable?.trim() || null });
  const installIntent = (): Phase5RuntimeIntent => { const major = loaderCatalog.selectedMinecraftJavaMajor; if (!major) throw new Error("runtime_java_requirement_missing"); const loaderVersion = draft.loader === "fabric" ? (draft.loaderVersion === "latest" ? loaderCatalog.fabricVersions.find((entry) => entry.stable)?.version ?? loaderCatalog.fabricVersions[0]?.version : draft.loaderVersion) : undefined; return { minecraftVersion: draft.minecraftVersion, loader: { kind: draft.loader as SupportedLoader, ...(loaderVersion ? { loaderVersion } : {}) }, java: { mode: "managed", majorVersion: major } }; };
  const validateDraft = () => { if (!draft.name.trim() || !draft.minecraftVersion) return t("instances.validationRequired"); if (metadataLoading) return t("instances.metadataLoading"); if (metadataError || !loaderCatalog.selectedMinecraftJavaMajor) return t("instances.metadataUnavailable"); if (!(["vanilla", "fabric"] as LoaderChoice[]).includes(draft.loader)) return t("instances.loaderUnavailable"); const next = normalizedSettings(); if (next.minRamMb > next.maxRamMb) return t("instances.ramInvalid"); if (draft.loader === "fabric" && loaderCatalog.fabricVersions.length === 0) return t("instances.fabricUnavailable"); return ""; };

  const createInstance = async () => {
    if (installGuard.current) return;
    const invalid = validateDraft(); if (invalid) { setMessage({ tone: "warning", text: invalid }); return; }
    installGuard.current = true; setBusy("create"); setInstallStartedAt(Date.now()); setInstallSeconds(0); setInstallStage("profile"); let created: Phase4Profile | null = null;
    try {
      created = await profileCommands.create(draft.name.trim());
      setInstallStage("settings");
      await instanceCommands.saveSettings(created.id, normalizedSettings());
      if (draft.accountId) await authCommands.assignProfileAccount(created.id, draft.accountId);
      setInstallStage("runtime");
      await runtimeCommands.install(created.id, installIntent());
      setInstallStage("finishing"); await refresh();
      setSelectedProfileId(created.id); setCreateOpen(false); setMessage({ tone: "success", text: t("instances.created") });
    }
    catch (error) { if (created) await profileCommands.trash(created.id).catch(() => undefined); setMessage({ tone: "error", text: `${t("instances.createFailed")} (${errorCode(error)})` }); }
    finally { installGuard.current = false; setBusy(null); setInstallStage("idle"); setInstallStartedAt(null); }
  };
  const saveInstance = async () => {
    if (!editProfile) return; const invalid = validateDraft(); if (invalid) { setMessage({ tone: "warning", text: invalid }); return; } setBusy(editProfile.id);
    try { const current = statuses[editProfile.id]?.runtime; const nextIntent = installIntent(); const runtimeChanged = !current || current.minecraftVersion !== nextIntent.minecraftVersion || current.loader.kind !== nextIntent.loader.kind || current.loader.loaderVersion !== nextIntent.loader.loaderVersion; if (runtimeChanged) await runtimeCommands.install(editProfile.id, nextIntent); await Promise.all([instanceCommands.rename(editProfile.id, draft.name.trim()), instanceCommands.saveSettings(editProfile.id, normalizedSettings()), authCommands.assignProfileAccount(editProfile.id, draft.accountId || null)]); setEditProfile(null); setMessage({ tone: "success", text: t("instances.saved") }); await refresh(); }
    catch (error) { setMessage({ tone: "error", text: `${t("instances.saveFailed")} (${errorCode(error)})` }); }
    finally { setBusy(null); }
  };
  const launch = async (profile: Phase4Profile) => { setBusy(profile.id); try { await instanceCommands.launch(profile.id); setMessage({ tone: "success", text: t("instances.launchStarted") }); await refresh(); } catch (error) { setMessage({ tone: "error", text: `${t("instances.launchFailed")} (${errorCode(error)})` }); await refresh(); } finally { setBusy(null); } };
  const stop = async (launchStatus: Phase5LaunchStatus) => { setBusy(launchStatus.profileId); try { await runtimeCommands.stop(launchStatus.launchId); await refresh(); } catch (error) { setMessage({ tone: "error", text: `${t("instances.stopFailed")} (${errorCode(error)})` }); } finally { setBusy(null); } };
  const repair = async (profile: Phase4Profile) => { const runtime = statuses[profile.id]?.runtime; if (!runtime) { openEdit(profile); return; } setBusy(profile.id); try { await runtimeCommands.install(profile.id, runtime); setMessage({ tone: "success", text: t("instances.repaired") }); await refresh(); } catch (error) { setMessage({ tone: "error", text: `${t("instances.repairFailed")} (${errorCode(error)})` }); } finally { setBusy(null); } };
  const remove = async () => { if (!deleteProfile) return; setBusy(deleteProfile.id); try { await profileCommands.trash(deleteProfile.id); setDeleteProfile(null); setMessage({ tone: "success", text: t("instances.deleted") }); await refresh(); } catch (error) { setMessage({ tone: "error", text: `${t("instances.deleteFailed")} (${errorCode(error)})` }); } finally { setBusy(null); } };

  const draftForm = <div className="snine-profile-installer">
    {installStage !== "idle" ? <div className="snine-profile-installer__progress"><RefreshCw className="ui-spin" /><div><strong>{t(`instances.installStage.${installStage}`)}</strong><span>{t("instances.installElapsed", { seconds: installSeconds })}</span></div></div> : null}
    <section className="snine-profile-installer__section"><div className="snine-profile-installer__section-title"><span>01</span><div><strong>{t("instances.section.identity")}</strong><small>{t("instances.section.identityDescription")}</small></div></div><div className="snine-profile-installer__two"><TextField label={t("instances.name")} value={draft.name} maxLength={80} onChange={(event) => setDraft((current) => ({ ...current, name: event.target.value }))} /><SelectField label={t("instances.account")} value={draft.accountId} onChange={(event) => setDraft((current) => ({ ...current, accountId: event.target.value }))}><option value="">{t("instances.noAccount")}</option>{auth?.accounts.map((account) => <option value={account.id} key={account.id}>{account.username}</option>)}</SelectField></div>{auth?.accounts.length === 0 ? <p className="snine-profile-installer__hint"><UserRound />{t("instances.noAccountDescription")}</p> : null}</section>
    <section className="snine-profile-installer__section"><div className="snine-profile-installer__section-title"><span>02</span><div><strong>{t("instances.section.runtime")}</strong><small>{t("instances.section.runtimeDescription")}</small></div></div><SelectField label={t("instances.minecraftVersion")} value={draft.minecraftVersion} disabled={catalogLoading || versions.length === 0} onChange={(event) => setDraft((current) => ({ ...current, minecraftVersion: event.target.value, loaderVersion: "latest" }))}><option value="">{catalogLoading ? t("instances.catalogLoading") : t("instances.catalogEmpty")}</option>{versions.map((entry) => <option key={entry.version} value={entry.version}>{entry.version} · {entry.releaseType}</option>)}</SelectField><div className="snine-profile-installer__filters"><Checkbox label={t("instances.showSnapshots")} checked={showSnapshots} onChange={(event) => void saveShellSettings({ ...shellSettings, showMinecraftSnapshots: event.target.checked })} /><Checkbox label={t("instances.showOld")} checked={showOld} onChange={(event) => void saveShellSettings({ ...shellSettings, showOldMinecraftVersions: event.target.checked })} /></div>{metadataLoading ? <p className="snine-profile-installer__hint"><RefreshCw className="ui-spin" />{t("instances.metadataLoading")}</p> : null}{metadataError ? <Status tone="error" label={t("instances.metadataUnavailable")}>{t("instances.metadataUnavailable")}</Status> : null}</section>
    <section className="snine-profile-installer__section"><div className="snine-profile-installer__section-title"><span>03</span><div><strong>{t("instances.section.loader")}</strong><small>{t("instances.section.loaderDescription")}</small></div></div><div className="snine-loader-grid" role="radiogroup" aria-label={t("instances.loader")}><button type="button" role="radio" aria-checked={draft.loader === "vanilla"} className={draft.loader === "vanilla" ? "is-selected" : ""} onClick={() => setDraft((current) => ({ ...current, loader: "vanilla", loaderVersion: "latest" }))}><Box /><strong>{t("instances.loader.vanilla")}</strong><small>{t("instances.loader.vanillaDescription")}</small>{draft.loader === "vanilla" ? <CircleCheck /> : null}</button><button type="button" role="radio" aria-checked={draft.loader === "fabric"} className={draft.loader === "fabric" ? "is-selected" : ""} disabled={!metadataLoading && loaderCatalog.fabricVersions.length === 0} onClick={() => setDraft((current) => ({ ...current, loader: "fabric", loaderVersion: "latest" }))}><ShieldCheck /><strong>{t("instances.loader.fabric")}</strong><small>{t("instances.loader.fabricDescription")}</small>{draft.loader === "fabric" ? <CircleCheck /> : null}</button>{(["forge", "neoforge", "optifine"] as const).map((loader) => <button type="button" role="radio" aria-checked={false} disabled key={loader}><Box /><strong>{t(`instances.loader.${loader}`)}</strong><small>{loader === "optifine" ? t("instances.manualWorkflow") : t("instances.providerPending")}</small></button>)}</div>{draft.loader === "fabric" ? <SelectField label={t("instances.loaderVersion")} value={draft.loaderVersion} onChange={(event) => setDraft((current) => ({ ...current, loaderVersion: event.target.value }))}><option value="latest">{t("instances.latest")}</option>{loaderCatalog.fabricVersions.map((entry) => <option key={entry.version} value={entry.version}>{entry.version}{entry.stable ? ` · ${t("instances.stable")}` : ""}</option>)}</SelectField> : null}</section>
    <button className="snine-profile-installer__advanced-toggle" type="button" aria-expanded={advancedOpen} onClick={() => setAdvancedOpen((value) => !value)}><Settings2 /><span><strong>{t("instances.advanced")}</strong><small>{t("instances.advancedDescription")}</small></span><ChevronRight className={advancedOpen ? "is-open" : ""} /></button>
    {advancedOpen ? <section className="snine-profile-installer__section is-advanced"><div className="instance-editor__grid"><TextField label={t("instances.minRam")} type="number" min={512} max={16384} step={512} value={draft.settings.minRamMb} onChange={(event) => updateDraftSettings("minRamMb", Number(event.target.value))} /><TextField label={t("instances.maxRam")} type="number" min={2048} max={16384} step={512} value={draft.settings.maxRamMb} onChange={(event) => updateDraftSettings("maxRamMb", Number(event.target.value))} /><TextField label={t("instances.width")} type="number" min={320} max={7680} value={draft.settings.width} onChange={(event) => updateDraftSettings("width", Number(event.target.value))} /><TextField label={t("instances.height")} type="number" min={240} max={4320} value={draft.settings.height} onChange={(event) => updateDraftSettings("height", Number(event.target.value))} /></div><Switch label={t("instances.fullscreen")} checked={draft.settings.fullscreen} onChange={(event) => updateDraftSettings("fullscreen", event.target.checked)} /><TextField label={t("instances.customJava")} description={t("instances.customJavaDescription")} value={draft.settings.customJavaExecutable ?? ""} onChange={(event) => updateDraftSettings("customJavaExecutable", event.target.value || null)} /><label className="ui-field"><span className="ui-field__label">{t("instances.jvmArgs")}</span><span className="ui-field__description">{t("instances.jvmArgsDescription")}</span><textarea rows={4} value={draft.jvmArgumentsText} onChange={(event) => setDraft((current) => ({ ...current, jvmArgumentsText: event.target.value }))} /></label></section> : null}
    <aside className="snine-profile-installer__summary"><ShieldCheck /><div><strong>{t("instances.atomicInstall")}</strong><span>{t("instances.installSummary", { version: draft.minecraftVersion || "—", loader: t(`instances.loader.${draft.loader}`), java: loaderCatalog.selectedMinecraftJavaMajor ?? "—" })}</span></div></aside>
  </div>;

  const selectedRuntime = selectedProfile ? statuses[selectedProfile.id] : null;
  const selectedSettings = selectedProfile ? settings[selectedProfile.id] : null;
  const selectedLaunch = selectedProfile ? launches.find((item) => item.profileId === selectedProfile.id && !["exited", "failed"].includes(item.state)) : null;
  const installedCount = profiles.filter((profile) => statuses[profile.id]?.installState === "installed").length;
  const runningCount = launches.filter((item) => item.state === "running").length;

  return <div className="page snine-profiles-page">
    <header className="page-heading snine-profiles-heading"><div><p className="page-eyebrow">{t("instances.eyebrow")}</p><h1>{t("page.instances.title")}</h1><p>{t("instances.managerDescription")}</p></div><div className="instances-heading-actions"><Button onClick={() => { setLoading(true); void refresh(); }} loading={loading}><RefreshCw aria-hidden="true" />{t("app.retry")}</Button><Button variant="primary" onClick={openCreate}><Plus aria-hidden="true" />{t("instances.installProfile")}</Button></div></header>
    {message ? <Status tone={message.tone} label={message.text}>{message.text}</Status> : null}
    <section className="snine-profile-stats"><Card><span>{t("instances.totalProfiles")}</span><strong>{profiles.length}</strong></Card><Card><span>{t("instances.installedProfiles")}</span><strong>{installedCount}</strong></Card><Card><span>{t("instances.runningProfiles")}</span><strong>{runningCount}</strong></Card><Card><span>{t("instances.availableVersions")}</span><strong>{catalog.minecraftVersions.length}</strong></Card></section>
    {loading && profiles.length === 0 ? <Card className="instances-empty"><EmptyState icon={<RefreshCw className="ui-spin" />} label={t("app.loading")} title={t("app.loading")} description={t("instances.loading")} /></Card> : null}
    {!loading && profiles.length === 0 ? <Card className="snine-profiles-empty"><EmptyState icon={<Box />} label={t("instances.startHere")} title={t("instances.emptyTitle")} description={t("instances.installProfileHint")} action={<Button variant="primary" onClick={openCreate}><Plus />{t("instances.installProfile")}</Button>} /></Card> : null}
    {profiles.length > 0 ? <section className="snine-profiles-workspace"><Card className="snine-profiles-rail"><header><div><strong>{t("instances.yourProfiles")}</strong><span>{profiles.length}</span></div><button type="button" onClick={openCreate} aria-label={t("instances.installProfile")}><Plus /></button></header><label className="snine-profiles-search"><Search /><input value={search} onChange={(event) => setSearch(event.target.value)} placeholder={t("instances.searchProfiles")} />{search ? <button type="button" onClick={() => setSearch("")} aria-label={t("app.close")}><X /></button> : null}</label><div className="snine-profiles-list">{filteredProfiles.map((profile) => { const runtime = statuses[profile.id]; const current = launches.find((item) => item.profileId === profile.id && !["exited", "failed"].includes(item.state)); return <button type="button" key={profile.id} className={selectedProfileId === profile.id ? "is-selected" : ""} onClick={() => setSelectedProfileId(profile.id)}><span className="snine-profile-mark"><Gamepad2 /></span><span><strong>{profile.displayName}</strong><small>{runtime?.runtime ? `${runtime.runtime.minecraftVersion} · ${runtime.runtime.loader.kind}` : t("instances.notInstalled")}</small></span><i className={current?.state === "running" ? "is-running" : runtime?.installState === "installed" ? "is-ready" : "is-warning"} /><ChevronRight /></button>; })}{filteredProfiles.length === 0 ? <div className="snine-profiles-no-results"><Search /><strong>{t("instances.noSearchResults")}</strong><span>{t("instances.tryAnotherSearch")}</span></div> : null}</div></Card>
      {selectedProfile ? <Card className="snine-profile-detail"><header className="snine-profile-detail__hero"><div className="snine-profile-detail__icon"><Gamepad2 /></div><div><div className="snine-profile-detail__title"><h2>{selectedProfile.displayName}</h2><Badge tone={selectedLaunch ? launchTone(selectedLaunch.state) : selectedRuntime?.installState === "installed" ? "success" : "warning"}>{selectedLaunch ? t(`instances.state.${selectedLaunch.state}`) : selectedRuntime?.installState === "installed" ? t("instances.ready") : t("instances.notInstalled")}</Badge></div><p>{selectedRuntime?.runtime ? `${selectedRuntime.runtime.minecraftVersion} · ${selectedRuntime.runtime.loader.kind}${selectedRuntime.runtime.loader.loaderVersion ? ` ${selectedRuntime.runtime.loader.loaderVersion}` : ""}` : t("instances.notConfigured")}</p></div></header>
        <div className="snine-profile-detail__actions">{selectedLaunch ? <Button variant="danger" loading={busy === selectedProfile.id} onClick={() => void stop(selectedLaunch)}><Square />{t("instances.stop")}</Button> : selectedRuntime?.installState === "installed" ? <Button variant="primary" loading={busy === selectedProfile.id} disabled={!selectedProfile.accountId} onClick={() => void launch(selectedProfile)}><Gamepad2 />{t("instances.play")}</Button> : <Button variant="primary" loading={busy === selectedProfile.id} onClick={() => void repair(selectedProfile)}><HardDrive />{t("instances.install")}</Button>}<Button onClick={() => openEdit(selectedProfile)}><Pencil />{t("instances.edit")}</Button></div>
        {!selectedProfile.accountId ? <div className="snine-profile-account-warning"><UserRound /><div><strong>{t("instances.accountRequired")}</strong><span>{t("instances.accountRequiredDescription")}</span></div><Button onClick={() => openEdit(selectedProfile)}>{t("instances.connectAccount")}</Button></div> : null}
        <dl className="snine-profile-facts"><div><dt>{t("instances.minecraftVersion")}</dt><dd>{selectedRuntime?.runtime?.minecraftVersion ?? "—"}</dd></div><div><dt>{t("instances.loader")}</dt><dd>{selectedRuntime?.runtime ? t(`instances.loader.${selectedRuntime.runtime.loader.kind}`) : "—"}</dd></div><div><dt>{t("instances.memory")}</dt><dd>{selectedSettings ? `${selectedSettings.minRamMb / 1024}–${selectedSettings.maxRamMb / 1024} GB` : "—"}</dd></div><div><dt>{t("instances.resolution")}</dt><dd>{selectedSettings ? (selectedSettings.fullscreen ? t("instances.fullscreen") : `${selectedSettings.width} × ${selectedSettings.height}`) : "—"}</dd></div><div><dt>{t("instances.account")}</dt><dd>{auth?.accounts.find((account) => account.id === selectedProfile.accountId)?.username ?? t("instances.noAccount")}</dd></div><div><dt>{t("instances.lastPlayed")}</dt><dd>{selectedSettings?.lastPlayedAtUnix ? formatDate(selectedSettings.lastPlayedAtUnix * 1000) : "—"}</dd></div></dl>
        <section className="snine-profile-folders"><div><strong>{t("instances.directories")}</strong><span>{t("instances.directoriesDescription")}</span></div><div><Button variant="ghost" onClick={() => void instanceCommands.openFolder(selectedProfile.id, "game")}><HardDrive />{t("instances.gameFolder")}</Button><Button variant="ghost" onClick={() => void instanceCommands.openFolder(selectedProfile.id, "mods")}><FolderOpen />{t("instances.folder.mods")}</Button><Button variant="ghost" onClick={() => void instanceCommands.openFolder(selectedProfile.id, "resourcepacks")}><FolderOpen />{t("instances.folder.resourcepacks")}</Button><Button variant="ghost" onClick={() => void instanceCommands.openFolder(selectedProfile.id, "screenshots")}><FolderOpen />{t("instances.folder.screenshots")}</Button></div></section>
        <footer className="snine-profile-detail__footer"><div><ShieldCheck /><span><strong>{t("instances.isolatedData")}</strong><small>{t("instances.isolatedDataDescription")}</small></span></div><Button variant="ghost" onClick={() => setDeleteProfile(selectedProfile)}><Trash2 />{t("instances.delete")}</Button></footer>
      </Card> : null}</section> : null}
    <Dialog open={createOpen} title={t("instances.createTitle")} description={t("instances.createDescription")} onClose={() => { if (!installGuard.current) setCreateOpen(false); }} footer={<><Button disabled={busy === "create"} onClick={() => setCreateOpen(false)}>{t("app.cancel")}</Button><Button variant="primary" disabled={catalogLoading || metadataLoading || metadataError || !draft.minecraftVersion} loading={busy === "create"} onClick={() => void createInstance()}><Plus />{t("instances.install")}</Button></>}>{draftForm}</Dialog>
    <Dialog open={Boolean(editProfile)} title={t("instances.editTitle")} description={t("instances.editWarning")} onClose={() => setEditProfile(null)} footer={<><Button onClick={() => setEditProfile(null)}>{t("app.cancel")}</Button><Button variant="primary" loading={busy === editProfile?.id} disabled={metadataLoading || metadataError} onClick={() => void saveInstance()}>{t("app.save")}</Button></>}>{draftForm}</Dialog>
    <ConfirmDialog open={Boolean(deleteProfile)} title={t("instances.deleteTitle")} description={t("instances.deleteDescription")} confirmLabel={t("instances.delete")} cancelLabel={t("app.cancel")} loading={busy === deleteProfile?.id} onClose={() => setDeleteProfile(null)} onConfirm={() => void remove()} />
  </div>;
}
