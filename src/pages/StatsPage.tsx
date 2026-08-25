import { useEffect, useMemo, useState } from "react";
import { Activity, BarChart3, CalendarRange, Clock3, Layers3, PackageSearch } from "lucide-react";
import { Card, EmptyState, Skeleton } from "../components/ui";
import { useI18n } from "../i18n/I18nProvider";
import { profileCommands } from "../lib/profileCommands";
import { runtimeCommands } from "../lib/runtimeCommands";
import type { Phase4Profile, Phase5LaunchStatus, Phase5RuntimeStatus } from "../lib/generated/ipc-contracts";

type RangeKey = "day" | "week" | "month";

type SummaryMap = Record<string, number>;

function startOfWeek(date: Date): Date {
  const next = new Date(date);
  const day = next.getDay();
  const diff = (day === 0 ? -6 : 1 - day);
  next.setDate(next.getDate() + diff);
  next.setHours(0, 0, 0, 0);
  return next;
}

function formatRangeLabel(date: Date, key: RangeKey): string {
  if (key === "day") return date.toLocaleDateString(undefined, { month: "short", day: "numeric" });
  if (key === "week") return `${date.toLocaleDateString(undefined, { month: "short", day: "numeric" })}`;
  return date.toLocaleDateString(undefined, { month: "short", year: "2-digit" });
}

function readLaunchBucket(timestampUnix: number, key: RangeKey): string {
  const date = new Date(timestampUnix * 1000);
  if (key === "day") return date.toISOString().slice(0, 10);
  if (key === "week") return startOfWeek(date).toISOString().slice(0, 10);
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}`;
}

export function StatsPage() {
  const { t, formatDate } = useI18n();
  const [profiles, setProfiles] = useState<Phase4Profile[]>([]);
  const [statuses, setStatuses] = useState<Record<string, Phase5RuntimeStatus>>({});
  const [launches, setLaunches] = useState<Phase5LaunchStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);

  useEffect(() => {
    let active = true;
    const refresh = async () => {
      setLoading(true);
      setError(false);
      try {
        const [nextProfiles, nextLaunches] = await Promise.all([
          profileCommands.list(),
          runtimeCommands.launchStatuses(),
        ]);
        const nextStatuses = await Promise.all(nextProfiles.map(async (profile) => {
          try {
            const status = await runtimeCommands.status(profile.id);
            return [profile.id, status] as const;
          } catch {
            return [profile.id, null] as const;
          }
        }));
        if (!active) return;
        setProfiles(nextProfiles);
        setLaunches(nextLaunches);
        setStatuses(Object.fromEntries(nextStatuses.filter((entry): entry is readonly [string, Phase5RuntimeStatus] => entry[1] !== null)));
      } catch {
        if (active) setError(true);
      } finally {
        if (active) setLoading(false);
      }
    };
    void refresh();
    return () => { active = false; };
  }, []);

  const metrics = useMemo(() => {
    const versionCounts: SummaryMap = {};
    const loaderCounts: SummaryMap = {};
    const dayCounts: SummaryMap = {};
    const weekCounts: SummaryMap = {};
    const monthCounts: SummaryMap = {};

    for (const launch of launches) {
      const runtime = statuses[launch.profileId]?.runtime;
      const version = runtime?.minecraftVersion ?? "unknown";
      const loader = runtime?.loader.kind ?? "unknown";
      versionCounts[version] = (versionCounts[version] ?? 0) + 1;
      loaderCounts[loader] = (loaderCounts[loader] ?? 0) + 1;
      const timestamp = launch.startedAtUnix * 1000;
      const dayKey = readLaunchBucket(launch.startedAtUnix, "day");
      const weekKey = readLaunchBucket(launch.startedAtUnix, "week");
      const monthKey = readLaunchBucket(launch.startedAtUnix, "month");
      dayCounts[dayKey] = (dayCounts[dayKey] ?? 0) + 1;
      weekCounts[weekKey] = (weekCounts[weekKey] ?? 0) + 1;
      monthCounts[monthKey] = (monthCounts[monthKey] ?? 0) + 1;
      if (timestamp <= 0) continue;
    }

    const topVersion = Object.entries(versionCounts).sort((a, b) => b[1] - a[1])[0];
    const topLoader = Object.entries(loaderCounts).sort((a, b) => b[1] - a[1])[0];
    const peakDay = Object.entries(dayCounts).sort((a, b) => b[1] - a[1])[0];
    const peakWeek = Object.entries(weekCounts).sort((a, b) => b[1] - a[1])[0];
    const peakMonth = Object.entries(monthCounts).sort((a, b) => b[1] - a[1])[0];

    return {
      versionCounts,
      loaderCounts,
      dayCounts,
      weekCounts,
      monthCounts,
      totalLaunches: launches.length,
      topVersion,
      topLoader,
      peakDay,
      peakWeek,
      peakMonth,
    };
  }, [launches, statuses]);

  const topVersions = useMemo(() => Object.entries(metrics.versionCounts).sort((a, b) => b[1] - a[1]).slice(0, 5), [metrics.versionCounts]);
  const topLoaders = useMemo(() => Object.entries(metrics.loaderCounts).sort((a, b) => b[1] - a[1]).slice(0, 5), [metrics.loaderCounts]);
  const dayLeaders = useMemo(() => Object.entries(metrics.dayCounts).sort((a, b) => b[1] - a[1]).slice(0, 5), [metrics.dayCounts]);
  const weekLeaders = useMemo(() => Object.entries(metrics.weekCounts).sort((a, b) => b[1] - a[1]).slice(0, 5), [metrics.weekCounts]);
  const monthLeaders = useMemo(() => Object.entries(metrics.monthCounts).sort((a, b) => b[1] - a[1]).slice(0, 5), [metrics.monthCounts]);

  const activeProfiles = profiles.filter((profile) => profile.lifecycleState === "active").length;

  return (
    <div className="page stats-page">
      <header className="page-heading">
        <div>
          <p className="page-eyebrow">{t("app.name")}</p>
          <h1>{t("page.stats.title")}</h1>
          <p>{t("page.stats.description")}</p>
        </div>
      </header>

      {error ? <Card className="stats-error"><p>{t("stats.unavailable")}</p></Card> : null}

      {loading ? (
        <div className="stats-grid">
          {Array.from({ length: 4 }).map((_, index) => <Card key={index}><Skeleton height="7rem" /></Card>)}
        </div>
      ) : (
        <>
          <section className="stats-grid" aria-label={t("stats.summary")}>
            <Card className="stats-card"><Activity aria-hidden="true" /><strong>{metrics.totalLaunches}</strong><span>{t("stats.totalLaunches")}</span></Card>
            <Card className="stats-card"><Layers3 aria-hidden="true" /><strong>{activeProfiles}</strong><span>{t("stats.activeProfiles")}</span></Card>
            <Card className="stats-card"><PackageSearch aria-hidden="true" /><strong>{metrics.topVersion ? metrics.topVersion[0] : "—"}</strong><span>{t("stats.topVersion")}</span></Card>
            <Card className="stats-card"><BarChart3 aria-hidden="true" /><strong>{metrics.topLoader ? metrics.topLoader[0] : "—"}</strong><span>{t("stats.topLoader")}</span></Card>
          </section>

          <section className="stats-layout">
            <Card className="stats-panel">
              <header><CalendarRange aria-hidden="true" /><div><h2>{t("stats.buckets.day")}</h2><p>{t("stats.buckets.dayDescription")}</p></div></header>
              {dayLeaders.length > 0 ? <ul className="stats-list">{dayLeaders.map(([label, count]) => <li key={label}><span>{formatRangeLabel(new Date(`${label}T00:00:00`), "day")}</span><strong>{count}</strong></li>)}</ul> : <EmptyState icon={<CalendarRange />} label={t("stats.emptyLabel")} title={t("stats.emptyTitle")} description={t("stats.emptyDescription")} />}
            </Card>

            <Card className="stats-panel">
              <header><Clock3 aria-hidden="true" /><div><h2>{t("stats.buckets.week")}</h2><p>{t("stats.buckets.weekDescription")}</p></div></header>
              {weekLeaders.length > 0 ? <ul className="stats-list">{weekLeaders.map(([label, count]) => <li key={label}><span>{formatRangeLabel(new Date(`${label}T00:00:00`), "week")}</span><strong>{count}</strong></li>)}</ul> : <EmptyState icon={<Clock3 />} label={t("stats.emptyLabel")} title={t("stats.emptyTitle")} description={t("stats.emptyDescription")} />}
            </Card>

            <Card className="stats-panel">
              <header><CalendarRange aria-hidden="true" /><div><h2>{t("stats.buckets.month")}</h2><p>{t("stats.buckets.monthDescription")}</p></div></header>
              {monthLeaders.length > 0 ? <ul className="stats-list">{monthLeaders.map(([label, count]) => <li key={label}><span>{formatRangeLabel(new Date(`${label}-01T00:00:00`), "month")}</span><strong>{count}</strong></li>)}</ul> : <EmptyState icon={<CalendarRange />} label={t("stats.emptyLabel")} title={t("stats.emptyTitle")} description={t("stats.emptyDescription")} />}
            </Card>
          </section>

          <section className="stats-layout stats-layout--double">
            <Card className="stats-panel">
              <header><PackageSearch aria-hidden="true" /><div><h2>{t("stats.versions.title")}</h2><p>{t("stats.versions.description")}</p></div></header>
              {topVersions.length > 0 ? <ul className="stats-list">{topVersions.map(([version, count]) => <li key={version}><span>{version}</span><strong>{count}</strong></li>)}</ul> : <EmptyState icon={<PackageSearch />} label={t("stats.emptyLabel")} title={t("stats.emptyTitle")} description={t("stats.emptyDescription")} />}
            </Card>

            <Card className="stats-panel">
              <header><Layers3 aria-hidden="true" /><div><h2>{t("stats.loaders.title")}</h2><p>{t("stats.loaders.description")}</p></div></header>
              {topLoaders.length > 0 ? <ul className="stats-list">{topLoaders.map(([loader, count]) => <li key={loader}><span>{loader}</span><strong>{count}</strong></li>)}</ul> : <EmptyState icon={<Layers3 />} label={t("stats.emptyLabel")} title={t("stats.emptyTitle")} description={t("stats.emptyDescription")} />}
            </Card>
          </section>

          <Card className="stats-panel stats-panel--wide">
            <header><BarChart3 aria-hidden="true" /><div><h2>{t("stats.timeline.title")}</h2><p>{t("stats.timeline.description")}</p></div></header>
            <div className="stats-summary-grid">
              <div><dt>{t("stats.peakDay")}</dt><dd>{metrics.peakDay ? `${metrics.peakDay[0]} · ${metrics.peakDay[1]}` : "—"}</dd></div>
              <div><dt>{t("stats.peakWeek")}</dt><dd>{metrics.peakWeek ? `${metrics.peakWeek[0]} · ${metrics.peakWeek[1]}` : "—"}</dd></div>
              <div><dt>{t("stats.peakMonth")}</dt><dd>{metrics.peakMonth ? `${metrics.peakMonth[0]} · ${metrics.peakMonth[1]}` : "—"}</dd></div>
            </div>
          </Card>
        </>
      )}
    </div>
  );
}
