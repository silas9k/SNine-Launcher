import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
} from "react";
import {
  Box,
  CircleOff,
  Download,
  Play,
  RefreshCw,
  ShieldAlert,
  ShieldCheck,
  Square,
  Wrench,
} from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import type { TranslationKey } from "../../i18n/messages";
import {
  type Phase4Profile,
  type Phase5CapabilityStatus,
  type Phase5ComponentCatalog,
  type Phase5LaunchStatus,
  type Phase5RuntimeCatalog,
  type Phase5RuntimeIntent,
  type Phase5RuntimeStatus,
} from "../../lib/generated/ipc-contracts";
import { runtimeCommands } from "../../lib/runtimeCommands";
import { contentCommands } from "../../lib/contentCommands";
import { typedIpcError } from "../../lib/shellCommands";
import {
  Badge,
  Button,
  SelectField,
  Skeleton,
  Status,
  Switch,
  TextField,
} from "../ui";
import { authCommands } from "../../lib/authCommands";

type LoadState = "idle" | "loading" | "ready" | "error";
type BusyAction =
  | "installing"
  | "repairing"
  | "launching"
  | "stopping"
  | "component-changing"
  | "assigning-account"
  | "standard-installing"
  | null;

type StandardSetupStage = "runtime" | "content" | null;

const SNINE_STANDARD_MINECRAFT_VERSION = "1.21.11";
const SNINE_STANDARD_MODRINTH_PROJECTS = [
  "P7dR8mSH", // Fabric API
  "AANobbMI", // Sodium
  "gvQqBUqZ", // Lithium
  "uXXizFIs", // FerriteCore
  "mOgUt4GM", // Mod Menu
  "5ZwdcRci", // ImmediatelyFast
  "NNAgCjsB", // Entity Culling
] as const;

interface RuntimeDraft {
  minecraftVersion: string;
  loaderKind: "vanilla" | "fabric" | "neoforge";
  loaderVersion: string;
  javaMode: "managed" | "system";
  javaMajor: 17 | 21 | 25;
  componentEnabled: boolean;
  componentId: string;
  componentVersion: string;
}

const EMPTY_DRAFT: RuntimeDraft = {
  minecraftVersion: "",
  loaderKind: "vanilla",
  loaderVersion: "",
  javaMode: "system",
  javaMajor: 21,
  componentEnabled: false,
  componentId: "",
  componentVersion: "",
};

const RUNTIME_ERROR_KEYS: Partial<Record<string, TranslationKey>> = {
  runtime_desktop_runtime_required: "error.runtime_desktop_runtime_required",
  runtime_profile_account_required: "error.runtime_profile_account_required",
  auth_minecraft_session_missing: "error.auth_minecraft_session_missing",
  auth_relogin_required: "error.auth_relogin_required",
  auth_minecraft_ownership_missing: "error.auth_minecraft_ownership_missing",
  runtime_profile_already_running: "error.runtime_profile_already_running",
  runtime_repair_required: "error.runtime_repair_required",
  runtime_not_installed: "error.runtime_not_installed",
  runtime_java_requirement_mismatch: "error.runtime_java_requirement_mismatch",
  runtime_java_not_found: "error.runtime_java_not_found",
  runtime_java_major_mismatch: "error.runtime_java_major_mismatch",
  runtime_java_probe_failed: "error.runtime_java_probe_failed",
  runtime_java_probe_timeout: "error.runtime_java_probe_timeout",
  runtime_neoforge_installation_pipeline_unavailable:
    "error.runtime_neoforge_installation_pipeline_unavailable",
  component_provider_origin_unconfigured:
    "error.component_provider_origin_unconfigured",
  network_error: "error.network_error",
  runtime_worker_failed: "error.runtime_worker_failed",
};

function draftFromStatus(status: Phase5RuntimeStatus): RuntimeDraft {
  if (!status.runtime) {
    return {
      ...EMPTY_DRAFT,
      componentEnabled: status.component != null,
      componentId: status.component?.componentId ?? "",
      componentVersion: status.component?.componentVersion ?? "",
    };
  }
  return {
    minecraftVersion: status.runtime.minecraftVersion,
    loaderKind: status.runtime.loader.kind,
    loaderVersion: status.runtime.loader.loaderVersion ?? "",
    javaMode: status.runtime.java.mode,
    javaMajor: status.runtime.java.majorVersion,
    componentEnabled: status.component != null,
    componentId: status.component?.componentId ?? "",
    componentVersion: status.component?.componentVersion ?? "",
  };
}

function activeLaunch(status: Phase5RuntimeStatus | null): Phase5LaunchStatus | null {
  return status?.launches.find((launch) =>
    launch.state === "starting" || launch.state === "running" || launch.state === "stopping"
  ) ?? null;
}

function failedLaunch(status: Phase5RuntimeStatus | null): Phase5LaunchStatus | null {
  return status?.launches.find((launch) => launch.state === "failed") ?? null;
}

export function RuntimePanel({ profile }: { profile: Phase4Profile | null }) {
  const { t } = useI18n();
  const [effectiveProfile, setEffectiveProfile] = useState<Phase4Profile | null>(profile);
  const [activeAccountId, setActiveAccountId] = useState<string | null>(null);
  const [loadState, setLoadState] = useState<LoadState>("idle");
  const [catalogState, setCatalogState] = useState<LoadState>("loading");
  const [status, setStatus] = useState<Phase5RuntimeStatus | null>(null);
  const [catalog, setCatalog] = useState<Phase5RuntimeCatalog | null>(null);
  const [fabricCatalog, setFabricCatalog] = useState<Phase5RuntimeCatalog["fabricVersions"]>([]);
  const [componentCatalog, setComponentCatalog] = useState<Phase5ComponentCatalog | null>(null);
  const [componentCatalogState, setComponentCatalogState] = useState<LoadState>("idle");
  const [draft, setDraft] = useState<RuntimeDraft>(EMPTY_DRAFT);
  const [busy, setBusy] = useState<BusyAction>(null);
  const [standardSetupStage, setStandardSetupStage] = useState<StandardSetupStage>(null);
  const [standardContentIndex, setStandardContentIndex] = useState(0);
  const [standardElapsedSeconds, setStandardElapsedSeconds] = useState(0);
  const [actionError, setActionError] = useState<string | null>(null);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [liveMessage, setLiveMessage] = useState("");
  const [memoryMb, setMemoryMb] = useState(4096);
  const statusRequest = useRef(0);
  const componentCatalogRequest = useRef(0);

  useEffect(() => {
    if (busy !== "standard-installing") {
      setStandardElapsedSeconds(0);
      return undefined;
    }
    const startedAt = Date.now();
    const timer = window.setInterval(() => {
      setStandardElapsedSeconds(Math.floor((Date.now() - startedAt) / 1000));
    }, 1000);
    return () => window.clearInterval(timer);
  }, [busy]);

  const localizeError = useCallback((error: unknown, fallback: TranslationKey) => {
    const typed = typedIpcError(error);
    const knownKey = typed ? RUNTIME_ERROR_KEYS[typed.code] : undefined;
    if (knownKey) return t(knownKey, typed?.params);
    return typed
      ? t("runtime.error.code", { code: typed.code })
      : t(fallback);
  }, [t]);

  const refreshCatalog = useCallback(async () => {
    setCatalogState("loading");
    setCatalogError(null);
    try {
      const next = await runtimeCommands.catalog();
      if (next.minecraftVersions.length === 0) {
        throw new Error("runtime_minecraft_catalog_empty");
      }
      setCatalog(next);
      setCatalogState("ready");
    } catch (error) {
      setCatalog(null);
      setCatalogState("error");
      setCatalogError(localizeError(error, "runtime.error.catalog"));
    }
  }, [localizeError]);

  const refreshStatus = useCallback(async (
    profileId: string,
    showLoading = true,
  ): Promise<Phase5RuntimeStatus | null> => {
    const request = ++statusRequest.current;
    if (showLoading) setLoadState("loading");
    try {
      const next = await runtimeCommands.status(profileId);
      if (request !== statusRequest.current) return null;
      setStatus(next);
      setDraft((current) => {
        const resolved = draftFromStatus(next);
        return next.runtime || !current.minecraftVersion
          ? resolved
          : { ...resolved, minecraftVersion: current.minecraftVersion };
      });
      setLoadState("ready");
      return next;
    } catch (error) {
      if (request !== statusRequest.current) return null;
      setStatus(null);
      setLoadState("error");
      setActionError(localizeError(error, "runtime.error.status"));
      return null;
    }
  }, [localizeError]);

  useEffect(() => {
    setEffectiveProfile(profile);
  }, [profile]);

  useEffect(() => {
    void authCommands.snapshot().then((snapshot) => {
      setActiveAccountId(snapshot.activeAccountId);
    }).catch(() => setActiveAccountId(null));
  }, [profile?.id]);

  useEffect(() => {
    void refreshCatalog();
  }, [refreshCatalog]);

  useEffect(() => {
    setStatus(null);
    setDraft(EMPTY_DRAFT);
    setComponentCatalog(null);
    setComponentCatalogState("idle");
    componentCatalogRequest.current += 1;
    setBusy(null);
    setActionError(null);
    setLiveMessage("");
    if (!effectiveProfile) {
      statusRequest.current += 1;
      setLoadState("idle");
      return;
    }
    void refreshStatus(effectiveProfile.id);
  }, [effectiveProfile, refreshStatus]);

  useEffect(() => {
    if (draft.minecraftVersion || !catalog?.minecraftVersions.length) return;
    const defaultVersion = catalog.minecraftVersions.find((entry) =>
      entry.releaseType === "release"
    ) ?? catalog.minecraftVersions[0];
    setDraft((current) => ({
      ...current,
      minecraftVersion: defaultVersion.version,
    }));
  }, [catalog, draft.minecraftVersion]);

  useEffect(() => {
    let current = true;
    if (!draft.minecraftVersion) {
      setFabricCatalog([]);
      return () => {
        current = false;
      };
    }
    void runtimeCommands.catalog(draft.minecraftVersion)
      .then((next) => {
        if (!current) return;
        setFabricCatalog(draft.loaderKind === "fabric" ? next.fabricVersions : []);
        setCatalog((previous) => previous
          ? {
              ...previous,
              neoforgeCapability: next.neoforgeCapability,
              s9labComponentCapability: next.s9labComponentCapability,
              selectedMinecraftJavaMajor: next.selectedMinecraftJavaMajor,
            }
          : next);
        if (next.selectedMinecraftJavaMajor) {
          setDraft((previous) => ({
            ...previous,
            javaMajor: next.selectedMinecraftJavaMajor as RuntimeDraft["javaMajor"],
          }));
        }
      })
      .catch((error) => {
        if (!current) return;
        setFabricCatalog([]);
        setActionError(localizeError(error, "runtime.error.catalog"));
      });
    return () => {
      current = false;
    };
  }, [draft.loaderKind, draft.minecraftVersion, localizeError]);

  const launch = activeLaunch(status);
  const lastFailedLaunch = failedLaunch(status);
  useEffect(() => {
    if (!effectiveProfile || !launch) return;
    const interval = window.setInterval(() => {
      void refreshStatus(effectiveProfile.id, false);
    }, 2_000);
    return () => window.clearInterval(interval);
  }, [launch?.launchId, effectiveProfile, refreshStatus]);

  const providerComponentCapability = status?.s9labComponentCapability
    ?? catalog?.s9labComponentCapability
    ?? null;
  const componentCapability = componentCatalog?.capability
    ?? providerComponentCapability;
  const neoforgeCapability = catalog?.neoforgeCapability ?? null;
  const isComponentAvailable = componentCapability?.state === "available"
    && componentCapability.reasonCode === "";
  const isNeoforgeAvailable = neoforgeCapability?.state === "available";
  const isBusy = busy != null;
  const runtimeLocked = isBusy || launch != null;

  const componentCatalogIntent = useMemo<Phase5RuntimeIntent | null>(() => {
    if (
      providerComponentCapability?.state !== "available"
      || providerComponentCapability.reasonCode !== ""
      || !draft.minecraftVersion
      || (draft.loaderKind !== "vanilla" && !draft.loaderVersion)
    ) {
      return null;
    }
    return {
      minecraftVersion: draft.minecraftVersion,
      loader: draft.loaderKind === "vanilla"
        ? { kind: "vanilla" }
        : { kind: draft.loaderKind, loaderVersion: draft.loaderVersion },
      java: {
        mode: draft.javaMode,
        majorVersion: draft.javaMajor,
      },
    };
  }, [
    draft.javaMajor,
    draft.javaMode,
    draft.loaderKind,
    draft.loaderVersion,
    draft.minecraftVersion,
    providerComponentCapability?.reasonCode,
    providerComponentCapability?.state,
  ]);

  useEffect(() => {
    const request = ++componentCatalogRequest.current;
    if (!componentCatalogIntent) {
      setComponentCatalog(null);
      setComponentCatalogState("idle");
      return;
    }

    setComponentCatalog(null);
    setComponentCatalogState("loading");
    void runtimeCommands.componentCatalog(componentCatalogIntent)
      .then((next) => {
        if (request !== componentCatalogRequest.current) return;
        setComponentCatalog(next);
        setComponentCatalogState("ready");
        setDraft((current) => {
          const selected = next.entries.find((entry) =>
            entry.componentId === current.componentId
            && entry.componentVersion === current.componentVersion
          ) ?? next.entries[0];
          return selected
            ? {
                ...current,
                componentId: selected.componentId,
                componentVersion: selected.componentVersion,
              }
            : {
                ...current,
                componentEnabled: false,
                componentId: "",
                componentVersion: "",
              };
        });
      })
      .catch(() => {
        if (request !== componentCatalogRequest.current) return;
        setComponentCatalog(null);
        setComponentCatalogState("error");
        setDraft((current) => ({
          ...current,
          componentEnabled: false,
          componentId: "",
          componentVersion: "",
        }));
      });
  }, [componentCatalogIntent]);

  const componentEntries = componentCatalog?.entries ?? [];
  const componentIds = useMemo(
    () => [...new Set(componentEntries.map((entry) => entry.componentId))],
    [componentEntries],
  );
  const componentVersions = useMemo(
    () => componentEntries.filter((entry) => entry.componentId === draft.componentId),
    [componentEntries, draft.componentId],
  );

  const statusView = useMemo((): {
    label: string;
    tone: "neutral" | "success" | "warning" | "error" | "info";
  } => {
    if (loadState === "loading") return { label: t("runtime.status.loading"), tone: "info" };
    if (loadState === "error") return { label: t("runtime.status.error"), tone: "error" };
    if (busy === "installing") return { label: t("runtime.status.installing"), tone: "info" };
    if (busy === "repairing") return { label: t("runtime.status.repairing"), tone: "info" };
    if (busy === "launching" || launch?.state === "starting") {
      return { label: t("runtime.status.starting"), tone: "info" };
    }
    if (busy === "stopping" || launch?.state === "stopping") {
      return { label: t("runtime.status.stopping"), tone: "warning" };
    }
    if (launch?.state === "running") return { label: t("runtime.status.running"), tone: "success" };
    if (lastFailedLaunch) return { label: t("runtime.status.error"), tone: "error" };
    if (status?.installState === "installed") {
      return { label: t("runtime.status.installed"), tone: "success" };
    }
    if (status?.installState === "repair-required") {
      return { label: t("runtime.status.repairRequired"), tone: "warning" };
    }
    if (status?.installState === "configured") {
      return { label: t("runtime.status.configured"), tone: "info" };
    }
    return { label: t("runtime.status.notConfigured"), tone: "neutral" };
  }, [busy, lastFailedLaunch, launch?.state, loadState, status?.installState, t]);

  const capabilityDescription = useCallback((
    capability: Phase5CapabilityStatus | null,
    kind: "component" | "neoforge",
  ) => {
    if (capability?.state === "available") {
      if (kind === "component") {
        if (componentCatalogState === "loading") {
          return t("runtime.component.catalogLoading");
        }
        if (componentCatalogState === "error") {
          return t("runtime.component.catalogError");
        }
        if (componentCatalogState === "ready" && componentEntries.length === 0) {
          return t("runtime.component.catalogEmpty");
        }
      }
      return t("runtime.capability.available");
    }
    if (kind === "component") return t("runtime.component.providerUnconfigured");
    return t("runtime.loader.neoforgeUnavailable");
  }, [componentCatalogState, componentEntries.length, t]);

  const validateDraft = (): TranslationKey | null => {
    if (!draft.minecraftVersion) return "runtime.validation.version";
    if (draft.loaderKind !== "vanilla" && !draft.loaderVersion) {
      return "runtime.validation.loaderVersion";
    }
    if (
      draft.componentEnabled
      && !componentEntries.some((entry) =>
        entry.componentId === draft.componentId
        && entry.componentVersion === draft.componentVersion
      )
    ) {
      return "runtime.validation.component";
    }
    return null;
  };

  const buildIntent = (): Phase5RuntimeIntent => ({
    minecraftVersion: draft.minecraftVersion,
    loader: draft.loaderKind === "vanilla"
      ? { kind: "vanilla" }
      : { kind: draft.loaderKind, loaderVersion: draft.loaderVersion },
    java: {
      mode: draft.javaMode,
      majorVersion: draft.javaMajor,
    },
  });

  const patchDraft = (patch: Partial<RuntimeDraft>) => {
    setDraft((current) => ({ ...current, ...patch }));
  };

  const installRuntime = async (event: FormEvent) => {
    event.preventDefault();
    if (!effectiveProfile || runtimeLocked) return;
    const validationError = validateDraft();
    if (validationError) {
      setActionError(t(validationError));
      return;
    }
    setActionError(null);
    setBusy("installing");
    setLiveMessage(t("runtime.operation.installStarted"));
    try {
      await runtimeCommands.install(
        effectiveProfile.id,
        buildIntent(),
        draft.componentEnabled
          ? {
              mode: "catalog",
              componentId: draft.componentId,
              componentVersion: draft.componentVersion,
            }
          : { mode: "disabled" },
      );
      await refreshStatus(effectiveProfile.id, false);
      setLiveMessage(t("runtime.operation.installDone"));
    } catch (error) {
      setActionError(localizeError(error, "runtime.error.generic"));
      setLiveMessage(t("runtime.operation.failed"));
    } finally {
      setBusy(null);
    }
  };

  const installSNineStandard = async () => {
    if (!effectiveProfile || runtimeLocked) return;
    setActionError(null);
    setBusy("standard-installing");
    setStandardSetupStage("runtime");
    setStandardContentIndex(0);
    setLiveMessage(t("runtime.standard.installing"));
    try {
      const standardCatalog = await runtimeCommands.catalog(SNINE_STANDARD_MINECRAFT_VERSION);
      const fabric = standardCatalog.fabricVersions.find((entry) => entry.stable)
        ?? standardCatalog.fabricVersions[0];
      if (!fabric) {
        throw new Error(JSON.stringify({
          code: "runtime_loader_version_required",
          messageKey: "error.runtime_loader_version_required",
          params: {},
        }));
      }
      const javaMajor = standardCatalog.selectedMinecraftJavaMajor;
      if (!javaMajor) {
        throw new Error(JSON.stringify({
          code: "runtime_java_major_unsupported",
          messageKey: "error.runtime_java_major_unsupported",
          params: {},
        }));
      }
      const intent: Phase5RuntimeIntent = {
        minecraftVersion: SNINE_STANDARD_MINECRAFT_VERSION,
        loader: { kind: "fabric", loaderVersion: fabric.version },
        java: { mode: "system", majorVersion: javaMajor },
      };
      setDraft({
        ...EMPTY_DRAFT,
        minecraftVersion: intent.minecraftVersion,
        loaderKind: "fabric",
        loaderVersion: fabric.version,
        javaMajor,
      });
      await runtimeCommands.install(effectiveProfile.id, intent, { mode: "disabled" });
      setStandardSetupStage("content");
      for (const [index, projectId] of SNINE_STANDARD_MODRINTH_PROJECTS.entries()) {
        setStandardContentIndex(index + 1);
        await contentCommands.install(effectiveProfile.id, projectId, null);
      }
      await refreshStatus(effectiveProfile.id, false);
      setLiveMessage(t("runtime.standard.done"));
    } catch (error) {
      setActionError(localizeError(error, "runtime.error.generic"));
      setLiveMessage(t("runtime.operation.failed"));
    } finally {
      setBusy(null);
    }
  };

  const repairRuntime = async () => {
    if (!effectiveProfile || runtimeLocked) return;
    setActionError(null);
    setBusy("repairing");
    setLiveMessage(t("runtime.operation.repairStarted"));
    try {
      await runtimeCommands.repair(effectiveProfile.id);
      await refreshStatus(effectiveProfile.id, false);
      setLiveMessage(t("runtime.operation.repairDone"));
    } catch (error) {
      setActionError(localizeError(error, "runtime.error.generic"));
      setLiveMessage(t("runtime.operation.failed"));
    } finally {
      setBusy(null);
    }
  };

  const launchRuntime = async () => {
    if (!effectiveProfile || isBusy) return;
    setActionError(null);
    setBusy("launching");
    setLiveMessage(t("runtime.operation.launchStarted"));
    try {
      const nextLaunch = await runtimeCommands.launch(effectiveProfile.id, memoryMb);
      setStatus((current) => current
        ? {
            ...current,
            launches: [
              nextLaunch,
              ...current.launches.filter((item) => item.launchId !== nextLaunch.launchId),
            ],
          }
        : current);
      setLiveMessage(t("runtime.operation.launchDone"));
    } catch (error) {
      setActionError(localizeError(error, "runtime.error.generic"));
      setLiveMessage(t("runtime.operation.failed"));
    } finally {
      setBusy(null);
    }
  };

  const stopRuntime = async () => {
    if (!launch || isBusy) return;
    setActionError(null);
    setBusy("stopping");
    setLiveMessage(t("runtime.operation.stopStarted"));
    try {
      const stopped = await runtimeCommands.stop(launch.launchId);
      setStatus((current) => current
        ? {
            ...current,
            launches: [
              stopped,
              ...current.launches.filter((item) => item.launchId !== stopped.launchId),
            ],
          }
        : current);
      setLiveMessage(t("runtime.operation.stopDone"));
    } catch (error) {
      setActionError(localizeError(error, "runtime.error.generic"));
      setLiveMessage(t("runtime.operation.failed"));
    } finally {
      setBusy(null);
    }
  };

  const removeComponent = async () => {
    if (!effectiveProfile || runtimeLocked) return;
    setActionError(null);
    setBusy("component-changing");
    setLiveMessage(t("runtime.operation.componentStarted"));
    try {
      await runtimeCommands.setComponent(effectiveProfile.id, { mode: "disabled" });
      await refreshStatus(effectiveProfile.id, false);
      setLiveMessage(t("runtime.operation.componentDone"));
    } catch (error) {
      setActionError(localizeError(error, "runtime.error.generic"));
      setLiveMessage(t("runtime.operation.failed"));
    } finally {
      setBusy(null);
    }
  };

  const installedVersionMissing = Boolean(
    draft.minecraftVersion
    && !catalog?.minecraftVersions.some((entry) => entry.version === draft.minecraftVersion),
  );
  const installedFabricVersionMissing = Boolean(
    draft.loaderKind === "fabric"
    && draft.loaderVersion
    && !fabricCatalog.some((entry) => entry.version === draft.loaderVersion),
  );

  const assignActiveAccount = async () => {
    if (!effectiveProfile || !activeAccountId || isBusy) return;
    setActionError(null);
    setBusy("assigning-account");
    try {
      await authCommands.assignProfileAccount(effectiveProfile.id, activeAccountId);
      setEffectiveProfile((current) => current ? { ...current, accountId: activeAccountId } : current);
      await refreshStatus(effectiveProfile.id, false);
      setLiveMessage(t("runtime.account.assigned"));
    } catch (error) {
      setActionError(localizeError(error, "runtime.error.generic"));
      setLiveMessage(t("runtime.operation.failed"));
    } finally {
      setBusy(null);
    }
  };

  const setupForm = (
    <form className="runtime-setup" onSubmit={installRuntime}>
      {catalogState === "loading" ? (
        <div className="runtime-catalog-state" role="status">
          <RefreshCw className="ui-spin" aria-hidden="true" />
          <span>{t("runtime.catalog.loading")}</span>
        </div>
      ) : null}
      {catalogState === "error" ? (
        <div className="runtime-catalog-state runtime-catalog-state--error" role="alert">
          <ShieldAlert aria-hidden="true" />
          <span>{catalogError ?? t("runtime.error.catalog")}</span>
          <Button type="button" onClick={() => void refreshCatalog()}>
            <RefreshCw aria-hidden="true" />
            {t("app.retry")}
          </Button>
        </div>
      ) : null}
      <fieldset disabled={runtimeLocked || loadState === "loading"}>
        <legend>{t("runtime.setup.title")}</legend>
        <div className="runtime-setup__grid">
          <SelectField
            className="runtime-field--wide"
            label={t("runtime.minecraftVersion")}
            value={draft.minecraftVersion}
            disabled={catalogState !== "ready"}
            onChange={(event) => patchDraft({
              minecraftVersion: event.currentTarget.value,
              loaderVersion: "",
            })}
          >
            <option value="">{catalogState === "loading"
              ? t("runtime.catalog.loading")
              : t("runtime.catalog.selectVersion")}</option>
            {installedVersionMissing
              ? <option value={draft.minecraftVersion}>{t("runtime.catalog.installedVersion", { version: draft.minecraftVersion })}</option>
              : null}
            {catalog?.minecraftVersions.map((entry) => (
              <option key={entry.version} value={entry.version}>
                {entry.releaseType === "snapshot"
                  ? t("runtime.catalog.snapshot", { version: entry.version })
                  : entry.version}
              </option>
            ))}
          </SelectField>
          <SelectField
            label={t("runtime.loader")}
            value={draft.loaderKind}
            onChange={(event) => patchDraft({
              loaderKind: event.currentTarget.value as RuntimeDraft["loaderKind"],
              loaderVersion: "",
            })}
          >
            <option value="vanilla">{t("runtime.loader.vanilla")}</option>
            <option value="fabric">{t("runtime.loader.fabric")}</option>
            <option value="neoforge" disabled={!isNeoforgeAvailable}>
              {t("runtime.loader.neoforge")}
            </option>
          </SelectField>
          {draft.loaderKind === "fabric" ? (
            <SelectField
              label={t("runtime.loaderVersion")}
              value={draft.loaderVersion}
              disabled={!draft.minecraftVersion}
              onChange={(event) => patchDraft({
                loaderVersion: event.currentTarget.value,
              })}
            >
              <option value="">{t("runtime.catalog.selectLoader")}</option>
              {installedFabricVersionMissing
                ? <option value={draft.loaderVersion}>{t("runtime.catalog.installedVersion", { version: draft.loaderVersion })}</option>
                : null}
              {fabricCatalog.map((entry) => (
                <option key={entry.version} value={entry.version}>
                  {entry.stable
                    ? entry.version
                    : t("runtime.catalog.previewVersion", { version: entry.version })}
                </option>
              ))}
            </SelectField>
          ) : null}
          {draft.loaderKind === "neoforge" ? (
            <TextField
              label={t("runtime.loaderVersion")}
              value={draft.loaderVersion}
              autoComplete="off"
              onChange={(event) => patchDraft({
                loaderVersion: event.currentTarget.value,
              })}
            />
          ) : null}
          <SelectField
            label={t("runtime.javaMode")}
            value={draft.javaMode}
            onChange={(event) => patchDraft({
              javaMode: event.currentTarget.value as RuntimeDraft["javaMode"],
            })}
          >
            <option value="managed" disabled>{t("runtime.java.managedUnavailable")}</option>
            <option value="system">{t("runtime.java.system")}</option>
          </SelectField>
          <SelectField
            label={t("runtime.javaMajor")}
            value={draft.javaMajor}
            onChange={(event) => patchDraft({
              javaMajor: Number(event.currentTarget.value) as RuntimeDraft["javaMajor"],
            })}
          >
            <option value={21}>{t("runtime.java.version", { version: 21 })}</option>
            <option value={17}>{t("runtime.java.version", { version: 17 })}</option>
            <option value={25}>{t("runtime.java.version", { version: 25 })}</option>
          </SelectField>
        </div>
        <div className="runtime-capability-note">
          <ShieldAlert aria-hidden="true" />
          <span>{capabilityDescription(neoforgeCapability, "neoforge")}</span>
        </div>
        {isComponentAvailable || draft.componentEnabled ? <div className="runtime-component">
          <Switch
            label={t("runtime.component.optional")}
            description={capabilityDescription(componentCapability, "component")}
            checked={draft.componentEnabled}
            disabled={
              !isComponentAvailable
              || runtimeLocked
              || componentCatalogState !== "ready"
              || componentEntries.length === 0
            }
            onChange={(event) => {
              const enabled = event.currentTarget.checked;
              const selected = componentEntries.find((entry) =>
                entry.componentId === draft.componentId
                && entry.componentVersion === draft.componentVersion
              ) ?? componentEntries[0];
              patchDraft({
                componentEnabled: enabled,
                componentId: enabled ? selected?.componentId ?? "" : draft.componentId,
                componentVersion: enabled
                  ? selected?.componentVersion ?? ""
                  : draft.componentVersion,
              });
            }}
          />
          {draft.componentEnabled && isComponentAvailable ? (
            <div className="runtime-component__selection">
              <SelectField
                label={t("runtime.component.id")}
                value={draft.componentId}
                disabled={componentCatalogState !== "ready"}
                onChange={(event) => {
                  const componentId = event.currentTarget.value;
                  const selected = componentEntries.find((entry) =>
                    entry.componentId === componentId
                  );
                  patchDraft({
                    componentId,
                    componentVersion: selected?.componentVersion ?? "",
                  });
                }}
              >
                {componentIds.map((componentId) => (
                  <option key={componentId} value={componentId}>{componentId}</option>
                ))}
              </SelectField>
              <SelectField
                label={t("runtime.component.version")}
                value={draft.componentVersion}
                disabled={componentCatalogState !== "ready"}
                onChange={(event) => patchDraft({
                  componentVersion: event.currentTarget.value,
                })}
              >
                {componentVersions.map((entry) => (
                  <option
                    key={`${entry.componentId}:${entry.componentVersion}`}
                    value={entry.componentVersion}
                  >
                    {entry.componentVersion}
                  </option>
                ))}
              </SelectField>
            </div>
          ) : null}
        </div> : null}
      </fieldset>
      {status?.installState !== "installed" ? <div className="runtime-standard">
        <div>
          <strong>{t("runtime.standard.title")}</strong>
          <p>{t("runtime.standard.description")}</p>
        </div>
        <Button
          type="button"
          loading={busy === "standard-installing"}
          disabled={runtimeLocked}
          onClick={() => void installSNineStandard()}
        >
          <Download aria-hidden="true" />
          {t("runtime.standard.action")}
        </Button>
      </div> : null}
      {busy === "standard-installing" ? <div className="runtime-standard-progress" role="status">
        <RefreshCw className="ui-spin" aria-hidden="true" />
        <span>{standardSetupStage === "content"
          ? t("runtime.standard.progressContent", {
            current: standardContentIndex,
            total: SNINE_STANDARD_MODRINTH_PROJECTS.length,
            seconds: standardElapsedSeconds,
          })
          : t("runtime.standard.progressRuntime", { seconds: standardElapsedSeconds })}
        </span>
      </div> : null}
      <Button
        type="submit"
        variant="primary"
        loading={busy === "installing"}
        disabled={runtimeLocked || catalogState !== "ready"}
      >
        <Download aria-hidden="true" />
        {status?.installState === "installed"
          ? t("runtime.reinstall")
          : t("runtime.install")}
      </Button>
    </form>
  );

  return (
    <aside
      id="runtime-control"
      className={`home-panel home-status ${profile ? "home-status--selected" : ""}`}
      aria-busy={loadState === "loading" || isBusy}
    >
      <p className="sr-only" aria-live="polite" aria-atomic="true">{liveMessage}</p>
      <header>
        <h2>{t("home.statusTitle")}</h2>
        <Badge tone={statusView.tone}>{statusView.label}</Badge>
      </header>

      {!effectiveProfile ? (
        <div className="home-status__empty">
          <CircleOff aria-hidden="true" />
          <strong>{t("home.noSelection")}</strong>
          <p>{t("home.launchUnavailable")}</p>
        </div>
      ) : (
        <div className="home-status__selection">
          <ShieldCheck aria-hidden="true" />
          <span>
            <strong>{effectiveProfile.displayName}</strong>
            <small>{t("library.revision", { revision: effectiveProfile.activeRevisionId.slice(-8) })}</small>
          </span>
        </div>
      )}

      {effectiveProfile && loadState === "loading" ? (
        <div className="runtime-loading" role="status" aria-label={t("runtime.status.loading")}>
          <Skeleton height="2.5rem" />
          <Skeleton height="5rem" />
          <Skeleton height="2.75rem" />
        </div>
      ) : null}

      {effectiveProfile && loadState === "error" ? (
        <div className="runtime-inline-error" role="alert">
          <ShieldAlert aria-hidden="true" />
          <div>
            <strong>{t("runtime.status.error")}</strong>
            <p>{actionError ?? t("runtime.error.status")}</p>
          </div>
          <Button onClick={() => void refreshStatus(effectiveProfile.id)}>
            <RefreshCw aria-hidden="true" />
            {t("app.retry")}
          </Button>
        </div>
      ) : null}

      {effectiveProfile && loadState === "ready" && status ? (
        <>
          <dl className="runtime-summary">
            {status.component || isComponentAvailable ? <div>
              <dt>{t("home.statusMinecraft")}</dt>
              <dd>{status.runtime?.minecraftVersion ?? t("home.notAvailable")}</dd>
            </div> : null}
            <div>
              <dt>{t("home.statusLoader")}</dt>
              <dd>{status.runtime
                ? t(`runtime.loader.${status.runtime.loader.kind}` as TranslationKey)
                : t("home.notAvailable")}</dd>
            </div>
            <div>
              <dt>{t("runtime.javaMajor")}</dt>
              <dd>{status.runtime
                ? t("runtime.java.version", { version: status.runtime.java.majorVersion })
                : t("home.notAvailable")}</dd>
            </div>
            {status.component || isComponentAvailable ? <div>
              <dt>{t("runtime.component.title")}</dt>
              <dd>{status.component
                ? t("runtime.component.current", {
                    id: status.component.componentId,
                    version: status.component.componentVersion,
                })
                : t("runtime.component.none")}</dd>
            </div> : null}
          </dl>

          {status.component ? (
            <div className="runtime-component-current">
              <Box aria-hidden="true" />
              <span>
                <strong>{status.component.componentId}</strong>
                <small>{status.component.componentVersion}</small>
              </span>
              <Button
                variant="ghost"
                loading={busy === "component-changing"}
                disabled={runtimeLocked}
                onClick={() => void removeComponent()}
              >
                {t("runtime.component.remove")}
              </Button>
            </div>
          ) : null}

          {status.installState === "not-configured" || status.installState === "configured"
            ? setupForm
            : (
                <details className="runtime-settings">
                  <summary>{t("runtime.setup.edit")}</summary>
                  {setupForm}
                </details>
              )}

          {status.installState === "repair-required" ? (
            <Status tone="warning" label={t("status.warning")}>
              {t("runtime.repairDescription")}
            </Status>
          ) : null}

          {!effectiveProfile.accountId && status.installState === "installed" ? (
            <Status tone="warning" label={t("runtime.account.requiredTitle")}>
              <div className="runtime-account-assignment">
                <span>{t("runtime.account.required")}</span>
                <Button
                  type="button"
                  variant="secondary"
                  loading={busy === "assigning-account"}
                  disabled={isBusy || !activeAccountId}
                  onClick={() => void assignActiveAccount()}
                >
                  {t("runtime.account.useActive")}
                </Button>
              </div>
            </Status>
          ) : null}

          {launch ? (
            <div className="runtime-running" role="status">
              <span className="runtime-running__pulse" aria-hidden="true" />
              <div>
                <strong>{statusView.label}</strong>
                <small>{t("runtime.runningAs", { account: launch.accountName })}</small>
              </div>
            </div>
          ) : null}

          {lastFailedLaunch && !launch ? (
            <Status tone="error" label={t("status.error")}>
              {t("runtime.launchFailed")}
            </Status>
          ) : null}

          {actionError ? (
            <div className="runtime-action-error" role="alert">
              <ShieldAlert aria-hidden="true" />
              <span>{actionError}</span>
            </div>
          ) : null}

          <div className="runtime-launch">
            {status.installState === "repair-required" ? (
              <Button
                variant="primary"
                loading={busy === "repairing"}
                disabled={isBusy}
                onClick={() => void repairRuntime()}
              >
                <Wrench aria-hidden="true" />
                {t("runtime.repair")}
              </Button>
            ) : launch ? (
              <Button
                variant="danger"
                loading={busy === "stopping"}
                disabled={isBusy || launch.state === "stopping"}
                onClick={() => void stopRuntime()}
              >
                <Square aria-hidden="true" />
                {t("runtime.stop")}
              </Button>
            ) : (
              <>
                <SelectField
                  label={t("runtime.memory")}
                  value={memoryMb}
                  disabled={isBusy}
                  onChange={(event) => setMemoryMb(Number(event.currentTarget.value))}
                >
                  {[2048, 4096, 6144, 8192].map((value) => (
                    <option key={value} value={value}>
                      {t("runtime.memoryValue", { memory: value / 1024 })}
                    </option>
                  ))}
                </SelectField>
                <Button
                  variant="primary"
                  loading={busy === "launching"}
                  disabled={
                    isBusy
                    || status.installState !== "installed"
                    || !effectiveProfile.accountId
                  }
                  onClick={() => void launchRuntime()}
                >
                  <Play aria-hidden="true" />
                  {t("runtime.launch")}
                </Button>
              </>
            )}
          </div>
        </>
      ) : null}
    </aside>
  );
}
