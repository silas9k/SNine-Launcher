import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ChangeEvent,
  type FormEvent,
} from "react";
import {
  ArchiveRestore,
  Blocks,
  Box,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  CircleOff,
  Download,
  FileArchive,
  FilePlus2,
  Filter,
  PackageCheck,
  RefreshCw,
  Search,
  ShieldAlert,
  ShieldCheck,
  Trash2,
  Upload,
} from "lucide-react";
import type { Phase4Profile } from "../../lib/generated/ipc-contracts";
import {
  contentCommands,
  type LocalContentFile,
  type Phase6Capability,
  type Phase6ContentSnapshot,
  type Phase6ContentType,
  type Phase6InstalledContent,
  type Phase6ProjectDetail,
  type Phase6ProjectVersion,
  type Phase6SearchInput,
  type Phase6SearchResult,
} from "../../lib/contentCommands";
import { typedIpcError } from "../../lib/shellCommands";
import { useI18n } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import {
  Badge,
  Button,
  Card,
  ConfirmDialog,
  EmptyState,
  SearchField,
  SelectField,
  Skeleton,
  Status,
  Switch,
  Tabs,
} from "../ui";

type LoadState = "idle" | "loading" | "ready" | "error";
type Workspace = "installed" | "discover";

interface ContentEditorProps {
  profiles: Phase4Profile[];
  onProfilesChanged?: () => Promise<void>;
}

const PAGE_SIZE = 20;

const ERROR_KEYS: Partial<Record<string, TranslationKey>> = {
  content_desktop_runtime_required: "error.content_desktop_runtime_required",
  content_local_path_unavailable: "error.content_local_path_unavailable",
  content_profile_runtime_required: "error.content_profile_runtime_required",
  content_profile_running: "error.content_profile_running",
  content_conflict: "error.content_conflict",
  content_dependency_unsatisfied: "error.content_dependency_unsatisfied",
  content_incompatible: "error.content_incompatible",
  content_not_found: "error.content_not_found",
  content_provider_unavailable: "error.content_provider_unavailable",
  content_import_invalid: "error.content_import_invalid",
  content_operation_cancelled: "error.content_operation_cancelled",
  content_update_unavailable: "error.content_update_unavailable",
  content_pack_member_update_unavailable: "error.content_pack_member_update_unavailable",
  network_error: "error.network_error",
};

function kindKey(kind: Phase6ContentType): TranslationKey {
  return `content.kind.${kind}` as TranslationKey;
}

function loaderKey(loader: Phase6ContentSnapshot["loader"]): TranslationKey {
  return loader ? `runtime.loader.${loader}` as TranslationKey : "content.notConfigured";
}

function capabilityTone(capability: Phase6Capability | null): "success" | "warning" {
  return capability?.state === "available" ? "success" : "warning";
}

function currentVersion(detail: Phase6ProjectDetail | null, versionId: string): Phase6ProjectVersion | null {
  return detail?.versions.find((version) => version.versionId === versionId) ?? null;
}

export function ContentEditor({ profiles, onProfilesChanged }: ContentEditorProps) {
  const { t, formatDate, formatNumber } = useI18n();
  const activeProfiles = useMemo(
    () => profiles.filter((profile) => profile.lifecycleState === "active"),
    [profiles],
  );
  const [profileId, setProfileId] = useState("");
  const [workspace, setWorkspace] = useState<Workspace>("installed");
  const [kind, setKind] = useState<Phase6ContentType>("mod");
  const [snapshotState, setSnapshotState] = useState<LoadState>("idle");
  const [snapshot, setSnapshot] = useState<Phase6ContentSnapshot | null>(null);
  const [selectedContentId, setSelectedContentId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [searchState, setSearchState] = useState<LoadState>("idle");
  const [searchResult, setSearchResult] = useState<Phase6SearchResult | null>(null);
  const [detailState, setDetailState] = useState<LoadState>("idle");
  const [detail, setDetail] = useState<Phase6ProjectDetail | null>(null);
  const [versionId, setVersionId] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [liveMessage, setLiveMessage] = useState("");
  const [removeTarget, setRemoveTarget] = useState<Phase6InstalledContent | null>(null);
  const snapshotRequest = useRef(0);
  const searchRequest = useRef(0);
  const detailRequest = useRef(0);
  const localInput = useRef<HTMLInputElement>(null);
  const packInput = useRef<HTMLInputElement>(null);
  const profileImportInput = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (activeProfiles.some((profile) => profile.id === profileId)) return;
    setProfileId(activeProfiles[0]?.id ?? "");
  }, [activeProfiles, profileId]);

  const localizeError = useCallback((error: unknown) => {
    const typed = typedIpcError(error);
    const key = typed ? ERROR_KEYS[typed.code] : undefined;
    return t(key ?? "content.error.generic", typed?.params);
  }, [t]);

  const refreshSnapshot = useCallback(async (nextProfileId: string, showLoading = true) => {
    const request = ++snapshotRequest.current;
    if (!nextProfileId) {
      setSnapshot(null);
      setSnapshotState("idle");
      return null;
    }
    if (showLoading) setSnapshotState("loading");
    try {
      const next = await contentCommands.snapshot(nextProfileId);
      if (request !== snapshotRequest.current) return null;
      setSnapshot(next);
      setSnapshotState("ready");
      setSelectedContentId((current) =>
        next.content.some((item) => item.contentId === current)
          ? current
          : next.content[0]?.contentId ?? null,
      );
      if (next.minecraftVersion && next.loader && next.localFileCapability.state === "available") {
        void contentCommands.checkUpdates(nextProfileId).then((updated) => {
          if (
            request === snapshotRequest.current
            && updated.lockSha256 === next.lockSha256
          ) {
            setSnapshot(updated);
          }
        }).catch(() => {
          // The installed list remains fully usable offline. Provider update
          // metadata is deliberately best-effort and never hides local state.
        });
      }
      return next;
    } catch (error) {
      if (request !== snapshotRequest.current) return null;
      setSnapshot(null);
      setSnapshotState("error");
      setErrorMessage(localizeError(error));
      return null;
    }
  }, [localizeError]);

  useEffect(() => {
    setErrorMessage(null);
    setLiveMessage("");
    setSearchResult(null);
    setDetail(null);
    setSearchState("idle");
    setDetailState("idle");
    void refreshSnapshot(profileId);
  }, [profileId, refreshSnapshot]);

  useEffect(() => {
    setSearchResult(null);
    setDetail(null);
    setVersionId("");
    setSearchState("idle");
    setDetailState("idle");
  }, [kind]);

  const installed = useMemo(
    () => snapshot?.content.filter((item) => item.contentType === kind) ?? [],
    [kind, snapshot],
  );
  useEffect(() => {
    setSelectedContentId((current) =>
      installed.some((item) => item.contentId === current)
        ? current
        : installed[0]?.contentId ?? null,
    );
  }, [installed]);
  const selectedContent = useMemo(
    () => snapshot?.content.find((item) => item.contentId === selectedContentId) ?? null,
    [selectedContentId, snapshot],
  );
  const selectedVersion = currentVersion(detail, versionId);
  const hasBlockingConflict = Boolean(selectedVersion?.conflicts.length);
  const hasMissingDependency = Boolean(
    selectedVersion?.dependencies.some((dependency) =>
      dependency.relation === "required" && !dependency.satisfied
    ),
  );

  const runMutation = async (
    key: string,
    operation: () => Promise<unknown>,
    successKey: TranslationKey,
  ) => {
    setBusy(key);
    setErrorMessage(null);
    setLiveMessage("");
    try {
      await operation();
      await refreshSnapshot(profileId, false);
      setLiveMessage(t(successKey));
    } catch (error) {
      setErrorMessage(localizeError(error));
    } finally {
      setBusy(null);
    }
  };

  const searchAt = async (offset: number) => {
    if (!snapshot?.minecraftVersion || !snapshot.loader || query.trim().length < 2) return;
    const input: Phase6SearchInput = {
      query: query.trim(),
      contentType: kind,
      minecraftVersion: snapshot.minecraftVersion,
      loader: snapshot.loader,
      offset,
      limit: PAGE_SIZE,
    };
    const request = ++searchRequest.current;
    setSearchState("loading");
    setErrorMessage(null);
    setDetail(null);
    setDetailState("idle");
    try {
      const result = await contentCommands.search(input);
      if (request !== searchRequest.current) return;
      setSearchResult(result);
      setSearchState("ready");
    } catch (error) {
      if (request !== searchRequest.current) return;
      setSearchResult(null);
      setSearchState("error");
      setErrorMessage(localizeError(error));
    }
  };

  const submitSearch = (event: FormEvent) => {
    event.preventDefault();
    void searchAt(0);
  };

  const loadDetail = async (projectId: string) => {
    const request = ++detailRequest.current;
    setDetailState("loading");
    setErrorMessage(null);
    try {
      const next = await contentCommands.project(projectId);
      if (request !== detailRequest.current) return;
      setDetail(next);
      setVersionId(next.versions.find((version) => version.compatible)?.versionId ?? "");
      setDetailState("ready");
    } catch (error) {
      if (request !== detailRequest.current) return;
      setDetail(null);
      setDetailState("error");
      setErrorMessage(localizeError(error));
    }
  };

  const installSelected = async () => {
    if (!detail || !versionId) return;
    await runMutation(
      `install:${detail.projectId}`,
      () => contentCommands.install(profileId, detail.projectId, versionId),
      "content.operation.installed",
    );
  };

  const confirmRemove = async () => {
    if (!removeTarget) return;
    const target = removeTarget;
    await runMutation(
      `remove:${target.contentId}`,
      () => contentCommands.remove(profileId, target.contentId),
      "content.operation.removed",
    );
    setRemoveTarget(null);
  };

  const useSelectedFile = async (
    event: ChangeEvent<HTMLInputElement>,
    action: (file: LocalContentFile) => Promise<void>,
  ) => {
    const file = event.currentTarget.files?.[0] as LocalContentFile | undefined;
    event.currentTarget.value = "";
    if (file) await action(file);
  };

  const importProfile = async (file: LocalContentFile) => {
    setBusy("profile-import");
    setErrorMessage(null);
    setLiveMessage("");
    try {
      const result = await contentCommands.importProfile(file);
      await onProfilesChanged?.();
      setProfileId(result.profileId);
      setLiveMessage(t("content.operation.profileImported"));
    } catch (error) {
      setErrorMessage(localizeError(error));
    } finally {
      setBusy(null);
    }
  };

  const exportProfile = async () => {
    setBusy("profile-export");
    setErrorMessage(null);
    setLiveMessage("");
    try {
      await contentCommands.exportProfile(profileId);
      setLiveMessage(t("content.operation.profileExported"));
    } catch (error) {
      setErrorMessage(localizeError(error));
    } finally {
      setBusy(null);
    }
  };

  if (activeProfiles.length === 0) {
    return (
      <Card className="content-editor content-editor--empty">
        <EmptyState
          icon={<Blocks />}
          label={t("content.empty.noProfilesLabel")}
          title={t("content.empty.noProfilesTitle")}
          description={t("content.empty.noProfilesDescription")}
          action={<>
            <input
              ref={profileImportInput}
              hidden
              type="file"
              accept=".s9profile,.zip"
              onChange={(event) => void useSelectedFile(event, importProfile)}
            />
            <Button
              loading={busy === "profile-import"}
              onClick={() => profileImportInput.current?.click()}
            ><Upload aria-hidden="true" />{t("content.importProfile")}</Button>
          </>}
        />
        {errorMessage ? <Status tone="error" label={t("status.error")}>{errorMessage}</Status> : null}
      </Card>
    );
  }

  return (
    <section className="content-editor" aria-labelledby="content-editor-title">
      <Card className="content-editor__header">
        <div className="content-editor__title">
          <span className="content-editor__icon"><Blocks aria-hidden="true" /></span>
          <div>
            <p className="page-eyebrow">{t("content.eyebrow")}</p>
            <h2 id="content-editor-title">{t("content.title")}</h2>
            <p>{t("content.description")}</p>
          </div>
        </div>
        <div className="content-editor__profile-actions">
          <SelectField
            label={t("content.profile")}
            value={profileId}
            onChange={(event) => setProfileId(event.currentTarget.value)}
          >
            {activeProfiles.map((profile) => (
              <option key={profile.id} value={profile.id}>{profile.displayName}</option>
            ))}
          </SelectField>
          <input
            ref={profileImportInput}
            hidden
            type="file"
            accept=".s9profile,.zip"
            onChange={(event) => void useSelectedFile(event, importProfile)}
          />
          <Button
            loading={busy === "profile-import"}
            onClick={() => profileImportInput.current?.click()}
          ><Upload aria-hidden="true" />{t("content.importProfile")}</Button>
          <Button
            loading={busy === "profile-export"}
            disabled={!profileId || snapshot?.profileFormatCapability.state !== "available"}
            onClick={() => void exportProfile()}
          ><Download aria-hidden="true" />{t("content.exportProfile")}</Button>
        </div>
        <div className="content-editor__context" aria-label={t("content.context") }>
          <span><Box aria-hidden="true" /><small>{t("content.minecraft")}</small><strong>{snapshot?.minecraftVersion ?? t("content.notConfigured")}</strong></span>
          <span><Filter aria-hidden="true" /><small>{t("content.loader")}</small><strong>{t(loaderKey(snapshot?.loader ?? null))}</strong></span>
          <span><ShieldCheck aria-hidden="true" /><small>{t("content.lock")}</small><strong className="content-hash">{snapshot?.lockSha256?.slice(0, 12) ?? t("content.noLock")}</strong></span>
          <Badge tone={snapshot?.content.some((item) => item.conflicts.length) ? "warning" : "success"}>
            {snapshot?.content.some((item) => item.conflicts.length) ? t("content.conflictsFound") : t("content.noConflicts")}
          </Badge>
        </div>
      </Card>

      {errorMessage ? <Status tone="error" label={t("status.error")}>{errorMessage}</Status> : null}
      {liveMessage ? <Status tone="success" label={t("status.success")}>{liveMessage}</Status> : null}

      <div className="content-editor__toolbar">
        <Tabs
          label={t("content.workspace")}
          value={workspace}
          onChange={(value) => setWorkspace(value as Workspace)}
          items={[
            { value: "installed", label: t("content.workspace.installed"), panelId: "content-installed-panel" },
            { value: "discover", label: t("content.workspace.discover"), panelId: "content-discover-panel" },
          ]}
        />
        <Tabs
          label={t("content.kindFilter")}
          value={kind}
          onChange={(value) => setKind(value as Phase6ContentType)}
          items={([
            "mod",
            "modpack",
            "shaderPack",
            "resourcePack",
          ] as Phase6ContentType[]).map((value) => ({ value, label: t(kindKey(value)) }))}
        />
      </div>

      {snapshotState === "loading" ? (
        <Card className="content-loading" aria-label={t("content.loading") }>
          <Skeleton height="3rem" /><Skeleton height="14rem" /><Skeleton height="4rem" />
        </Card>
      ) : null}

      {snapshotState === "error" ? (
        <Card className="content-editor--empty">
          <EmptyState
            icon={<ShieldAlert />}
            label={t("content.error.snapshotLabel")}
            title={t("content.error.snapshotTitle")}
            description={t("content.error.snapshotDescription")}
            action={<Button onClick={() => void refreshSnapshot(profileId)}><RefreshCw aria-hidden="true" />{t("app.retry")}</Button>}
          />
        </Card>
      ) : null}

      {snapshotState === "ready" && workspace === "installed" ? (
        <div id="content-installed-panel" role="tabpanel" className="content-workspace content-workspace--installed">
          <Card className="content-list-panel">
            <header>
              <div><h3>{t("content.installedTitle", { kind: t(kindKey(kind)) })}</h3><p>{t("content.installedCount", { count: formatNumber(installed.length) })}</p></div>
              <Badge tone="neutral">{t("content.reproducible")}</Badge>
            </header>
            <div className="content-local-actions">
              <input
                ref={localInput}
                hidden
                type="file"
                accept={kind === "mod" ? ".jar" : ".zip"}
                onChange={(event) => void useSelectedFile(event, async (file) => {
                  await runMutation(
                    "local-add",
                    () => contentCommands.addLocal(profileId, file, kind),
                    "content.operation.localAdded",
                  );
                })}
              />
              <input
                ref={packInput}
                hidden
                type="file"
                accept=".mrpack"
                onChange={(event) => void useSelectedFile(event, async (file) => {
                  await runMutation(
                    "pack-import",
                    () => contentCommands.importModrinthPack(profileId, file),
                    "content.operation.packImported",
                  );
                })}
              />
              {kind !== "modpack" ? (
                <Button
                  loading={busy === "local-add"}
                  disabled={snapshot?.localFileCapability.state !== "available"}
                  onClick={() => localInput.current?.click()}
                ><FilePlus2 aria-hidden="true" />{t("content.addLocal")}</Button>
              ) : (
                <Button
                  loading={busy === "pack-import"}
                  disabled={snapshot?.localFileCapability.state !== "available"}
                  onClick={() => packInput.current?.click()}
                ><FileArchive aria-hidden="true" />{t("content.importMrpack")}</Button>
              )}
              <Badge tone={capabilityTone(snapshot?.localFileCapability ?? null)}>
                {snapshot?.localFileCapability.state === "available" ? t("content.localVerified") : t("content.localUnavailable")}
              </Badge>
            </div>
            {installed.length === 0 ? (
              <EmptyState
                icon={<PackageCheck />}
                label={t("content.empty.installedLabel")}
                title={t("content.empty.installedTitle", { kind: t(kindKey(kind)) })}
                description={t("content.empty.installedDescription")}
                action={<Button onClick={() => setWorkspace("discover")}><Search aria-hidden="true" />{t("content.discoverAction")}</Button>}
              />
            ) : (
              <ul className="content-item-list" aria-label={t("content.installedList") }>
                {installed.map((item) => (
                  <li key={item.contentId}>
                    <button
                      type="button"
                      aria-pressed={selectedContentId === item.contentId}
                      onClick={() => setSelectedContentId(item.contentId)}
                    >
                      <span className={`content-item-list__state content-item-list__state--${item.enabled ? "enabled" : "disabled"}`} aria-hidden="true" />
                      <span><strong>{item.displayName}</strong><small>{item.versionNumber} · {item.source === "modrinth" ? t("content.source.modrinth") : t("content.source.local")}</small></span>
                      {item.conflicts.length ? <Badge tone="error">{t("content.conflictCount", { count: item.conflicts.length })}</Badge> : item.managedByPack ? <Badge tone="neutral">{t("content.managedByPack")}</Badge> : item.update ? <Badge tone="accent">{t("content.updateAvailable")}</Badge> : null}
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </Card>

          <Card className="content-detail-panel">
            {selectedContent && selectedContent.contentType === kind ? (
              <>
                <header className="content-detail-panel__heading">
                  <div>
                    <p className="page-eyebrow">{t(kindKey(selectedContent.contentType))}</p>
                    <h3>{selectedContent.displayName}</h3>
                    <p>{selectedContent.versionNumber}</p>
                    {selectedContent.managedByPack ? <Badge tone="neutral">{t("content.managedByPack")}</Badge> : null}
                  </div>
                  <Switch
                    label={selectedContent.enabled ? t("content.enabled") : t("content.disabled")}
                    description={t("content.toggleDescription")}
                    checked={selectedContent.enabled}
                    disabled={busy !== null}
                    onChange={(event) => {
                      const enabled = event.currentTarget.checked;
                      void runMutation(
                        `toggle:${selectedContent.contentId}`,
                        () => contentCommands.setEnabled(profileId, selectedContent.contentId, enabled),
                        enabled ? "content.operation.enabled" : "content.operation.disabled",
                      );
                    }}
                  />
                </header>
                <dl className="content-metadata">
                  <div><dt>{t("content.source")}</dt><dd>{selectedContent.source === "modrinth" ? t("content.source.modrinth") : t("content.source.local")}</dd></div>
                  <div><dt>{t("content.size")}</dt><dd>{t("content.bytes", { bytes: formatNumber(selectedContent.sizeBytes) })}</dd></div>
                  <div><dt>{t("content.sha256")}</dt><dd className="content-hash">{selectedContent.sha256}</dd></div>
                </dl>
                <section className="content-relations" aria-labelledby="installed-dependencies-title">
                  <h4 id="installed-dependencies-title">{t("content.dependencies")}</h4>
                  {selectedContent.dependencies.length ? (
                    <ul>{selectedContent.dependencies.map((dependency) => (
                      <li key={`${dependency.projectId}:${dependency.relation}`}>
                        {dependency.satisfied ? <CheckCircle2 aria-hidden="true" /> : <CircleOff aria-hidden="true" />}
                        <span><strong>{dependency.displayName}</strong><small>{t(`content.relation.${dependency.relation}` as TranslationKey)}</small></span>
                        <Badge tone={dependency.satisfied ? "success" : dependency.relation === "required" ? "error" : "warning"}>
                          {dependency.satisfied ? t("content.satisfied") : t("content.missing")}
                        </Badge>
                      </li>
                    ))}</ul>
                  ) : <p>{t("content.noDependencies")}</p>}
                </section>
                <section className="content-relations" aria-labelledby="installed-conflicts-title">
                  <h4 id="installed-conflicts-title">{t("content.conflicts")}</h4>
                  {selectedContent.conflicts.length ? (
                    <ul>{selectedContent.conflicts.map((conflict) => (
                      <li key={conflict.contentId} className="content-relation--conflict"><ShieldAlert aria-hidden="true" /><span><strong>{conflict.displayName}</strong><small>{t("content.conflictDetected")}</small></span></li>
                    ))}</ul>
                  ) : <p>{t("content.noConflicts")}</p>}
                </section>
                <footer className="content-detail-panel__actions">
                    <Button
                      loading={busy === `update:${selectedContent.contentId}`}
                      disabled={selectedContent.source !== "modrinth" || selectedContent.managedByPack || busy !== null}
                      title={selectedContent.managedByPack ? t("content.managedByPackDescription") : undefined}
                    onClick={() => void runMutation(
                      `update:${selectedContent.contentId}`,
                      () => contentCommands.update(profileId, selectedContent.contentId),
                      "content.operation.updated",
                    )}
                    ><RefreshCw aria-hidden="true" />{selectedContent.update ? t("content.updateTo", { version: selectedContent.update.versionNumber }) : selectedContent.source === "modrinth" ? t("content.checkUpdate") : t("content.upToDate")}</Button>
                  <Button
                    variant="danger"
                    disabled={selectedContent.managedByPack || busy !== null}
                    title={selectedContent.managedByPack ? t("content.managedByPackDescription") : undefined}
                    onClick={() => setRemoveTarget(selectedContent)}
                  ><Trash2 aria-hidden="true" />{t("content.remove")}</Button>
                </footer>
              </>
            ) : (
              <EmptyState
                icon={<ArchiveRestore />}
                label={t("content.empty.selectionLabel")}
                title={t("content.empty.selectionTitle")}
                description={t("content.empty.selectionDescription")}
              />
            )}
          </Card>
        </div>
      ) : null}

      {snapshotState === "ready" && workspace === "discover" ? (
        <div id="content-discover-panel" role="tabpanel" className="content-discover">
          <Card className="content-search-panel">
            <form onSubmit={submitSearch} className="content-search-form">
              <SearchField
                label={t("content.search")}
                placeholder={t("content.searchPlaceholder", { kind: t(kindKey(kind)) })}
                value={query}
                minLength={2}
                maxLength={96}
                onChange={(event) => setQuery(event.currentTarget.value)}
              />
              <SelectField label={t("content.minecraftFilter")} value={snapshot?.minecraftVersion ?? ""} disabled>
                <option value={snapshot?.minecraftVersion ?? ""}>{snapshot?.minecraftVersion ?? t("content.notConfigured")}</option>
              </SelectField>
              <SelectField label={t("content.loaderFilter")} value={snapshot?.loader ?? ""} disabled>
                <option value={snapshot?.loader ?? ""}>{t(loaderKey(snapshot?.loader ?? null))}</option>
              </SelectField>
              <Button
                type="submit"
                variant="primary"
                loading={searchState === "loading"}
                disabled={query.trim().length < 2 || !snapshot?.minecraftVersion || !snapshot.loader}
              ><Search aria-hidden="true" />{t("app.search")}</Button>
            </form>
            {!snapshot?.minecraftVersion || !snapshot.loader ? (
              <Status tone="warning" label={t("status.warning")}>{t("content.runtimeRequired")}</Status>
            ) : null}
            {searchResult?.capability.state !== "available" && searchState === "ready" ? (
              <Status tone="warning" label={t("status.warning")}>{t("content.providerUnavailable")}</Status>
            ) : null}
            {searchState === "idle" ? (
              <EmptyState icon={<Search />} label={t("content.empty.searchLabel")} title={t("content.empty.searchTitle")} description={t("content.empty.searchDescription")} />
            ) : null}
            {searchState === "loading" ? <div className="content-result-loading"><Skeleton height="5rem" /><Skeleton height="5rem" /><Skeleton height="5rem" /></div> : null}
            {searchState === "ready" && searchResult?.capability.state === "available" && searchResult.hits.length === 0 ? (
              <EmptyState icon={<CircleOff />} label={t("content.empty.resultsLabel")} title={t("content.empty.resultsTitle")} description={t("content.empty.resultsDescription")} />
            ) : null}
            {searchState === "ready" && searchResult?.hits.length ? (
              <>
                <div className="content-search-summary"><strong>{t("content.results", { count: formatNumber(searchResult.total) })}</strong><span>{t("content.compatibilityScope")}</span></div>
                <ul className="content-search-results" aria-label={t("content.searchResults") }>
                  {searchResult.hits.map((hit) => (
                    <li key={hit.projectId}>
                      <button type="button" aria-pressed={detail?.projectId === hit.projectId} onClick={() => void loadDetail(hit.projectId)}>
                        <span className="content-search-results__icon"><Blocks aria-hidden="true" /></span>
                        <span><strong>{hit.title}</strong><small>{hit.author} · {formatNumber(hit.downloads)} {t("content.downloads")}</small><p>{hit.description}</p></span>
                        <Badge tone="neutral">{hit.latestVersion ?? t("content.versionUnknown")}</Badge>
                      </button>
                    </li>
                  ))}
                </ul>
                <div className="content-pagination">
                  <Button disabled={searchResult.offset === 0} onClick={() => void searchAt(Math.max(0, searchResult.offset - PAGE_SIZE))}><ChevronLeft aria-hidden="true" />{t("content.previousPage")}</Button>
                  <span>{t("content.resultRange", { from: searchResult.offset + 1, to: Math.min(searchResult.offset + searchResult.hits.length, searchResult.total), total: searchResult.total })}</span>
                  <Button disabled={searchResult.offset + searchResult.hits.length >= searchResult.total} onClick={() => void searchAt(searchResult.offset + PAGE_SIZE)}>{t("content.nextPage")}<ChevronRight aria-hidden="true" /></Button>
                </div>
              </>
            ) : null}
          </Card>

          <Card className="content-project-panel" aria-live="polite">
            {detailState === "loading" ? <div className="content-detail-loading"><Skeleton height="2rem" /><Skeleton height="6rem" /><Skeleton height="3rem" /></div> : null}
            {detailState === "idle" ? (
              <EmptyState icon={<PackageCheck />} label={t("content.empty.projectLabel")} title={t("content.empty.projectTitle")} description={t("content.empty.projectDescription")} />
            ) : null}
            {detailState === "ready" && detail ? (
              <>
                <header className="content-project-panel__heading">
                  <p className="page-eyebrow">{t(kindKey(detail.contentType))}</p>
                  <h3>{detail.title}</h3>
                  <p>{detail.description}</p>
                  <div><Badge tone="neutral">{detail.author}</Badge><Badge tone="neutral">{detail.license}</Badge></div>
                </header>
                <SelectField label={t("content.version")} value={versionId} onChange={(event) => setVersionId(event.currentTarget.value)}>
                  <option value="">{t("content.noCompatibleVersion")}</option>
                  {detail.versions.map((version) => (
                    <option key={version.versionId} value={version.versionId} disabled={!version.compatible}>
                      {version.versionNumber} · {formatDate(version.publishedAtUnix * 1000)}{version.compatible ? "" : ` · ${t("content.incompatible")}`}
                    </option>
                  ))}
                </SelectField>
                {selectedVersion ? (
                  <>
                    <section className="content-relations" aria-labelledby="project-dependencies-title">
                      <h4 id="project-dependencies-title">{t("content.dependencies")}</h4>
                      {selectedVersion.dependencies.length ? <ul>{selectedVersion.dependencies.map((dependency) => (
                        <li key={`${dependency.projectId}:${dependency.relation}`}>
                          {dependency.satisfied ? <CheckCircle2 aria-hidden="true" /> : <CircleOff aria-hidden="true" />}
                          <span><strong>{dependency.displayName}</strong><small>{t(`content.relation.${dependency.relation}` as TranslationKey)}</small></span>
                          <Badge tone={dependency.satisfied ? "success" : dependency.relation === "required" ? "warning" : "neutral"}>{dependency.satisfied ? t("content.installed") : t("content.resolvedOnInstall")}</Badge>
                        </li>
                      ))}</ul> : <p>{t("content.noDependencies")}</p>}
                    </section>
                    <section className="content-relations" aria-labelledby="project-conflicts-title">
                      <h4 id="project-conflicts-title">{t("content.conflicts")}</h4>
                      {selectedVersion.conflicts.length ? <ul>{selectedVersion.conflicts.map((conflict) => (
                        <li key={conflict.contentId} className="content-relation--conflict"><ShieldAlert aria-hidden="true" /><span><strong>{conflict.displayName}</strong><small>{t("content.installBlocked")}</small></span></li>
                      ))}</ul> : <p>{t("content.noConflicts")}</p>}
                    </section>
                  </>
                ) : null}
                {(hasBlockingConflict || hasMissingDependency) ? <Status tone="warning" label={t("status.warning")}>{t("content.installNeedsResolution")}</Status> : null}
                <Button
                  variant="primary"
                  loading={busy === `install:${detail.projectId}`}
                  disabled={!selectedVersion?.compatible || hasBlockingConflict || busy !== null}
                  onClick={() => void installSelected()}
                ><Download aria-hidden="true" />{t("content.installVerified")}</Button>
              </>
            ) : null}
          </Card>
        </div>
      ) : null}

      <ConfirmDialog
        open={Boolean(removeTarget)}
        title={t("content.removeTitle")}
        description={t("content.removeDescription", { name: removeTarget?.displayName ?? "" })}
        confirmLabel={t("content.remove")}
        cancelLabel={t("app.cancel")}
        loading={Boolean(removeTarget && busy === `remove:${removeTarget.contentId}`)}
        onClose={() => setRemoveTarget(null)}
        onConfirm={() => void confirmRemove()}
      />
    </section>
  );
}
