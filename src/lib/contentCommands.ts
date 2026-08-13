import { invoke } from "@tauri-apps/api/core";
import {
  PHASE6_ADD_LOCAL_FILE,
  PHASE6_CHECK_CONTENT_UPDATES,
  PHASE6_CONTENT_SNAPSHOT,
  PHASE6_EXPORT_PROFILE,
  PHASE6_IMPORT_MODRINTH_PACK,
  PHASE6_IMPORT_PROFILE,
  PHASE6_INSTALL_MODRINTH,
  PHASE6_MODRINTH_PROJECT,
  PHASE6_MODRINTH_SEARCH,
  PHASE6_REMOVE_CONTENT,
  PHASE6_SET_CONTENT_ENABLED,
  PHASE6_UPDATE_CONTENT,
  type Phase6Capability,
  type Phase6ContentSnapshot,
  type Phase6InstalledContent,
  type Phase6OperationResult,
  type Phase6ProfileTransferResult,
  type Phase6ProjectDetail,
  type Phase6ProjectVersion,
  type Phase6SearchInput,
  type Phase6SearchResult,
} from "./generated/ipc-contracts";

export type {
  Phase6Capability,
  Phase6ContentSnapshot,
  Phase6InstalledContent,
  Phase6OperationResult,
  Phase6ProfileTransferResult,
  Phase6ProjectDetail,
  Phase6ProjectVersion,
  Phase6SearchInput,
  Phase6SearchResult,
};

export type Phase6ContentType = Phase6InstalledContent["contentType"];
export type Phase6Loader = NonNullable<Phase6ContentSnapshot["loader"]>;

export interface LocalContentFile extends File {
  /** Supplied by the desktop webview for an explicitly selected local file. */
  path?: string;
}

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function desktopOnly() {
  return {
    code: "content_desktop_runtime_required",
    messageKey: "error.content_desktop_runtime_required",
    params: {},
  };
}

function selectedPath(file: LocalContentFile): string {
  if (!isTauriRuntime()) throw desktopOnly();
  const path = typeof file.path === "string" ? file.path.trim() : "";
  if (!path) {
    throw {
      code: "content_local_path_unavailable",
      messageKey: "error.content_local_path_unavailable",
      params: {},
    };
  }
  return path;
}

const browserCapability: Phase6Capability = {
  capabilityId: "content.desktop",
  state: "disabled",
  reasonCode: "content_desktop_runtime_required",
};

export const contentCommands = {
  snapshot: (profileId: string): Promise<Phase6ContentSnapshot> =>
    isTauriRuntime()
      ? invoke<Phase6ContentSnapshot>(PHASE6_CONTENT_SNAPSHOT, { profileId })
      : Promise.resolve({
          profileId,
          minecraftVersion: null,
          loader: null,
          lockSha256: null,
          content: [],
          localFileCapability: structuredClone(browserCapability),
          profileFormatCapability: structuredClone(browserCapability),
        }),

  checkUpdates: (profileId: string): Promise<Phase6ContentSnapshot> =>
    isTauriRuntime()
      ? invoke<Phase6ContentSnapshot>(PHASE6_CHECK_CONTENT_UPDATES, { profileId })
      : Promise.reject(desktopOnly()),

  search: (input: Phase6SearchInput): Promise<Phase6SearchResult> =>
    isTauriRuntime()
      ? invoke<Phase6SearchResult>(PHASE6_MODRINTH_SEARCH, {
          query: input.query,
          contentType: input.contentType,
          minecraftVersion: input.minecraftVersion,
          loader: input.loader,
          offset: input.offset,
          limit: input.limit,
        })
      : Promise.resolve({
          capability: structuredClone(browserCapability),
          total: 0,
          offset: input.offset,
          hits: [],
        }),

  project: (profileId: string, projectId: string): Promise<Phase6ProjectDetail> =>
    isTauriRuntime()
      ? invoke<Phase6ProjectDetail>(PHASE6_MODRINTH_PROJECT, { profileId, projectId })
      : Promise.reject(desktopOnly()),

  install: (
    profileId: string,
    projectId: string,
    versionId: string | null,
  ): Promise<Phase6OperationResult> =>
    isTauriRuntime()
      ? invoke<Phase6OperationResult>(PHASE6_INSTALL_MODRINTH, { profileId, projectId, versionId })
      : Promise.reject(desktopOnly()),

  setEnabled: (
    profileId: string,
    contentId: string,
    enabled: boolean,
  ): Promise<Phase6OperationResult> =>
    isTauriRuntime()
      ? invoke<Phase6OperationResult>(PHASE6_SET_CONTENT_ENABLED, { profileId, contentId, enabled })
      : Promise.reject(desktopOnly()),

  remove: (profileId: string, contentId: string): Promise<Phase6OperationResult> =>
    isTauriRuntime()
      ? invoke<Phase6OperationResult>(PHASE6_REMOVE_CONTENT, { profileId, contentId })
      : Promise.reject(desktopOnly()),

  update: (profileId: string, contentId: string): Promise<Phase6OperationResult> =>
    isTauriRuntime()
      ? invoke<Phase6OperationResult>(PHASE6_UPDATE_CONTENT, { profileId, contentId })
      : Promise.reject(desktopOnly()),

  addLocal: (
    profileId: string,
    file: LocalContentFile,
    contentType: Phase6ContentType,
  ): Promise<Phase6OperationResult> =>
    invoke<Phase6OperationResult>(PHASE6_ADD_LOCAL_FILE, {
      profileId,
      sourcePath: selectedPath(file),
      contentType,
    }),

  importModrinthPack: (
    profileId: string,
    file: LocalContentFile,
  ): Promise<Phase6OperationResult> =>
    invoke<Phase6OperationResult>(PHASE6_IMPORT_MODRINTH_PACK, {
      profileId,
      sourcePath: selectedPath(file),
    }),

  exportProfile: (profileId: string): Promise<Phase6ProfileTransferResult> =>
    isTauriRuntime()
      ? invoke<Phase6ProfileTransferResult>(PHASE6_EXPORT_PROFILE, { profileId })
      : Promise.reject(desktopOnly()),

  importProfile: (file: LocalContentFile): Promise<Phase6ProfileTransferResult> =>
    invoke<Phase6ProfileTransferResult>(PHASE6_IMPORT_PROFILE, {
      sourcePath: selectedPath(file),
    }),
};
