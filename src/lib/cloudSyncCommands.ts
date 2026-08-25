import { invoke } from "@tauri-apps/api/core";
import {
  PHASE8_CLOUD_SYNC_SNAPSHOT,
  type Phase8CloudSyncSnapshot,
} from "./generated/ipc-contracts";

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

const browserSnapshot: Phase8CloudSyncSnapshot = {
  providerState: "unconfigured",
  reasonCode: "cloud_provider_unconfigured",
  microsoftBaseAccount: null,
  linkedS9labAccount: null,
  sessionState: "unavailable",
  online: false,
  deviceLimit: 2,
  enrolledDevices: 0,
  scopes: ["profile-metadata", "content-lists", "settings"],
  localRevision: {
    revisionId: "local-browser-preview",
    payloadSha256: "0".repeat(64),
    profileCount: 0,
    contentCount: 0,
    settingsIncluded: true,
  },
  pendingConflicts: 0,
};

export const cloudSyncCommands = {
  snapshot: (): Promise<Phase8CloudSyncSnapshot> => isTauriRuntime()
    ? invoke<Phase8CloudSyncSnapshot>(PHASE8_CLOUD_SYNC_SNAPSHOT)
    : Promise.resolve(structuredClone(browserSnapshot)),
};
