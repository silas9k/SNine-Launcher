// Generated from contracts/ipc-contracts.json. Do not edit manually.
export const IPC_CONTRACT_VERSION = 4 as const;

export interface TypedIpcError {
  code: string;
  messageKey: string;
  params: Record<string, string>;
}

export interface Phase1CoreStatus {
  schemaVersion: number;
  platform: string;
  registeredRoots: string[];
  incompleteOperations: number;
  startupRecoveredOperations: number;
}

export interface ShellSettings {
  appearance: "system" | "light" | "dark" | "contrast";
  locale: "system" | "de" | "en";
  accentColor: string;
  density: "compact" | "comfortable";
  navigationMode: "compact" | "expanded";
  backgroundVariant: "calm" | "grid" | "terrain";
  reducedMotion: boolean;
}

export interface Phase2ShellBootstrap {
  settings: ShellSettings;
}

export interface Phase2SaveShellSettingsInput {
  settings: ShellSettings;
}

export interface Phase3Account {
  id: string;
  username: string;
  kind: "microsoft";
  sessionState: "active" | "relogin-required";
  ownershipVerifiedAtUnix: number;
  lastOnlineAuthAtUnix: number;
  addedAtUnix: number;
  lastUsedAtUnix: number;
}

export interface Phase3OfflinePolicy {
  policy: "unconfigured";
  eligible: boolean;
  reason: string;
}

export interface Phase3AuthSnapshot {
  accounts: Phase3Account[];
  activeAccountId: string | null;
  offlinePolicy: Phase3OfflinePolicy;
}

export interface Phase3DeviceLoginPrompt {
  loginId: string;
  userCode: string;
  verificationUri: string;
  expiresAtUnix: number;
  intervalSeconds: number;
}

export interface Phase3StartDeviceLoginInput {
  locale: "de" | "en";
}

export interface Phase3LoginIdInput {
  loginId: string;
}

export interface Phase3AccountIdInput {
  accountId: string;
}

export interface Phase3ProfileAccountInput {
  profileId: string;
  accountId: string | null;
}

export interface Phase4Profile {
  id: string;
  displayName: string;
  lifecycleState: "active" | "archived" | "trash";
  activeRevisionId: string;
  accountId: string | null;
  favorite: boolean;
  verificationState: "verified" | "unverified";
  sourceProfileId: string | null;
  createdAtUnix: number;
  updatedAtUnix: number;
}

export interface Phase4CreateProfileInput {
  displayName: string;
}

export interface Phase4DuplicateProfileInput {
  profileId: string;
  displayName: string;
}

export interface Phase4ProfileIdInput {
  profileId: string;
}

export interface Phase4FavoriteInput {
  profileId: string;
  favorite: boolean;
}

export interface Phase4CacheGcReport {
  scannedBlobs: number;
  reachableBlobs: number;
  eligibleForQuarantine: number;
  eligibleBytes: number;
  quarantinedThisRun: number;
  restoredThisRun: number;
  retainedInQuarantine: number;
  deletionPolicy: "unconfigured";
}

export const PHASE1_CORE_STATUS = "phase1_core_status" as const;

export const PHASE2_SHELL_BOOTSTRAP = "phase2_shell_bootstrap" as const;

export const PHASE2_SAVE_SHELL_SETTINGS = "phase2_save_shell_settings" as const;

export const PHASE3_AUTH_SNAPSHOT = "phase3_auth_snapshot" as const;

export const PHASE3_START_DEVICE_LOGIN = "phase3_start_device_login" as const;

export const PHASE3_COMPLETE_DEVICE_LOGIN = "phase3_complete_device_login" as const;

export const PHASE3_CANCEL_DEVICE_LOGIN = "phase3_cancel_device_login" as const;

export const PHASE3_REFRESH_ACCOUNT = "phase3_refresh_account" as const;

export const PHASE3_SELECT_ACCOUNT = "phase3_select_account" as const;

export const PHASE3_REMOVE_ACCOUNT = "phase3_remove_account" as const;

export const PHASE3_ASSIGN_PROFILE_ACCOUNT = "phase3_assign_profile_account" as const;

export const PHASE4_LIST_PROFILES = "phase4_list_profiles" as const;

export const PHASE4_CREATE_PROFILE = "phase4_create_profile" as const;

export const PHASE4_DUPLICATE_PROFILE = "phase4_duplicate_profile" as const;

export const PHASE4_ARCHIVE_PROFILE = "phase4_archive_profile" as const;

export const PHASE4_TRASH_PROFILE = "phase4_trash_profile" as const;

export const PHASE4_RESTORE_PROFILE = "phase4_restore_profile" as const;

export const PHASE4_SET_PROFILE_FAVORITE = "phase4_set_profile_favorite" as const;

export const PHASE4_CACHE_GC_PREVIEW = "phase4_cache_gc_preview" as const;

export const PHASE4_QUARANTINE_UNREFERENCED_CACHE = "phase4_quarantine_unreferenced_cache" as const;

