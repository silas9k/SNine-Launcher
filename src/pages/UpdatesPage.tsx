import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ArchiveRestore,
  Box,
  Check,
  ChevronRight,
  Download,
  History,
  PackageCheck,
  RefreshCw,
  RotateCcw,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { Badge, Button, Checkbox, ConfirmDialog, Dialog, EmptyState, ErrorState, SelectField, Skeleton, Switch, TextField } from "../components/ui";
import { useI18n } from "../i18n/I18nProvider";
import type {
  Phase7Revision,
  Phase7RestorePoint,
  Phase7UpdateChannel,
  Phase7UpdatePolicy,
  Phase7UpdatePreview,
  Phase7UpdateProfile,
  Phase7UpdateSnapshot,
} from "../lib/generated/ipc-contracts";
import { formatDate, shortId } from "../lib/format";
import { typedIpcError } from "../lib/shellCommands";
import { updateCommands } from "../lib/updateCommands";
import type { TranslationKey } from "../i18n/messages";

type BusyAction = "load" | "preview" | "apply" | "backup" | "policy" | "rollback" | "restore" | null;

const channelIcons = {
  launcher: Sparkles,
  profiles: Box,
  "s9lab-component": ShieldCheck,
  content: PackageCheck,
} as const;

const channelLabels: Record<Phase7UpdateChannel["channel"], TranslationKey> = {
  launcher: "updates.channel.launcher",
  profiles: "updates.channel.profiles",
  "s9lab-component": "updates.channel.component",
  content: "updates.channel.content",
};

function messageFor(error: unknown): TranslationKey {
  return (typedIpcError(error)?.messageKey as TranslationKey | undefined) ?? "updates.error.generic";
}

export function UpdatesPage() {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = useState<Phase7UpdateSnapshot | null>(null);
  const [selectedProfileId, setSelectedProfileId] = useState("");
  const [preview, setPreview] = useState<Phase7UpdatePreview | null>(null);
  const [selectedChanges, setSelectedChanges] = useState<string[]>([]);
  const [busy, setBusy] = useState<BusyAction>("load");
  const [error, setError] = useState<TranslationKey | null>(null);
  const [rollbackTarget, setRollbackTarget] = useState<Phase7Revision | null>(null);
  const [notice, setNotice] = useState<TranslationKey | null>(null);
  const [restoreTarget, setRestoreTarget] = useState<Phase7RestorePoint | null>(null);
  const [restoreName, setRestoreName] = useState("");
  const [restoreOptions, setRestoreOptions] = useState({ includeAccount: true, includeSettings: false, includeFiles: true });
  const automaticRunStarted = useRef(false);

  const load = useCallback(async () => {
    setBusy("load");
    setError(null);
    try {
      const next = await updateCommands.snapshot();
      setSnapshot(next);
      setSelectedProfileId((current) => current && next.profiles.some((profile) => profile.profileId === current)
        ? current
        : next.profiles[0]?.profileId ?? "");
    } catch (nextError) {
      setError(messageFor(nextError));
    } finally {
      setBusy(null);
    }
  }, []);

  useEffect(() => { void load(); }, [load]);
  useEffect(() => {
    if (!snapshot || automaticRunStarted.current || snapshot.policy.profiles !== "automatic" || snapshot.policy.content !== "automatic") return;
    automaticRunStarted.current = true;
    setBusy("apply");
    void updateCommands.runAutomatic().then(async (results) => {
      if (results.length) {
        await load();
        setNotice("updates.notice.automaticApplied");
      }
    }).catch((nextError) => setError(messageFor(nextError))).finally(() => setBusy(null));
  }, [load, snapshot]);
  useEffect(() => {
    setPreview(null);
    setSelectedChanges([]);
    setNotice(null);
  }, [selectedProfileId]);

  const selectedProfile = useMemo(
    () => snapshot?.profiles.find((profile) => profile.profileId === selectedProfileId) ?? null,
    [selectedProfileId, snapshot],
  );
  const restorePoints = useMemo(
    () => snapshot?.restorePoints.filter((point) => point.profileId === selectedProfileId) ?? [],
    [selectedProfileId, snapshot],
  );

  const check = async () => {
    if (!selectedProfileId) return;
    setBusy("preview");
    setError(null);
    setNotice(null);
    try {
      const next = await updateCommands.preview(selectedProfileId);
      setPreview(next);
      setSelectedChanges(next.changes.map((change) => change.itemId));
    } catch (nextError) {
      setError(messageFor(nextError));
    } finally {
      setBusy(null);
    }
  };

  const saveChannelMode = async (channel: Phase7UpdateChannel["channel"], automatic: boolean) => {
    if (!snapshot) return;
    const field = channel === "s9lab-component" ? "s9labComponent" : channel;
    const policy = { ...snapshot.policy, [field]: automatic ? "automatic" : "manual" } as Phase7UpdatePolicy;
    setBusy("policy");
    setError(null);
    try {
      setSnapshot(await updateCommands.savePolicy(policy));
    } catch (nextError) {
      setError(messageFor(nextError));
    } finally {
      setBusy(null);
    }
  };

  const createBackup = async () => {
    if (!selectedProfileId) return;
    setBusy("backup");
    setError(null);
    try {
      await updateCommands.createRestorePoint(selectedProfileId);
      await load();
      setNotice("updates.notice.backupCreated");
    } catch (nextError) {
      setError(messageFor(nextError));
    } finally {
      setBusy(null);
    }
  };

  const apply = async () => {
    if (!selectedProfileId || selectedChanges.length === 0) return;
    setBusy("apply");
    setError(null);
    try {
      await updateCommands.apply(selectedProfileId, selectedChanges);
      await load();
      setPreview(null);
      setSelectedChanges([]);
      setNotice("updates.notice.applied");
    } catch (nextError) {
      setError(messageFor(nextError));
    } finally {
      setBusy(null);
    }
  };

  const rollback = async () => {
    if (!selectedProfileId || !rollbackTarget) return;
    setBusy("rollback");
    setError(null);
    try {
      await updateCommands.rollback(selectedProfileId, rollbackTarget.revisionId);
      setRollbackTarget(null);
      await load();
      setNotice("updates.notice.rolledBack");
    } catch (nextError) {
      setError(messageFor(nextError));
    } finally {
      setBusy(null);
    }
  };

  const openRestore = (point: Phase7RestorePoint) => {
    setRestoreTarget(point);
    setRestoreName(`${point.profileName} · ${t("updates.recoveredSuffix")}`);
    setRestoreOptions({ includeAccount: true, includeSettings: false, includeFiles: true });
  };

  const restoreBackup = async () => {
    if (!restoreTarget || !restoreName.trim()) return;
    setBusy("restore");
    setError(null);
    try {
      await updateCommands.restoreBackup(restoreTarget.backupId, restoreName, restoreOptions);
      setRestoreTarget(null);
      await load();
      setNotice("updates.notice.restoredCopy");
    } catch (nextError) {
      setError(messageFor(nextError));
    } finally {
      setBusy(null);
    }
  };

  if (busy === "load" && !snapshot) {
    return <div className="page updates-page"><header className="page-heading"><div><Skeleton width="7rem" /><Skeleton width="18rem" height="2.4rem" /></div></header><div className="updates-loading"><Skeleton height="8rem" /><Skeleton height="24rem" /></div></div>;
  }
  if (error && !snapshot) {
    return <div className="page updates-page"><ErrorState title={t("updates.error.title")} description={t(error)} action={<Button onClick={() => void load()}>{t("app.retry")}</Button>} /></div>;
  }

  return (
    <div className="page updates-page">
      <header className="page-heading updates-heading">
        <div>
          <p className="page-eyebrow">{t("updates.eyebrow")}</p>
          <h1>{t("updates.title")}</h1>
          <p>{t("updates.description")}</p>
        </div>
        <div className="updates-heading__actions">
          <Button onClick={() => void createBackup()} loading={busy === "backup"} disabled={!selectedProfile}><ArchiveRestore aria-hidden="true" />{t("updates.createRestorePoint")}</Button>
          <Button variant="primary" onClick={() => void check()} loading={busy === "preview"} disabled={!selectedProfile}><RefreshCw aria-hidden="true" />{t("updates.checkNow")}</Button>
        </div>
      </header>

      {error ? <div className="updates-inline-error" role="alert">{t(error)}</div> : null}
      {notice ? <div className="updates-inline-success" role="status"><Check aria-hidden="true" />{t(notice)}</div> : null}

      <section className="updates-channels" aria-label={t("updates.channels") }>
        {snapshot?.channels.map((channel) => {
          const Icon = channelIcons[channel.channel];
          const available = channel.state === "available";
          return (
            <article className={`update-channel update-channel--${channel.state}`} key={channel.channel}>
              <span className="update-channel__icon"><Icon aria-hidden="true" /></span>
              <div className="update-channel__copy"><strong>{t(channelLabels[channel.channel])}</strong><small>{available ? t("updates.channel.ready") : t("updates.channel.unconfigured")}</small></div>
              <Badge tone={available ? "success" : "warning"}>{available ? t("updates.available") : t("updates.blocked")}</Badge>
              <Switch
                label={t("updates.automatic")}
                checked={channel.mode === "automatic"}
                disabled={!available || busy === "policy"}
                onChange={(event) => void saveChannelMode(channel.channel, event.target.checked)}
              />
            </article>
          );
        })}
      </section>

      <div className="updates-workspace">
        <section className="updates-main-panel" aria-labelledby="updates-plan-title">
          <div className="updates-section-head">
            <div><p className="page-eyebrow">{t("updates.planEyebrow")}</p><h2 id="updates-plan-title">{t("updates.planTitle")}</h2></div>
            <SelectField label={t("updates.profile") } value={selectedProfileId} onChange={(event) => setSelectedProfileId(event.target.value)}>
              {snapshot?.profiles.map((profile) => <option key={profile.profileId} value={profile.profileId}>{profile.displayName}</option>)}
            </SelectField>
          </div>

          {!selectedProfile ? (
            <EmptyState icon={<Box />} label={t("updates.emptyProfiles") } title={t("updates.emptyProfiles") } description={t("updates.emptyProfilesDescription") } />
          ) : !preview ? (
            <div className="updates-ready-state">
              <div className="updates-ready-state__visual"><ShieldCheck aria-hidden="true" /><span>{t("updates.protected")}</span></div>
              <div><h3>{t("updates.readyTitle")}</h3><p>{t("updates.readyDescription")}</p></div>
              <dl><div><dt>{t("updates.activeRevision")}</dt><dd>{shortId(selectedProfile.activeRevisionId)}</dd></div><div><dt>{t("updates.restorePoints")}</dt><dd>{restorePoints.length}</dd></div></dl>
            </div>
          ) : preview.changes.length === 0 ? (
            <EmptyState icon={<PackageCheck />} label={t("updates.upToDate") } title={t("updates.upToDate") } description={t("updates.upToDateDescription") } action={<Button onClick={() => setPreview(null)}>{t("updates.closePreview")}</Button>} />
          ) : (
            <div className="update-plan">
              <div className="update-plan__summary"><div><Download aria-hidden="true" /><span><strong>{t("updates.changesCount", { count: preview.changes.length })}</strong><small>{t("updates.restorePointAutomatic")}</small></span></div><Badge tone="accent">{t("updates.verified")}</Badge></div>
              <div className="update-change-list">
                {preview.changes.map((change) => (
                  <div className="update-change" key={change.itemId}>
                    <Checkbox label={change.displayName} checked={selectedChanges.includes(change.itemId)} onChange={(event) => setSelectedChanges((current) => event.target.checked ? [...current, change.itemId] : current.filter((id) => id !== change.itemId))} />
                    <span className="update-change__versions"><code>{change.currentVersion}</code><ChevronRight aria-hidden="true" /><code>{change.targetVersion}</code></span>
                    <span className="update-change__trust"><ShieldCheck aria-hidden="true" />{t("updates.hashVerified")}</span>
                  </div>
                ))}
              </div>
              <footer className="update-plan__footer"><span>{t("updates.selectedCount", { count: selectedChanges.length })}</span><div><Button onClick={() => setPreview(null)}>{t("app.cancel")}</Button><Button variant="primary" loading={busy === "apply"} disabled={selectedChanges.length === 0} onClick={() => void apply()}>{t("updates.apply")}</Button></div></footer>
            </div>
          )}
        </section>

        <aside className="updates-recovery" aria-labelledby="updates-recovery-title">
          <div className="updates-section-head"><div><p className="page-eyebrow">{t("updates.recoveryEyebrow")}</p><h2 id="updates-recovery-title">{t("updates.recoveryTitle")}</h2></div><History aria-hidden="true" /></div>
          <div className="recovery-strip"><span>{restorePoints.length}</span><div><strong>{t("updates.localBackups")}</strong><small>{t("updates.localBackupsDescription")}</small></div></div>
          {restorePoints.length ? <div className="restore-point-list">{restorePoints.slice(0, 3).map((point) => <button type="button" key={point.backupId} onClick={() => openRestore(point)}><ArchiveRestore aria-hidden="true" /><span><strong>{formatDate(point.createdAtUnix)}</strong><small>{t("updates.backupFiles", { count: point.fileCount })}</small></span><ChevronRight aria-hidden="true" /></button>)}</div> : null}
          <div className="revision-timeline">
            {selectedProfile?.revisions.map((revision) => (
              <div className={`revision-entry ${revision.active ? "revision-entry--active" : ""}`} key={revision.revisionId}>
                <span className="revision-entry__dot" />
                <div><strong>{revision.active ? t("updates.current") : t("updates.revision")}</strong><small>{formatDate(revision.createdAtUnix)}</small><code>{shortId(revision.revisionId)}</code></div>
                {revision.active ? <Badge tone="success">{t("updates.active")}</Badge> : <Button variant="ghost" onClick={() => setRollbackTarget(revision)}><RotateCcw aria-hidden="true" />{t("updates.rollback")}</Button>}
              </div>
            ))}
          </div>
        </aside>
      </div>

      <ConfirmDialog
        open={Boolean(rollbackTarget)}
        title={t("updates.rollbackTitle")}
        description={t("updates.rollbackDescription")}
        confirmLabel={t("updates.rollbackConfirm")}
        cancelLabel={t("app.cancel")}
        loading={busy === "rollback"}
        onClose={() => setRollbackTarget(null)}
        onConfirm={() => void rollback()}
      />
      <Dialog
        open={Boolean(restoreTarget)}
        title={t("updates.restoreBackupTitle")}
        description={t("updates.restoreBackupDescription")}
        onClose={() => setRestoreTarget(null)}
        footer={<><Button onClick={() => setRestoreTarget(null)}>{t("app.cancel")}</Button><Button variant="primary" loading={busy === "restore"} disabled={!restoreName.trim()} onClick={() => void restoreBackup()}>{t("updates.restoreAsCopy")}</Button></>}
      >
        <div className="restore-options">
          <TextField label={t("updates.restoreName")} value={restoreName} maxLength={64} onChange={(event) => setRestoreName(event.target.value)} />
          <Checkbox label={t("updates.restoreFiles")} description={t("updates.restoreFilesDescription")} checked={restoreOptions.includeFiles} onChange={(event) => setRestoreOptions((current) => ({ ...current, includeFiles: event.target.checked }))} />
          <Checkbox label={t("updates.restoreAccount")} description={t("updates.restoreAccountDescription")} checked={restoreOptions.includeAccount} onChange={(event) => setRestoreOptions((current) => ({ ...current, includeAccount: event.target.checked }))} />
          <Checkbox label={t("updates.restoreSettings")} description={t("updates.restoreSettingsDescription")} checked={restoreOptions.includeSettings} onChange={(event) => setRestoreOptions((current) => ({ ...current, includeSettings: event.target.checked }))} />
        </div>
      </Dialog>
    </div>
  );
}
