import { useCallback, useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import { ContentEditor } from "../components/content/ContentEditor";
import { Button, ErrorState, Skeleton } from "../components/ui";
import { useI18n } from "../i18n/I18nProvider";
import type { Phase4Profile } from "../lib/generated/ipc-contracts";
import { profileCommands } from "../lib/profileCommands";

type LoadState = "loading" | "ready" | "error";

export function DiscoverPage() {
  const { t } = useI18n();
  const [profiles, setProfiles] = useState<Phase4Profile[]>([]);
  const [state, setState] = useState<LoadState>("loading");

  const reload = useCallback(async () => {
    setState("loading");
    try {
      setProfiles(await profileCommands.list());
      setState("ready");
    } catch {
      setState("error");
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  return (
    <div className="page discover-page">
      {state === "loading" ? (
        <div className="discover-page__loading" role="status" aria-label={t("app.loading")}>
          <Skeleton height="7rem" />
          <Skeleton height="26rem" />
        </div>
      ) : null}
      {state === "error" ? (
        <ErrorState
          title={t("app.unavailableTitle")}
          description={t("error.internal_error")}
          action={<Button onClick={() => void reload()}><RefreshCw aria-hidden="true" />{t("app.retry")}</Button>}
        />
      ) : null}
      {state === "ready" ? <ContentEditor profiles={profiles} onProfilesChanged={reload} mode="discover" /> : null}
    </div>
  );
}
