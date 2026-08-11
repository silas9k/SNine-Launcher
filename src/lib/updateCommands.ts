import { invoke } from "@tauri-apps/api/core";
import {
  PHASE7_APPLY_PROFILE_UPDATES,
  PHASE7_CREATE_RESTORE_POINT,
  PHASE7_PREVIEW_PROFILE_UPDATES,
  PHASE7_ROLLBACK_PROFILE,
  PHASE7_RESTORE_BACKUP,
  PHASE7_RUN_AUTOMATIC_UPDATES,
  PHASE7_SAVE_UPDATE_POLICY,
  PHASE7_UPDATE_SNAPSHOT,
  type Phase7RestorePoint,
  type Phase7UpdateOperationResult,
  type Phase7UpdatePolicy,
  type Phase7UpdatePreview,
  type Phase7UpdateSnapshot,
  type Phase4Profile,
} from "./generated/ipc-contracts";

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

const browserSnapshot: Phase7UpdateSnapshot = {
  policy: {
    formatVersion: 1,
    launcher: "manual",
    profiles: "manual",
    s9labComponent: "manual",
    content: "manual",
  },
  channels: [
    { channel: "launcher", mode: "manual", state: "unconfigured", reasonCode: "launcher_update_trust_not_configured" },
    { channel: "profiles", mode: "manual", state: "available", reasonCode: null },
    { channel: "s9lab-component", mode: "manual", state: "unconfigured", reasonCode: "s9lab_component_provider_unconfigured" },
    { channel: "content", mode: "manual", state: "available", reasonCode: null },
  ],
  profiles: [],
  restorePoints: [],
};

function desktopOnly(): never {
  throw {
    code: "update_desktop_runtime_required",
    messageKey: "error.update_desktop_runtime_required",
    params: {},
  };
}

export const updateCommands = {
  snapshot: (): Promise<Phase7UpdateSnapshot> =>
    isTauriRuntime() ? invoke(PHASE7_UPDATE_SNAPSHOT) : Promise.resolve(browserSnapshot),
  savePolicy: (policy: Phase7UpdatePolicy): Promise<Phase7UpdateSnapshot> =>
    isTauriRuntime()
      ? invoke(PHASE7_SAVE_UPDATE_POLICY, { policy })
      : Promise.resolve({ ...browserSnapshot, policy }),
  preview: (profileId: string): Promise<Phase7UpdatePreview> =>
    isTauriRuntime()
      ? invoke(PHASE7_PREVIEW_PROFILE_UPDATES, { profileId })
      : Promise.resolve({ profileId, baseRevisionId: "browser-preview", changes: [] }),
  createRestorePoint: (profileId: string): Promise<Phase7RestorePoint> =>
    isTauriRuntime()
      ? invoke(PHASE7_CREATE_RESTORE_POINT, { profileId })
      : Promise.reject(desktopOnly()),
  apply: (profileId: string, contentIds: string[]): Promise<Phase7UpdateOperationResult> =>
    isTauriRuntime()
      ? invoke(PHASE7_APPLY_PROFILE_UPDATES, { profileId, contentIds })
      : Promise.reject(desktopOnly()),
  rollback: (profileId: string, revisionId: string): Promise<Phase7UpdateOperationResult> =>
    isTauriRuntime()
      ? invoke(PHASE7_ROLLBACK_PROFILE, { profileId, revisionId })
      : Promise.reject(desktopOnly()),
  restoreBackup: (
    backupId: string,
    displayName: string,
    selection: { includeAccount: boolean; includeSettings: boolean; includeFiles: boolean },
  ): Promise<Phase4Profile> =>
    isTauriRuntime()
      ? invoke(PHASE7_RESTORE_BACKUP, { backupId, displayName, ...selection })
      : Promise.reject(desktopOnly()),
  runAutomatic: (): Promise<Phase7UpdateOperationResult[]> =>
    isTauriRuntime()
      ? invoke(PHASE7_RUN_AUTOMATIC_UPDATES)
      : Promise.resolve([]),
};
