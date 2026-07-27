import { useCallback, useEffect, useMemo, useState } from "react";
import { Archive, Copy, Database, FolderPlus, RotateCcw, Search, Star, Trash2 } from "lucide-react";
import { profileCommands } from "../lib/profileCommands";
import { typedIpcError } from "../lib/shellCommands";
import type { Phase4CacheGcReport, Phase4Profile } from "../lib/generated/ipc-contracts";
import {
  Badge,
  Button,
  Card,
  ConfirmDialog,
  Dialog,
  EmptyState,
  SearchField,
  Status,
  Tabs,
  TextField,
} from "../components/ui";
import { useI18n } from "../i18n/I18nProvider";
import type { TranslationKey } from "../i18n/messages";

type Lifecycle = Phase4Profile["lifecycleState"];
type EditorState = { mode: "create"; source: null; name: string } | { mode: "duplicate"; source: Phase4Profile; name: string };

export function LibraryPage() {
  const { t, formatDate, formatNumber } = useI18n();
  const [profiles, setProfiles] = useState<Phase4Profile[]>([]);
  const [cacheReport, setCacheReport] = useState<Phase4CacheGcReport | null>(null);
  const [lifecycle, setLifecycle] = useState<Lifecycle>("active");
  const [query, setQuery] = useState("");
  const [editor, setEditor] = useState<EditorState | null>(null);
  const [trashTarget, setTrashTarget] = useState<Phase4Profile | null>(null);
  const [cacheDialogOpen, setCacheDialogOpen] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [errorKey, setErrorKey] = useState<TranslationKey | null>(null);

  const reload = useCallback(async () => {
    const [nextProfiles, nextCacheReport] = await Promise.all([
      profileCommands.list(),
      profileCommands.cachePreview(),
    ]);
    setProfiles(nextProfiles);
    setCacheReport(nextCacheReport);
  }, []);
  const closeEditor = useCallback(() => setEditor(null), []);
  const closeTrashDialog = useCallback(() => setTrashTarget(null), []);
  const closeCacheDialog = useCallback(() => setCacheDialogOpen(false), []);
  useEffect(() => { void reload().catch(() => setErrorKey("error.internal_error")); }, [reload]);

  const report = (error: unknown) => {
    const typed = typedIpcError(error);
    setErrorKey((typed?.messageKey as TranslationKey | undefined) ?? "error.internal_error");
  };

  const visibleProfiles = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    return profiles.filter((profile) => profile.lifecycleState === lifecycle
      && (!normalized || profile.displayName.toLocaleLowerCase().includes(normalized)));
  }, [lifecycle, profiles, query]);

  const saveEditor = async () => {
    if (!editor) return;
    setBusy("editor");
    setErrorKey(null);
    try {
      if (editor.mode === "create") await profileCommands.create(editor.name);
      else await profileCommands.duplicate(editor.source.id, editor.name);
      setEditor(null);
      setLifecycle("active");
      await reload();
    } catch (error) {
      report(error);
    } finally {
      setBusy(null);
    }
  };

  const runAction = async (key: string, action: () => Promise<unknown>) => {
    setBusy(key);
    setErrorKey(null);
    try {
      await action();
      await reload();
    } catch (error) {
      report(error);
    } finally {
      setBusy(null);
    }
  };

  const moveToTrash = async () => {
    if (!trashTarget) return;
    const target = trashTarget;
    await runAction(`trash:${target.id}`, () => profileCommands.trash(target.id));
    setTrashTarget(null);
  };

  const quarantineCache = async () => {
    setBusy("cache");
    setErrorKey(null);
    try {
      await profileCommands.quarantineUnreferenced();
      await reload();
      setCacheDialogOpen(false);
    } catch (error) {
      report(error);
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="page library-page">
      <header className="page-heading">
        <div><p className="page-eyebrow">{t("app.name")}</p><h1>{t("page.library.title")}</h1><p>{t("page.library.description")}</p></div>
        <Button variant="primary" onClick={() => setEditor({ mode: "create", source: null, name: "" })}>
          <FolderPlus aria-hidden="true" />{t("library.create")}
        </Button>
      </header>

      {errorKey ? <Status tone="error" label={t("status.error")}>{t(errorKey)}</Status> : null}
      <div className="library-toolbar">
        <Tabs
          label={t("library.lifecycleFilter")}
          value={lifecycle}
          onChange={(value) => setLifecycle(value as Lifecycle)}
          items={[
            { value: "active", label: t("library.active") },
            { value: "archived", label: t("library.archived") },
            { value: "trash", label: t("library.trash") },
          ]}
        />
        <SearchField
          label={t("library.search")}
          placeholder={t("library.searchPlaceholder")}
          value={query}
          onChange={(event) => setQuery(event.currentTarget.value)}
        />
      </div>

      <Card className="storage-overview">
        <header>
          <div><h2><Database aria-hidden="true" />{t("library.storageTitle")}</h2><p>{t("library.storageDescription")}</p></div>
          <Badge tone="warning">{t("library.permanentDeletionDisabled")}</Badge>
        </header>
        <dl>
          <div><dt>{t("library.cacheBlobs")}</dt><dd>{formatNumber(cacheReport?.scannedBlobs ?? 0)}</dd></div>
          <div><dt>{t("library.cacheReachable")}</dt><dd>{formatNumber(cacheReport?.reachableBlobs ?? 0)}</dd></div>
          <div><dt>{t("library.cacheEligible")}</dt><dd>{t("library.cacheEligibleValue", { count: formatNumber(cacheReport?.eligibleForQuarantine ?? 0), bytes: formatNumber(cacheReport?.eligibleBytes ?? 0) })}</dd></div>
          <div><dt>{t("library.cacheQuarantined")}</dt><dd>{formatNumber(cacheReport?.retainedInQuarantine ?? 0)}</dd></div>
        </dl>
        <Button
          disabled={!cacheReport?.eligibleForQuarantine}
          loading={busy === "cache"}
          onClick={() => setCacheDialogOpen(true)}
        >{t("library.reviewCleanup")}</Button>
      </Card>

      {visibleProfiles.length === 0 ? (
        <Card className="library-empty">
          <EmptyState
            icon={<Search />}
            label={t("empty.previewLabel")}
            title={t("library.emptyTitle")}
            description={t("library.emptyDescription")}
            action={lifecycle === "active" ? <Button variant="primary" onClick={() => setEditor({ mode: "create", source: null, name: "" })}>{t("library.create")}</Button> : undefined}
          />
        </Card>
      ) : (
        <div className="profile-grid" role="list" aria-label={t("library.profileList") }>
          {visibleProfiles.map((profile) => (
            <Card className="profile-card" role="listitem" key={profile.id}>
              <header>
                <div><h2>{profile.displayName}</h2><p>{t("library.revision", { revision: profile.activeRevisionId.slice(-8) })}</p></div>
                <Badge tone={profile.verificationState === "verified" ? "success" : "warning"}>
                  {profile.verificationState === "verified" ? t("library.verified") : t("library.unverified")}
                </Badge>
              </header>
              <dl>
                <div><dt>{t("library.account")}</dt><dd>{profile.accountId ? t("library.accountAssigned") : t("library.noAccount")}</dd></div>
                <div><dt>{t("library.updated")}</dt><dd>{formatDate(profile.updatedAtUnix * 1000)}</dd></div>
              </dl>
              <div className="profile-card__actions">
                {profile.lifecycleState !== "trash" ? <Button
                  loading={busy === `favorite:${profile.id}`}
                  onClick={() => void runAction(`favorite:${profile.id}`, () => profileCommands.setFavorite(profile.id, !profile.favorite))}
                ><Star aria-hidden="true" fill={profile.favorite ? "currentColor" : "none"} />{profile.favorite ? t("library.unfavorite") : t("library.favorite")}</Button> : null}
                {profile.lifecycleState !== "trash" ? <Button onClick={() => setEditor({ mode: "duplicate", source: profile, name: t("library.copyName", { name: profile.displayName }) })}><Copy aria-hidden="true" />{t("library.duplicate")}</Button> : null}
                {profile.lifecycleState === "active" ? <Button
                  loading={busy === `archive:${profile.id}`}
                  onClick={() => void runAction(`archive:${profile.id}`, () => profileCommands.archive(profile.id))}
                ><Archive aria-hidden="true" />{t("library.archive")}</Button> : null}
                {profile.lifecycleState === "archived" || profile.lifecycleState === "trash" ? <Button
                  loading={busy === `restore:${profile.id}`}
                  onClick={() => void runAction(`restore:${profile.id}`, () => profileCommands.restore(profile.id))}
                ><RotateCcw aria-hidden="true" />{profile.lifecycleState === "trash" ? t("library.restore") : t("library.unarchive")}</Button> : null}
                {profile.lifecycleState !== "trash" ? <Button variant="danger" onClick={() => setTrashTarget(profile)}><Trash2 aria-hidden="true" />{t("library.moveToTrash")}</Button> : null}
              </div>
            </Card>
          ))}
        </div>
      )}

      <Dialog
        open={Boolean(editor)}
        title={editor?.mode === "duplicate" ? t("library.duplicateTitle") : t("library.createTitle")}
        description={editor?.mode === "duplicate" ? t("library.duplicateDescription") : t("library.createDescription")}
        onClose={closeEditor}
        footer={<><Button onClick={closeEditor}>{t("app.cancel")}</Button><Button variant="primary" loading={busy === "editor"} disabled={!editor?.name.trim()} onClick={() => void saveEditor()}>{editor?.mode === "duplicate" ? t("library.duplicate") : t("library.create")}</Button></>}
      >
        {editor ? <TextField label={t("library.name")} value={editor.name} maxLength={64} autoFocus onChange={(event) => setEditor({ ...editor, name: event.currentTarget.value })} /> : null}
      </Dialog>

      <ConfirmDialog
        open={Boolean(trashTarget)}
        title={t("library.trashTitle")}
        description={t("library.trashDescription", { name: trashTarget?.displayName ?? "" })}
        confirmLabel={t("library.moveToTrash")}
        cancelLabel={t("app.cancel")}
        loading={Boolean(trashTarget && busy === `trash:${trashTarget.id}`)}
        onClose={closeTrashDialog}
        onConfirm={() => void moveToTrash()}
      />
      <ConfirmDialog
        open={cacheDialogOpen}
        title={t("library.cacheCleanupTitle")}
        description={t("library.cacheCleanupDescription", { count: formatNumber(cacheReport?.eligibleForQuarantine ?? 0) })}
        confirmLabel={t("library.quarantineCache")}
        cancelLabel={t("app.cancel")}
        loading={busy === "cache"}
        onClose={closeCacheDialog}
        onConfirm={() => void quarantineCache()}
      />
    </div>
  );
}
