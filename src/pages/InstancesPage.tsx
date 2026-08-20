import { useCallback, useEffect, useMemo, useState } from "react";
import { Activity, CircleAlert, CircleStop, Cpu, RefreshCw, Square } from "lucide-react";
import { Badge, Button, Card, EmptyState, Status } from "../components/ui";
import { useI18n } from "../i18n/I18nProvider";
import { runtimeCommands } from "../lib/runtimeCommands";
import { profileCommands } from "../lib/profileCommands";
import type { Phase4Profile, Phase5LaunchStatus } from "../lib/generated/ipc-contracts";

function launchTone(state: Phase5LaunchStatus["state"]): "success" | "warning" | "error" | "info" | "neutral" {
  if (state === "running") return "success";
  if (state === "starting") return "info";
  if (state === "stopping") return "warning";
  if (state === "failed") return "error";
  return "neutral";
}

export function InstancesPage() {
  const { t, formatDate } = useI18n();
  const [profiles, setProfiles] = useState<Phase4Profile[]>([]);
  const [launches, setLaunches] = useState<Phase5LaunchStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [stopping, setStopping] = useState<string | null>(null);
  const [error, setError] = useState(false);
  const refresh = useCallback(async () => {
    setError(false);
    try {
      const [nextProfiles, nextLaunches] = await Promise.all([profileCommands.list(), runtimeCommands.launchStatuses()]);
      setProfiles(nextProfiles);
      setLaunches(nextLaunches);
    } catch { setError(true); } finally { setLoading(false); }
  }, []);
  useEffect(() => { void refresh(); }, [refresh]);
  useEffect(() => {
    if (!launches.some((item) => ["starting", "running", "stopping"].includes(item.state))) return;
    const timer = window.setInterval(() => void refresh(), 2_000);
    return () => window.clearInterval(timer);
  }, [launches, refresh]);
  const names = useMemo(() => new Map(profiles.map((profile) => [profile.id, profile.displayName])), [profiles]);
  const active = launches.filter((item) => item.state !== "exited" && item.state !== "failed");
  const stop = async (launch: Phase5LaunchStatus) => {
    setStopping(launch.launchId);
    try { await runtimeCommands.stop(launch.launchId); await refresh(); } catch { setError(true); } finally { setStopping(null); }
  };
  return <div className="page instances-page">
    <header className="page-heading"><div><p className="page-eyebrow">{t("app.name")}</p><h1>{t("page.instances.title")}</h1><p>{t("page.instances.description")}</p></div><Button onClick={() => { setLoading(true); void refresh(); }} loading={loading}><RefreshCw aria-hidden="true" />{t("app.retry")}</Button></header>
    {error ? <Status tone="error" label={t("status.error")}>{t("instances.error")}</Status> : null}
    <section className="instances-overview" aria-label={t("instances.overview")}><Card><Activity aria-hidden="true" /><strong>{active.length}</strong><span>{t("instances.active")}</span></Card><Card><Cpu aria-hidden="true" /><strong>{launches.length}</strong><span>{t("instances.recent")}</span></Card></section>
    {loading && launches.length === 0 ? <Card className="instances-empty"><EmptyState icon={<RefreshCw className="ui-spin" />} label={t("app.loading")} title={t("app.loading")} description={t("instances.loading")} /></Card> : null}
    {!loading && launches.length === 0 ? <Card className="instances-empty"><EmptyState icon={<CircleStop />} label={t("instances.overview")} title={t("instances.emptyTitle")} description={t("instances.emptyDescription")} /></Card> : null}
    {launches.length > 0 ? <section className="instances-list" aria-label={t("instances.list")}>{launches.map((launch) => <Card className="instance-card" key={launch.launchId}><header><div><h2>{names.get(launch.profileId) ?? t("instances.unknownProfile")}</h2><p>{launch.accountName}</p></div><Badge tone={launchTone(launch.state)}>{t(`instances.state.${launch.state}`)}</Badge></header><dl><div><dt>{t("instances.started")}</dt><dd>{formatDate(launch.startedAtUnix * 1000)}</dd></div><div><dt>{t("instances.process")}</dt><dd>{launch.processId ?? "—"}</dd></div><div><dt>{t("instances.exitCode")}</dt><dd>{launch.exitCode ?? "—"}</dd></div></dl>{["starting", "running", "stopping"].includes(launch.state) ? <Button variant="danger" loading={stopping === launch.launchId} disabled={launch.state === "stopping"} onClick={() => void stop(launch)}><Square aria-hidden="true" />{t("instances.stop")}</Button> : <span className="instance-card__ended"><CircleAlert aria-hidden="true" />{t("instances.historyNote")}</span>}</Card>)}</section> : null}
  </div>;
}
