import { invoke } from "@tauri-apps/api/core";
import {
  SNINE_CLIENT_DOWNLOAD_UPDATE,
  SNINE_CLIENT_UPDATE_CHECK,
  type SnineClientDownloadResult,
  type SnineClientUpdateCheck,
} from "./generated/ipc-contracts";

export type { SnineClientDownloadResult, SnineClientUpdateCheck } from "./generated/ipc-contracts";

export interface SnineClientDownloadProgress {
  profileId: string;
  downloadedBytes: number;
  totalBytes: number | null;
  percent: number;
  stage: "downloading" | "verifying" | "complete";
}

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const snineClientUpdate = {
  async check(profileId: string): Promise<SnineClientUpdateCheck> {
    if (!isTauriRuntime()) throw new Error("snine_update_desktop_runtime_required");
    return invoke<SnineClientUpdateCheck>(SNINE_CLIENT_UPDATE_CHECK, { profileId });
  },
  async download(profileId: string): Promise<SnineClientDownloadResult> {
    if (!isTauriRuntime()) throw new Error("snine_update_desktop_runtime_required");
    return invoke<SnineClientDownloadResult>(SNINE_CLIENT_DOWNLOAD_UPDATE, { profileId });
  },
};
