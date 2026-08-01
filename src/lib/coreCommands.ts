import { invoke } from "@tauri-apps/api/core";
import {
  PHASE1_CORE_STATUS,
  type Phase1CoreStatus,
} from "./generated/ipc-contracts";

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

const browserStatus: Phase1CoreStatus = {
  schemaVersion: 0,
  platform: "browser-preview",
  registeredRoots: [],
  incompleteOperations: 0,
  startupRecoveredOperations: 0,
};

export const coreCommands = {
  status: (): Promise<Phase1CoreStatus> =>
    isTauriRuntime()
      ? invoke<Phase1CoreStatus>(PHASE1_CORE_STATUS)
      : Promise.resolve(structuredClone(browserStatus)),
};
