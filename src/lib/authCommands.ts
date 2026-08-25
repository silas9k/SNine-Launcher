import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  PHASE3_AUTH_SNAPSHOT,
  PHASE3_ASSIGN_PROFILE_ACCOUNT,
  PHASE3_CANCEL_DEVICE_LOGIN,
  PHASE3_COMPLETE_DEVICE_LOGIN,
  PHASE3_REFRESH_ACCOUNT,
  PHASE3_REMOVE_ACCOUNT,
  PHASE3_SELECT_ACCOUNT,
  PHASE3_START_DEVICE_LOGIN,
  type Phase3Account,
  type Phase3AuthSnapshot,
  type Phase3DeviceLoginPrompt,
} from "./generated/ipc-contracts";

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

const browserSnapshot: Phase3AuthSnapshot = {
  accounts: [],
  activeAccountId: null,
  offlinePolicy: {
    policy: "unconfigured",
    eligible: false,
    reason: "offline_policy_unconfigured",
  },
};

function desktopOnly(): never {
  throw {
    code: "auth_desktop_runtime_required",
    messageKey: "error.auth_desktop_runtime_required",
    params: {},
  };
}

export const authCommands = {
  snapshot: async (): Promise<Phase3AuthSnapshot> =>
    isTauriRuntime() ? invoke<Phase3AuthSnapshot>(PHASE3_AUTH_SNAPSHOT) : structuredClone(browserSnapshot),
  startDeviceLogin: (locale: "de" | "en"): Promise<Phase3DeviceLoginPrompt> =>
    isTauriRuntime()
      ? invoke<Phase3DeviceLoginPrompt>(PHASE3_START_DEVICE_LOGIN, { locale })
      : Promise.reject(desktopOnly()),
  completeDeviceLogin: (loginId: string): Promise<Phase3Account> =>
    isTauriRuntime()
      ? invoke<Phase3Account>(PHASE3_COMPLETE_DEVICE_LOGIN, { loginId })
      : Promise.reject(desktopOnly()),
  cancelDeviceLogin: (loginId: string): Promise<void> =>
    isTauriRuntime()
      ? invoke<void>(PHASE3_CANCEL_DEVICE_LOGIN, { loginId })
      : Promise.resolve(),
  refreshAccount: (accountId: string): Promise<Phase3Account> =>
    isTauriRuntime()
      ? invoke<Phase3Account>(PHASE3_REFRESH_ACCOUNT, { accountId })
      : Promise.reject(desktopOnly()),
  selectAccount: (accountId: string): Promise<Phase3Account> =>
    isTauriRuntime()
      ? invoke<Phase3Account>(PHASE3_SELECT_ACCOUNT, { accountId })
      : Promise.reject(desktopOnly()),
  removeAccount: (accountId: string): Promise<void> =>
    isTauriRuntime()
      ? invoke<void>(PHASE3_REMOVE_ACCOUNT, { accountId })
      : Promise.reject(desktopOnly()),
  assignProfileAccount: (profileId: string, accountId: string | null): Promise<void> =>
    isTauriRuntime()
      ? invoke<void>(PHASE3_ASSIGN_PROFILE_ACCOUNT, { profileId, accountId })
      : Promise.reject(desktopOnly()),
};

export async function openMicrosoftVerification(url: string): Promise<void> {
  const parsed = new URL(url);
  const allowed = parsed.protocol === "https:"
    && (parsed.hostname === "microsoft.com"
      || parsed.hostname.endsWith(".microsoft.com")
      || parsed.hostname === "microsoftonline.com"
      || parsed.hostname.endsWith(".microsoftonline.com"))
    && !parsed.username
    && !parsed.password
    && !parsed.port;
  if (!allowed) throw new Error("auth_verification_uri_invalid");
  if (isTauriRuntime()) await openUrl(parsed.href);
  else window.open(parsed.href, "_blank", "noopener,noreferrer");
}
