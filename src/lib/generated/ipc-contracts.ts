// Generated from contracts/ipc-contracts.json. Do not edit manually.
export const IPC_CONTRACT_VERSION = 11 as const;

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
  showMinecraftSnapshots: boolean;
  showOldMinecraftVersions: boolean;
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

export interface Phase5CapabilityStatus {
  capabilityId: string;
  state: "available" | "unconfigured" | "disabled";
  reasonCode: string;
}

export interface Phase5MinecraftCatalogEntry {
  version: string;
  releaseType: "release" | "snapshot";
}

export interface Phase5LoaderCatalogEntry {
  version: string;
  stable: boolean;
}

export interface Phase5RuntimeCatalog {
  minecraftVersions: Phase5MinecraftCatalogEntry[];
  fabricVersions: Phase5LoaderCatalogEntry[];
  selectedMinecraftJavaMajor: 8 | 17 | 21 | 25 | null;
  neoforgeCapability: Phase5CapabilityStatus;
  s9labComponentCapability: Phase5CapabilityStatus;
}

export interface Phase5LoaderSelection {
  kind: "vanilla" | "fabric" | "neoforge";
  loaderVersion?: string;
}

export interface Phase5JavaPolicy {
  mode: "managed" | "system";
  majorVersion: 8 | 17 | 21 | 25;
}

export interface Phase5RuntimeIntent {
  minecraftVersion: string;
  loader: Phase5LoaderSelection;
  java: Phase5JavaPolicy;
}

export interface Phase5ComponentCatalogEntry {
  componentId: string;
  componentVersion: string;
  minecraftVersion: string;
  loader: Phase5LoaderSelection;
  sizeBytes: number;
  sha256: string;
}

export interface Phase5ComponentCatalog {
  capability: Phase5CapabilityStatus;
  entries: Phase5ComponentCatalogEntry[];
}

export interface Phase5ComponentSelection {
  mode: "disabled" | "catalog";
  componentId?: string;
  componentVersion?: string;
}

export interface Phase5InstalledComponent {
  componentId: string;
  componentVersion: string;
}

export interface Phase5LaunchStatus {
  launchId: string;
  profileId: string;
  state: "preparing" | "checking-files" | "downloading" | "starting" | "running" | "stopping" | "exited" | "failed";
  processId: number | null;
  accountName: string;
  startedAtUnix: number;
  finishedAtUnix: number | null;
  exitCode: number | null;
  failureCode: string | null;
}

export interface Phase5RuntimeStatus {
  profileId: string;
  activeRevisionId: string;
  lifecycleState: "active" | "archived" | "trash";
  installState: "not-configured" | "configured" | "installed" | "repair-required";
  runtime: Phase5RuntimeIntent | null;
  component: Phase5InstalledComponent | null;
  launches: Phase5LaunchStatus[];
  s9labComponentCapability: Phase5CapabilityStatus;
}

export interface Phase5OperationResult {
  operationId: string;
  profileId: string;
  revisionId: string;
  installState: "installed";
}

export interface SnineClientUpdateCheck {
  reachable: boolean;
  updateAvailable: boolean;
  externalClientInstalled: boolean;
  installedVersion: string | null;
  remoteVersion: string | null;
  remoteSizeBytes: number | null;
  statusMessage: string;
}

export interface SnineClientDownloadResult {
  installedVersion: string | null;
  sha256: string;
  sizeBytes: number;
  targetFile: string;
}

export interface SnineClientProfileInput {
  profileId: string;
}

export interface Phase5CatalogInput {
  minecraftVersion: string | null;
}

export interface Phase5ComponentCatalogInput {
  intent: Phase5RuntimeIntent;
}

export interface Phase5ProfileIdInput {
  profileId: string;
}

export interface Phase5InstallInput {
  profileId: string;
  intent: Phase5RuntimeIntent;
  component: Phase5ComponentSelection;
}

export interface Phase5LaunchInput {
  profileId: string;
  memoryMb: number;
}

export interface Phase5StopInput {
  launchId: string;
}

export interface Phase4RenameProfileInput {
  profileId: string;
  displayName: string;
}

export interface Phase5InstanceSettings {
  formatVersion: 1;
  icon: string;
  minRamMb: number;
  maxRamMb: number;
  jvmArguments: string[];
  width: number;
  height: number;
  fullscreen: boolean;
  customJavaExecutable: string | null;
  lastPlayedAtUnix: number | null;
}

export interface Phase5SaveInstanceSettingsInput {
  profileId: string;
  settings: Phase5InstanceSettings;
}

export interface Phase5OpenInstanceFolderInput {
  profileId: string;
  folder: "game" | "mods" | "resourcepacks" | "screenshots" | "logs";
}

export interface Phase5ComponentChangeInput {
  profileId: string;
  selection: Phase5ComponentSelection;
}

export interface Phase6Capability {
  capabilityId: string;
  state: "available" | "unconfigured" | "disabled";
  reasonCode: string;
}

export interface Phase6Dependency {
  projectId: string;
  displayName: string;
  relation: "required" | "optional" | "incompatible";
  satisfied: boolean;
}

export interface Phase6Conflict {
  contentId: string;
  displayName: string;
  reasonCode: string;
}

export interface Phase6InstalledContentUpdate {
  versionId: string;
  versionNumber: string;
}

export interface Phase6InstalledContent {
  contentId: string;
  projectId: string | null;
  versionId: string | null;
  displayName: string;
  versionNumber: string;
  contentType: "mod" | "modpack" | "shaderPack" | "resourcePack";
  source: "modrinth" | "local";
  enabled: boolean;
  managedByPack: boolean;
  sizeBytes: number;
  sha256: string;
  dependencies: Phase6Dependency[];
  conflicts: Phase6Conflict[];
  update: Phase6InstalledContentUpdate | null;
}

export interface Phase6ContentSnapshot {
  profileId: string;
  minecraftVersion: string | null;
  loader: "vanilla" | "fabric" | "neoforge" | null;
  lockSha256: string | null;
  content: Phase6InstalledContent[];
  localFileCapability: Phase6Capability;
  profileFormatCapability: Phase6Capability;
}

export interface Phase6SearchInput {
  query: string;
  contentType: "mod" | "modpack" | "shaderPack" | "resourcePack";
  minecraftVersion: string;
  loader: "vanilla" | "fabric" | "neoforge";
  offset: number;
  limit: number;
}

export interface Phase6SearchHit {
  projectId: string;
  slug: string;
  title: string;
  description: string;
  contentType: "mod" | "modpack" | "shaderPack" | "resourcePack";
  author: string;
  downloads: number;
  follows: number;
  iconUrl: string | null;
  updatedAtUnix: number;
  latestVersion: string | null;
}

export interface Phase6SearchResult {
  capability: Phase6Capability;
  total: number;
  offset: number;
  hits: Phase6SearchHit[];
}

export interface Phase6ProjectVersion {
  versionId: string;
  versionNumber: string;
  name: string;
  publishedAtUnix: number;
  compatible: boolean;
  incompatibilityReason: string | null;
  dependencies: Phase6Dependency[];
  conflicts: Phase6Conflict[];
}

export interface Phase6ProjectDetail {
  projectId: string;
  slug: string;
  title: string;
  description: string;
  contentType: "mod" | "modpack" | "shaderPack" | "resourcePack";
  author: string;
  license: string;
  iconUrl: string | null;
  downloads: number;
  followers: number;
  updatedAtUnix: number;
  categories: string[];
  versions: Phase6ProjectVersion[];
}

export interface Phase6OperationResult {
  operationId: string;
  profileId: string;
  revisionId: string;
  changedContentIds: string[];
}

export interface Phase6ProfileTransferResult {
  operationId: string;
  profileId: string;
  displayName: string;
  fileName: string | null;
}

export interface Phase6ProfileIdInput {
  profileId: string;
}

export interface Phase6ProjectIdInput {
  profileId: string;
  projectId: string;
}

export interface Phase6InstallInput {
  profileId: string;
  projectId: string;
  versionId: string | null;
}

export interface Phase6ContentToggleInput {
  profileId: string;
  contentId: string;
  enabled: boolean;
}

export interface Phase6ContentIdInput {
  profileId: string;
  contentId: string;
}

export interface Phase6LocalFileInput {
  profileId: string;
  sourcePath: string;
  contentType: "mod" | "modpack" | "shaderPack" | "resourcePack";
}

export interface Phase6ProfileFileInput {
  sourcePath: string;
}

export interface Phase6ModrinthPackInput {
  profileId: string;
  sourcePath: string;
}

export interface Phase7UpdatePolicy {
  formatVersion: 1;
  launcher: "manual" | "automatic";
  profiles: "manual" | "automatic";
  s9labComponent: "manual" | "automatic";
  content: "manual" | "automatic";
}

export interface Phase7UpdateChannel {
  channel: "launcher" | "profiles" | "s9lab-component" | "content";
  mode: "manual" | "automatic";
  state: "available" | "unconfigured" | "disabled";
  reasonCode: string | null;
}

export interface Phase7Revision {
  revisionId: string;
  createdAtUnix: number;
  active: boolean;
}

export interface Phase7UpdateProfile {
  profileId: string;
  displayName: string;
  activeRevisionId: string;
  revisions: Phase7Revision[];
}

export interface Phase7RestorePoint {
  backupId: string;
  profileId: string;
  profileName: string;
  sourceRevisionId: string;
  createdAtUnix: number;
  fileCount: number;
  sizeBytes: number;
}

export interface Phase7UpdateSnapshot {
  policy: Phase7UpdatePolicy;
  channels: Phase7UpdateChannel[];
  profiles: Phase7UpdateProfile[];
  restorePoints: Phase7RestorePoint[];
}

export interface Phase7UpdateChange {
  channel: "content";
  itemId: string;
  displayName: string;
  currentVersion: string;
  targetVersion: string;
  verification: string;
}

export interface Phase7UpdatePreview {
  profileId: string;
  baseRevisionId: string;
  changes: Phase7UpdateChange[];
}

export interface Phase7UpdateOperationResult {
  operationId: string;
  profileId: string;
  revisionId: string;
  restorePointId: string;
  appliedChanges: string[];
}

export interface Phase7SavePolicyInput {
  policy: Phase7UpdatePolicy;
}

export interface Phase7ApplyUpdatesInput {
  profileId: string;
  contentIds: string[];
}

export interface Phase7RollbackInput {
  profileId: string;
  revisionId: string;
}

export interface Phase7RestoreBackupInput {
  backupId: string;
  displayName: string;
  includeAccount: boolean;
  includeSettings: boolean;
  includeFiles: boolean;
}

export interface Phase8LocalSyncRevision {
  revisionId: string;
  payloadSha256: string;
  profileCount: number;
  contentCount: number;
  settingsIncluded: boolean;
}

export interface Phase8CloudSyncSnapshot {
  providerState: "unconfigured" | "offline" | "available";
  reasonCode: string;
  microsoftBaseAccount: string | null;
  linkedS9labAccount: string | null;
  sessionState: "unavailable" | "signed-out" | "active" | "expired";
  online: boolean;
  deviceLimit: number;
  enrolledDevices: number;
  scopes: ("profile-metadata" | "content-lists" | "settings")[];
  localRevision: Phase8LocalSyncRevision;
  pendingConflicts: number;
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

export const PHASE4_RENAME_PROFILE = "phase4_rename_profile" as const;

export const PHASE4_CACHE_GC_PREVIEW = "phase4_cache_gc_preview" as const;

export const PHASE4_QUARANTINE_UNREFERENCED_CACHE = "phase4_quarantine_unreferenced_cache" as const;

export const PHASE5_RUNTIME_CATALOG = "phase5_runtime_catalog" as const;

export const PHASE5_S9LAB_COMPONENT_CATALOG = "phase5_s9lab_component_catalog" as const;

export const PHASE5_PROFILE_RUNTIME_STATUS = "phase5_profile_runtime_status" as const;

export const PHASE5_INSTALL_PROFILE = "phase5_install_profile" as const;

export const PHASE5_REPAIR_PROFILE = "phase5_repair_profile" as const;

export const PHASE5_LAUNCH_PROFILE = "phase5_launch_profile" as const;

export const PHASE5_LAUNCH_INSTANCE = "phase5_launch_instance" as const;

export const PHASE5_INSTANCE_SETTINGS = "phase5_instance_settings" as const;

export const PHASE5_SAVE_INSTANCE_SETTINGS = "phase5_save_instance_settings" as const;

export const PHASE5_OPEN_INSTANCE_FOLDER = "phase5_open_instance_folder" as const;

export const PHASE5_STOP_LAUNCH = "phase5_stop_launch" as const;

export const PHASE5_LAUNCH_STATUSES = "phase5_launch_statuses" as const;

export const PHASE5_SET_S9LAB_COMPONENT = "phase5_set_s9lab_component" as const;

export const SNINE_CLIENT_UPDATE_CHECK = "snine_client_update_check" as const;

export const SNINE_CLIENT_DOWNLOAD_UPDATE = "snine_client_download_update" as const;

export const PHASE6_CONTENT_SNAPSHOT = "phase6_content_snapshot" as const;

export const PHASE6_CHECK_CONTENT_UPDATES = "phase6_check_content_updates" as const;

export const PHASE6_MODRINTH_SEARCH = "phase6_modrinth_search" as const;

export const PHASE6_MODRINTH_PROJECT = "phase6_modrinth_project" as const;

export const PHASE6_INSTALL_MODRINTH = "phase6_install_modrinth" as const;

export const PHASE6_SET_CONTENT_ENABLED = "phase6_set_content_enabled" as const;

export const PHASE6_REMOVE_CONTENT = "phase6_remove_content" as const;

export const PHASE6_UPDATE_CONTENT = "phase6_update_content" as const;

export const PHASE6_ADD_LOCAL_FILE = "phase6_add_local_file" as const;

export const PHASE6_IMPORT_MODRINTH_PACK = "phase6_import_modrinth_pack" as const;

export const PHASE6_EXPORT_PROFILE = "phase6_export_profile" as const;

export const PHASE6_IMPORT_PROFILE = "phase6_import_profile" as const;

export const PHASE7_UPDATE_SNAPSHOT = "phase7_update_snapshot" as const;

export const PHASE7_SAVE_UPDATE_POLICY = "phase7_save_update_policy" as const;

export const PHASE7_PREVIEW_PROFILE_UPDATES = "phase7_preview_profile_updates" as const;

export const PHASE7_CREATE_RESTORE_POINT = "phase7_create_restore_point" as const;

export const PHASE7_APPLY_PROFILE_UPDATES = "phase7_apply_profile_updates" as const;

export const PHASE7_ROLLBACK_PROFILE = "phase7_rollback_profile" as const;

export const PHASE7_RESTORE_BACKUP = "phase7_restore_backup" as const;

export const PHASE7_RUN_AUTOMATIC_UPDATES = "phase7_run_automatic_updates" as const;

export const PHASE8_CLOUD_SYNC_SNAPSHOT = "phase8_cloud_sync_snapshot" as const;

