import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  Check,
  ChevronDown,
  Download,
  ExternalLink,
  LoaderCircle,
  LogIn,
  Play,
  Plus,
  X,
} from "lucide-react";
import { authCommands, openMicrosoftVerification } from "../lib/authCommands";
import { profileCommands } from "../lib/profileCommands";
import { runtimeCommands } from "../lib/runtimeCommands";
import { snineClientUpdate, type SnineClientDownloadProgress } from "../lib/snineClientUpdate";
import type {
  Phase3Account,
  Phase3DeviceLoginPrompt,
  Phase4Profile,
  Phase5LaunchStatus,
} from "../lib/generated/ipc-contracts";
import { useWorkspaceStore } from "../app/workspaceStore";
import { useShellStore } from "../app/shellStore";
import { useI18n } from "../i18n/I18nProvider";
import { LauncherSkinPreview } from "../components/player/LauncherSkinPreview";
import { MinecraftAvatar } from "../components/player/MinecraftAvatar";
import {
  launcherBadgeIconUrl,
  loadSNineLauncherCosmetics,
  pollSNineLauncherLiveState,
  resolveSNineLiveCosmetics,
  type LauncherCosmeticSnapshot,
} from "../lib/snineClientBridge";
import { loadLauncherPreferences } from "../theme/launcherPreferences";
import {
  didLaunchCrash,
  isLaunchActive,
  isLaunchLocked,
  latestLaunchForProfile,
  launchButtonKey,
  tryAcquireLaunchGuard,
  type LaunchUiState,
} from "../lib/launchLifecycle";

const EMPTY_COSMETICS: LauncherCosmeticSnapshot = {
  ok: false,
  playerName: "SNine",
  online: false,
  badgeIcon: "",
  plusActive: false,
  equipped: [],
  source: "",
  statusMessage: "not_connected",
  liveSync: null,
};

type DownloadStage = "downloading" | "verifying" | "complete" | "error";

interface DownloadDialogState {
  visible: boolean;
  stage: DownloadStage;
  percent: number;
  downloadedBytes: number;
  totalBytes: number | null;
  version: string | null;
  error: string;
}

const HIDDEN_DOWNLOAD: DownloadDialogState = {
  visible: false,
  stage: "downloading",
  percent: 0,
  downloadedBytes: 0,
  totalBytes: null,
  version: null,
  error: "",
};

function equippedSignatureFromAssets(snapshot: LauncherCosmeticSnapshot): string {
  return snapshot.equipped
    .map((asset) => `${asset.kind.toLowerCase()}:${asset.id}`)
    .sort()
    .join("|");
}

function equippedSignatureFromMap(map: Record<string, string>): string {
  return Object.entries(map)
    .filter(([, id]) => Boolean(id?.trim()))
    .map(([kind, id]) => `${kind.toLowerCase()}:${id.trim()}`)
    .sort()
    .join("|");
}

function readableLaunchError(error: unknown, t: (key: any, params?: Record<string, string | number>) => string): string {
  const record = error && typeof error === "object" ? error as Record<string, unknown> : null;
  const text = error instanceof Error
    ? error.message
    : typeof record?.code === "string"
      ? record.code
      : typeof record?.message === "string"
        ? record.message
        : String(error ?? "");
  if (text.includes("runtime_not_installed")) return t("launcher.error.runtimeNotInstalled");
  if (text.includes("runtime_repair_required")) return t("launcher.error.runtimeRepair");
  if (text.includes("runtime_profile_account_required")) return t("launcher.error.profileAccount");
  if (text.includes("auth_relogin_required")) return t("launcher.error.relogin");
  if (text.includes("snine_update_")) {
    const compact = text.length > 260 ? `${text.slice(0, 257)}...` : text;
    return `${t("launcher.error.updateDownload")} (${compact})`;
  }
  if (text.includes("snine_client_mod_id_invalid")) return t("launcher.error.invalidClient");
  return text && text !== "[object Object]" ? text : t("launcher.error.launch");
}

function formatBytes(value: number | null): string {
  if (!value || value <= 0) return "";
  if (value < 1024 * 1024) return `${Math.round(value / 1024)} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

export function HomePage() {
  const { t, locale } = useI18n();
  const [profiles, setProfiles] = useState<Phase4Profile[]>([]);
  const [accounts, setAccounts] = useState<Phase3Account[]>([]);
  const [activeAccount, setActiveAccount] = useState<Phase3Account | null>(null);
  const [loading, setLoading] = useState(true);
  const [launchState, setLaunchState] = useState<LaunchUiState>("idle");
  const [launchError, setLaunchError] = useState("");
  const [cosmetics, setCosmetics] = useState<LauncherCosmeticSnapshot>(EMPTY_COSMETICS);
  const [accountMenuOpen, setAccountMenuOpen] = useState(false);
  const [accountSwitching, setAccountSwitching] = useState(false);
  const [loginPrompt, setLoginPrompt] = useState<Phase3DeviceLoginPrompt | null>(null);
  const [loginBusy, setLoginBusy] = useState<"start" | "complete" | null>(null);
  const [loginError, setLoginError] = useState("");
  const [downloadDialog, setDownloadDialog] = useState<DownloadDialogState>(HIDDEN_DOWNLOAD);
  const [clientVersion, setClientVersion] = useState<string | null>(null);
  const [activeLaunch, setActiveLaunch] = useState<Phase5LaunchStatus | null>(null);
  const [playerRenderRevision, setPlayerRenderRevision] = useState(0);
  const [launcherPreferences] = useState(loadLauncherPreferences);

  const selectedProfileId = useWorkspaceStore((state) => state.selectedProfileId);
  const selectProfile = useWorkspaceStore((state) => state.selectProfile);
  const reconcileProfiles = useWorkspaceStore((state) => state.reconcileProfiles);
  const reducedMotion = useShellStore((state) => state.settings.reducedMotion);
  const pushToast = useShellStore((state) => state.pushToast);

  const cosmeticSignatureRef = useRef("");
  const cosmeticSyncEpochRef = useRef(0);
  const currentPlayerAccountIdRef = useRef<string | null>(null);
  const pollBusyRef = useRef(false);
  const closeAfterLaunchIssuedRef = useRef(false);
  const launchInFlightRef = useRef(false);
  const selectedProfileIdRef = useRef<string | null>(null);
  const lastLaunchTransitionRef = useRef("");
  const accountMenuRef = useRef<HTMLDivElement>(null);

  const loadBase = useCallback(async () => {
    setLoading(true);
    try {
      const [items, auth] = await Promise.all([profileCommands.list(), authCommands.snapshot()]);
      const activeProfiles = items.filter((profile) => profile.lifecycleState === "active");
      setProfiles(activeProfiles);
      setAccounts(auth.accounts);
      setActiveAccount(auth.accounts.find((account) => account.id === auth.activeAccountId) ?? null);
      reconcileProfiles(activeProfiles.map((profile) => profile.id));
      if (!selectedProfileId && activeProfiles[0]) selectProfile(activeProfiles[0].id);
    } catch (error) {
      console.error("[SNine Launcher] Home bootstrap failed", error);
      setProfiles([]);
      setAccounts([]);
      setActiveAccount(null);
      reconcileProfiles([]);
    } finally {
      setLoading(false);
    }
  }, [reconcileProfiles, selectProfile, selectedProfileId]);

  useEffect(() => { void loadBase(); }, [loadBase]);

  const selectedProfile = useMemo(
    () => profiles.find((profile) => profile.id === selectedProfileId) ?? profiles[0] ?? null,
    [profiles, selectedProfileId],
  );

  useEffect(() => {
    selectedProfileIdRef.current = selectedProfile?.id ?? null;
  }, [selectedProfile?.id]);

  const playerAccount = useMemo(() => (
    (selectedProfile?.accountId
      ? accounts.find((account) => account.id === selectedProfile.accountId)
      : null) ?? activeAccount
  ), [accounts, activeAccount, selectedProfile?.accountId]);

  const playerName = playerAccount?.username ?? "SNINE";

  const applyLaunchStatus = useCallback((status: Phase5LaunchStatus) => {
    if (status.profileId !== selectedProfileIdRef.current) return;
    setActiveLaunch(status);
    const transition = `${status.launchId}:${status.state}`;
    const shouldNotify = lastLaunchTransitionRef.current !== transition;
    lastLaunchTransitionRef.current = transition;

    if (status.state === "failed") {
      launchInFlightRef.current = false;
      setLaunchState("failed");
      setLaunchError(status.exitCode == null
        ? t("launcher.error.launch")
        : `${t("launcher.error.launch")} (Exit Code ${status.exitCode})`);
      if (shouldNotify) pushToast("error", status.exitCode == null ? "launcher.toast.startFailed" : "launcher.toast.crashed");
      return;
    }
    if (status.state === "exited") {
      launchInFlightRef.current = false;
      setLaunchState("idle");
      if (didLaunchCrash(status)) {
        setLaunchError(`${t("launcher.error.launch")} (Exit Code ${status.exitCode ?? "?"})`);
        if (shouldNotify) pushToast("error", "launcher.toast.crashed");
      } else {
        setLaunchError("");
        if (shouldNotify) pushToast("info", "launcher.toast.ended");
      }
      return;
    }

    setLaunchState(status.state);
    setLaunchError("");
    if (shouldNotify && status.state === "preparing") pushToast("info", "launcher.toast.preparing");
    if (shouldNotify && status.state === "running") pushToast("success", "launcher.toast.started");
  }, [pushToast, t]);

  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<Phase5LaunchStatus>("minecraft-launch-status", (event) => {
      applyLaunchStatus(event.payload);
    }).then((dispose) => {
      if (disposed) dispose();
      else unlisten = dispose;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [applyLaunchStatus]);

  useEffect(() => {
    if (!accountMenuOpen) return;
    const close = (event: PointerEvent) => {
      if (!accountMenuRef.current?.contains(event.target as Node)) setAccountMenuOpen(false);
    };
    window.addEventListener("pointerdown", close);
    return () => window.removeEventListener("pointerdown", close);
  }, [accountMenuOpen]);

  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<SnineClientDownloadProgress>("snine-client-download-progress", (event) => {
      const progress = event.payload;
      setDownloadDialog((current) => ({
        ...current,
        visible: true,
        stage: progress.stage === "complete" ? "complete" : progress.stage === "verifying" ? "verifying" : "downloading",
        percent: progress.stage === "complete" ? 100 : Math.max(current.percent, Math.min(100, progress.percent || 0)),
        downloadedBytes: progress.downloadedBytes,
        totalBytes: progress.totalBytes,
        error: "",
      }));
    }).then((dispose) => {
      if (disposed) dispose();
      else unlisten = dispose;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const switchAccount = useCallback(async (account: Phase3Account) => {
    if (accountSwitching || playerAccount?.id === account.id) {
      setAccountMenuOpen(false);
      return;
    }

    // The account switch owns a fresh renderer/sync generation. In-flight work from
    // the previous UUID is invalid immediately, before any async auth IPC can finish.
    ++cosmeticSyncEpochRef.current;
    currentPlayerAccountIdRef.current = account.id;
    pollBusyRef.current = false;
    cosmeticSignatureRef.current = "";
    setCosmetics(EMPTY_COSMETICS);
    setAccountSwitching(true);
    setLaunchError("");

    try {
      const selected = await authCommands.selectAccount(account.id);
      if (selectedProfile) {
        await authCommands.assignProfileAccount(selectedProfile.id, selected.id);
        setProfiles((current) => current.map((profile) => (
          profile.id === selectedProfile.id ? { ...profile, accountId: selected.id } : profile
        )));
      }

      setAccounts((current) => current.map((entry) => entry.id === selected.id ? selected : entry));
      setActiveAccount(selected);
      currentPlayerAccountIdRef.current = selected.id;
      setAccountMenuOpen(false);
      console.info("[SNine Launcher] Player account switched", { accountId: selected.id, username: selected.username });
    } catch (error) {
      console.error("[SNine Launcher] Account switch failed", error);
      setLaunchError(t("launcher.error.accountSwitch"));
      // Restore the currently rendered account identity if selection failed.
      currentPlayerAccountIdRef.current = playerAccount?.id ?? null;
    } finally {
      setAccountSwitching(false);
    }
  }, [accountSwitching, playerAccount?.id, selectedProfile, t]);

  const startAccountLogin = useCallback(async () => {
    if (loginBusy) return;
    setLoginBusy("start");
    setLoginError("");
    try {
      const prompt = await authCommands.startDeviceLogin(locale);
      setLoginPrompt(prompt);
      setAccountMenuOpen(false);
      await openMicrosoftVerification(prompt.verificationUri).catch((error) => {
        console.warn("[SNine Launcher] Microsoft verification page could not be opened", error);
      });
    } catch (error) {
      console.error("[SNine Launcher] Account login start failed", error);
      setLoginError(t("launcher.error.loginStart"));
    } finally {
      setLoginBusy(null);
    }
  }, [locale, loginBusy, t]);

  const cancelAccountLogin = useCallback(async () => {
    const prompt = loginPrompt;
    setLoginPrompt(null);
    setLoginError("");
    if (prompt) await authCommands.cancelDeviceLogin(prompt.loginId).catch(() => undefined);
  }, [loginPrompt]);

  const completeAccountLogin = useCallback(async () => {
    if (!loginPrompt || loginBusy) return;
    setLoginBusy("complete");
    setLoginError("");
    try {
      const added = await authCommands.completeDeviceLogin(loginPrompt.loginId);
      const selected = await authCommands.selectAccount(added.id);
      if (selectedProfile) await authCommands.assignProfileAccount(selectedProfile.id, added.id);
      setLoginPrompt(null);
      ++cosmeticSyncEpochRef.current;
      currentPlayerAccountIdRef.current = selected.id;
      pollBusyRef.current = false;
      cosmeticSignatureRef.current = "";
      setCosmetics(EMPTY_COSMETICS);
      setActiveAccount(selected);
      await loadBase();
    } catch (error) {
      console.error("[SNine Launcher] Account login completion failed", error);
      setLoginError(t("launcher.error.loginComplete"));
    } finally {
      setLoginBusy(null);
    }
  }, [loadBase, loginBusy, loginPrompt, selectedProfile, t]);

  const handshakeCosmetics = useCallback(async () => {
    if (!playerAccount) {
      ++cosmeticSyncEpochRef.current;
      setCosmetics(EMPTY_COSMETICS);
      cosmeticSignatureRef.current = "";
      return;
    }

    const expectedAccountId = playerAccount.id;
    const expectedProfileId = selectedProfile?.id ?? null;
    const syncEpoch = cosmeticSyncEpochRef.current;
    try {
      const snapshot = await loadSNineLauncherCosmetics(
        expectedAccountId,
        playerAccount.username,
        expectedProfileId,
        launcherPreferences.showPreviewCosmetics,
      );
      if (syncEpoch !== cosmeticSyncEpochRef.current || currentPlayerAccountIdRef.current !== expectedAccountId) return;
      cosmeticSignatureRef.current = equippedSignatureFromAssets(snapshot);
      setCosmetics(snapshot);
    } catch (error) {
      if (syncEpoch === cosmeticSyncEpochRef.current && currentPlayerAccountIdRef.current === expectedAccountId) {
        console.warn("[SNine Launcher] Initial cosmetic sync failed", error);
      }
    }
  }, [launcherPreferences.showPreviewCosmetics, playerAccount, selectedProfile?.id]);

  useEffect(() => {
    // Every account identity owns its own skin + cosmetic lifecycle.
    // Reset first so neither the renderer nor the UI can show the previous account's loadout.
    currentPlayerAccountIdRef.current = playerAccount?.id ?? null;
    ++cosmeticSyncEpochRef.current;
    pollBusyRef.current = false;
    cosmeticSignatureRef.current = "";
    setCosmetics(EMPTY_COSMETICS);
    setPlayerRenderRevision((revision) => revision + 1);

    const timer = window.setTimeout(() => { void handshakeCosmetics(); }, 0);
    return () => window.clearTimeout(timer);
  }, [playerAccount?.id, playerAccount?.username, selectedProfile?.id]);

  useEffect(() => {
    const refreshCape = () => { void handshakeCosmetics(); };
    window.addEventListener("snine-cape-selection-changed", refreshCape);
    window.addEventListener("storage", refreshCape);
    return () => {
      window.removeEventListener("snine-cape-selection-changed", refreshCape);
      window.removeEventListener("storage", refreshCape);
    };
  }, [handshakeCosmetics]);

  // The game client owns the per-player WebSocket. The launcher mirrors the authenticated
  // profile once per second so the selected F5/nametag icon updates immediately too. Full
  // cosmetic assets are resolved only when their preview setting is enabled.
  useEffect(() => {
    if (!playerAccount) return;
    const live = cosmetics.liveSync;

    if (!live?.accountId || !live.username) {
      const reconnect = window.setInterval(() => { void handshakeCosmetics(); }, 5_000);
      return () => window.clearInterval(reconnect);
    }

    let disposed = false;
    let consecutiveFailures = 0;
    const syncEpoch = cosmeticSyncEpochRef.current;
    const expectedAccountId = playerAccount.id;

    const poll = async () => {
      if (disposed || pollBusyRef.current) return;
      pollBusyRef.current = true;
      try {
        const state = await pollSNineLauncherLiveState(live);
        if (disposed || syncEpoch !== cosmeticSyncEpochRef.current || currentPlayerAccountIdRef.current !== expectedAccountId) return;
        consecutiveFailures = 0;
        const nextSignature = equippedSignatureFromMap(state.equippedCosmetics);

        if (nextSignature !== cosmeticSignatureRef.current) {
          const assets = launcherPreferences.showPreviewCosmetics
            ? await resolveSNineLiveCosmetics(state.equippedCosmetics, selectedProfile?.id)
            : [];
          if (disposed || syncEpoch !== cosmeticSyncEpochRef.current || currentPlayerAccountIdRef.current !== expectedAccountId) return;
          cosmeticSignatureRef.current = nextSignature;
          setCosmetics((current) => ({
            ...current,
            ok: true,
            online: state.online,
            badgeIcon: state.badgeIcon,
            plusActive: state.plusActive,
            equipped: assets,
            source: "snine-live-profile-sync",
            statusMessage: "live_loadout_synced",
            liveSync: live,
          }));
        } else {
          setCosmetics((current) => ({
            ...current,
            ok: true,
            online: state.online,
            badgeIcon: state.badgeIcon,
            plusActive: state.plusActive,
            source: "snine-live-profile-sync",
            statusMessage: "live_loadout_synced",
            liveSync: live,
          }));
        }
      } catch (error) {
        if (disposed || syncEpoch !== cosmeticSyncEpochRef.current || currentPlayerAccountIdRef.current !== expectedAccountId) return;
        consecutiveFailures += 1;
        console.warn("[SNine Launcher] Live cosmetic poll failed", error);
        if (consecutiveFailures >= 2) {
          consecutiveFailures = 0;
          void handshakeCosmetics();
        }
      } finally {
        pollBusyRef.current = false;
      }
    };

    void poll();
    const timer = window.setInterval(() => { void poll(); }, 1_000);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [cosmetics.liveSync?.accountId, cosmetics.liveSync?.username, handshakeCosmetics, launcherPreferences.showPreviewCosmetics, playerAccount?.id, playerAccount?.username, selectedProfile?.id]);

  useEffect(() => {
    if (launchState !== "running" || !launcherPreferences.closeOnLaunch || closeAfterLaunchIssuedRef.current) return;
    closeAfterLaunchIssuedRef.current = true;
    if ("__TAURI_INTERNALS__" in window) {
      void invoke("window_close").catch((error) => {
        closeAfterLaunchIssuedRef.current = false;
        console.warn("[SNine Launcher] Close-after-launch failed", error);
      });
    }
  }, [launchState, launcherPreferences.closeOnLaunch]);

  useEffect(() => {
    const profileId = selectedProfile?.id;
    if (!profileId) {
      setClientVersion(null);
      return;
    }

    let disposed = false;
    void snineClientUpdate.check(profileId).then(async (result) => {
      if (disposed) return;
      setClientVersion(result.installedVersion);

      // Warm the newest SNine build while the user is still on the Home screen.
      // Normal Play clicks therefore do not have to spend time downloading an
      // update that the launcher already knew about.
      if (result.reachable && result.updateAvailable) {
        console.info("[SNine Launcher] New SNine Client build detected; preloading update");
        try {
          const downloaded = await snineClientUpdate.download(profileId);
          if (!disposed) setClientVersion(downloaded.installedVersion);
        } catch (error) {
          if (!disposed) console.warn("[SNine Launcher] Background client preload failed", error);
        }
      }
    }).catch((error) => {
      if (!disposed) {
        setClientVersion(null);
        console.warn("[SNine Launcher] SNine Client version check failed", error);
      }
    });
    return () => { disposed = true; };
  }, [selectedProfile?.id]);

  useEffect(() => {
    if (!selectedProfile) return;
    let disposed = false;
    const update = async () => {
      try {
        const statuses = await runtimeCommands.launchStatuses();
        if (disposed) return;
        const latest = latestLaunchForProfile(statuses, selectedProfile.id);
        if (latest) applyLaunchStatus(latest);
      } catch {
        // Non-critical on the one-page launcher.
      }
    };
    void update();
    const timer = window.setInterval(() => { void update(); }, 2_000);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [applyLaunchStatus, selectedProfile]);


  const launch = useCallback(async () => {
    if (!playerAccount || isLaunchLocked(launchState) || !tryAcquireLaunchGuard(launchInFlightRef)) return;
    setLaunchError("");
    setLaunchState("preparing");
    closeAfterLaunchIssuedRef.current = false;
    try {
      let profile = selectedProfile;

      // The one-button launcher must work on a fresh install. If the user has a
      // Minecraft account but no active SNine profile yet, create the profile on
      // the first Start click, bind the selected account, and continue with the
      // normal verified SNine runtime/install pipeline immediately.
      if (!profile) {
        const created = await profileCommands.create("SNine Client");
        await authCommands.assignProfileAccount(created.id, playerAccount.id);
        profile = { ...created, accountId: playerAccount.id };
        setProfiles((current) => {
          const withoutDuplicate = current.filter((entry) => entry.id !== profile!.id);
          return [profile!, ...withoutDuplicate];
        });
        reconcileProfiles([profile.id]);
        selectProfile(profile.id);
        selectedProfileIdRef.current = profile.id;
        console.info("[SNine Launcher] Created default SNine profile", {
          profileId: profile.id,
          accountId: playerAccount.id,
        });
      }

      const result = await runtimeCommands.launch(profile.id, 4096);
      applyLaunchStatus(result);
      setDownloadDialog((current) => current.stage === "error" ? current : HIDDEN_DOWNLOAD);
      void snineClientUpdate.check(profile.id).then((status) => setClientVersion(status.installedVersion)).catch(() => undefined);
    } catch (error) {
      console.error("[SNine Launcher] Launch failed", error);
      const message = readableLaunchError(error, t);
      setLaunchError(message);
      setDownloadDialog((current) => current.visible ? { ...current, stage: "error", error: message } : current);
      setLaunchState("failed");
      if (!lastLaunchTransitionRef.current.endsWith(":failed")) {
        pushToast("error", "launcher.toast.startFailed");
      }
    } finally {
      launchInFlightRef.current = false;
    }
  }, [applyLaunchStatus, launchState, playerAccount, pushToast, reconcileProfiles, selectProfile, selectedProfile, t]);

  const canLaunch = Boolean(playerAccount) && !loading && !accountSwitching;

  const launchLocked = isLaunchLocked(launchState);
  const badgeIconUrl = launcherBadgeIconUrl(cosmetics.badgeIcon, cosmetics.plusActive);
  const launchLabel = launchState === "downloading"
    && downloadDialog.visible
    && downloadDialog.totalBytes != null
    && downloadDialog.totalBytes > 0
    ? `${t(launchButtonKey(launchState))} ${Math.round(downloadDialog.percent)}%`
    : t(launchButtonKey(launchState));


  return (
    <div className="snine-one-home">
      <header className="snine-one-home__header">
        <div className="snine-one-home__header-actions">
          <div className="snine-account-switcher" ref={accountMenuRef}>
            <button
              type="button"
              className="snine-account-switcher__button"
              onClick={() => setAccountMenuOpen((open) => !open)}
              disabled={accountSwitching || launchLocked}
              aria-expanded={accountMenuOpen}
            >
              {accountSwitching ? <LoaderCircle className="ui-spin" aria-hidden="true" /> : (
                <MinecraftAvatar accountId={playerAccount?.id} username={playerAccount?.username} className="snine-account-switcher__avatar" />
              )}
              <span>{playerAccount ? playerName : t("launcher.home.accountFallback")}</span>
              <ChevronDown aria-hidden="true" />
            </button>
            {accountMenuOpen ? (
              <div className="snine-account-switcher__menu" role="menu">
                <small>{t("launcher.home.accountsTitle")}</small>
                {accounts.map((account) => (
                  <button
                    type="button"
                    key={account.id}
                    className={account.id === playerAccount?.id ? "is-selected" : ""}
                    onClick={() => void switchAccount(account)}
                    disabled={accountSwitching || launchLocked}
                    role="menuitem"
                  >
                    <MinecraftAvatar accountId={account.id} username={account.username} className="snine-account-switcher__avatar" />
                    <span>
                      <strong>{account.username}</strong>
                      <small>{account.sessionState === "active" ? t("launcher.home.microsoftConnected") : t("launcher.home.relogin")}</small>
                    </span>
                    {account.id === playerAccount?.id ? <Check aria-hidden="true" /> : null}
                  </button>
                ))}
                <button
                  type="button"
                  className="snine-account-switcher__add"
                  onClick={() => void startAccountLogin()}
                  disabled={loginBusy === "start" || launchLocked}
                  role="menuitem"
                >
                  {loginBusy === "start" ? <LoaderCircle className="ui-spin" aria-hidden="true" /> : <Plus aria-hidden="true" />}
                  <span>
                    <strong>{t("launcher.home.addAccount")}</strong>
                    <small>{t("launcher.home.signInMicrosoft")}</small>
                  </span>
                </button>
              </div>
            ) : null}
          </div>
        </div>
      </header>

      <section className="snine-one-home__player-stage" aria-label={`${playerName} player preview`}>
        <LauncherSkinPreview
          key={`${playerAccount?.id ?? "no-account"}:${playerRenderRevision}`}
          accountId={playerAccount?.id}
          playerName={playerName}
          reducedMotion={reducedMotion || !launcherPreferences.previewAnimations}
          cosmetics={launcherPreferences.showPreviewCosmetics ? cosmetics.equipped : []}
          badgeIconUrl={badgeIconUrl}
        />
      </section>


      <footer className="snine-one-home__launch-area">
        <button
          type="button"
          className={`snine-one-home__launch ${launchLocked ? "is-locked" : ""}`}
          onClick={() => void launch()}
          disabled={!canLaunch || launchLocked}
        >
          <span className="snine-one-home__launch-icon">
            {launchState === "running" ? <Check aria-hidden="true" /> : launchLocked ? <LoaderCircle className="ui-spin" aria-hidden="true" /> : <Play aria-hidden="true" fill="currentColor" />}
          </span>
          <span className="snine-one-home__launch-copy">
            <small>{clientVersion ? `SNine Client ${clientVersion}` : selectedProfile?.displayName ?? "SNINE CLIENT"}</small>
            <strong>{launchLabel}</strong>
          </span>
          <span className="snine-one-home__launch-arrow">→</span>
        </button>


        {(launchError || !playerAccount) ? (
          <div className="snine-one-home__footer-status">
            {launchError ? (
              <span className="snine-one-home__error">{launchError}</span>
            ) : (
              <span>{t("launcher.home.noAccount")}</span>
            )}
          </div>
        ) : null}
      </footer>

      {loginPrompt ? (
        <div className="snine-account-login-overlay" role="dialog" aria-modal="true" aria-labelledby="snine-account-login-title">
          <div className="snine-account-login-dialog">
            <div className="snine-account-login-dialog__icon"><LogIn aria-hidden="true" /></div>
            <small>{t("launcher.home.loginLabel")}</small>
            <strong id="snine-account-login-title">{t("launcher.home.loginTitle")}</strong>
            <p>{t("launcher.home.loginInstruction")}</p>
            <button
              type="button"
              className="snine-account-login-dialog__code"
              onClick={() => navigator.clipboard?.writeText(loginPrompt.userCode)}
              title={t("launcher.home.copyCode")}
            >
              {loginPrompt.userCode}
            </button>
            {loginError ? <p className="snine-account-login-dialog__error">{loginError}</p> : null}
            <div className="snine-account-login-dialog__actions">
              <button type="button" onClick={() => void cancelAccountLogin()}>{t("app.cancel").toUpperCase()}</button>
              <button type="button" onClick={() => void openMicrosoftVerification(loginPrompt.verificationUri)}><ExternalLink aria-hidden="true" /> {t("launcher.home.openMicrosoft")}</button>
              <button type="button" className="is-primary" onClick={() => void completeAccountLogin()} disabled={loginBusy === "complete"}>
                {loginBusy === "complete" ? <LoaderCircle className="ui-spin" aria-hidden="true" /> : <Check aria-hidden="true" />} {t("launcher.home.done")}
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {downloadDialog.visible ? (
        <div className="snine-download-overlay" role="dialog" aria-modal="true" aria-labelledby="snine-download-title">
          <div className="snine-download-dialog">
            <div className="snine-download-dialog__top">
              <span className="snine-download-dialog__icon">
                {downloadDialog.stage === "complete" ? <Check aria-hidden="true" /> : downloadDialog.stage === "error" ? <X aria-hidden="true" /> : <Download aria-hidden="true" />}
              </span>
              <div>
                <small>{t("launcher.home.updateLabel")}</small>
                <strong id="snine-download-title">
                  {downloadDialog.stage === "verifying" ? t("launcher.home.updateVerifying") : downloadDialog.stage === "complete" ? t("launcher.home.updateReady") : downloadDialog.stage === "error" ? t("launcher.home.updateFailed") : t("launcher.home.updateDownloading")}
                </strong>
              </div>
              {downloadDialog.stage === "error" ? (
                <button type="button" onClick={() => setDownloadDialog(HIDDEN_DOWNLOAD)} aria-label={t("launcher.home.closeWindow")}><X aria-hidden="true" /></button>
              ) : null}
            </div>

            {downloadDialog.stage === "error" ? (
              <p className="snine-download-dialog__error">{downloadDialog.error}</p>
            ) : (
              <>
                <div className="snine-download-dialog__progress-row">
                  <span>{downloadDialog.version ? t("launcher.home.version", { version: downloadDialog.version }) : "SNine Client"}</span>
                  <strong>{Math.round(downloadDialog.percent)}%</strong>
                </div>
                <div className="snine-download-dialog__bar" aria-label={t("launcher.home.percent", { percent: Math.round(downloadDialog.percent) })}>
                  <span style={{ width: `${Math.max(0, Math.min(100, downloadDialog.percent))}%` }} />
                </div>
                <div className="snine-download-dialog__bytes">
                  <span>{formatBytes(downloadDialog.downloadedBytes)}</span>
                  <span>{formatBytes(downloadDialog.totalBytes)}</span>
                </div>
              </>
            )}
          </div>
        </div>
      ) : null}
    </div>
  );
}
