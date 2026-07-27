import { invoke } from "@tauri-apps/api/core";
import {
  PHASE4_ARCHIVE_PROFILE,
  PHASE4_CACHE_GC_PREVIEW,
  PHASE4_CREATE_PROFILE,
  PHASE4_DUPLICATE_PROFILE,
  PHASE4_LIST_PROFILES,
  PHASE4_QUARANTINE_UNREFERENCED_CACHE,
  PHASE4_RESTORE_PROFILE,
  PHASE4_SET_PROFILE_FAVORITE,
  PHASE4_TRASH_PROFILE,
  type Phase4CacheGcReport,
  type Phase4Profile,
} from "./generated/ipc-contracts";

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function desktopOnly(): never {
  throw {
    code: "profile_desktop_runtime_required",
    messageKey: "error.profile_desktop_runtime_required",
    params: {},
  };
}

export const profileCommands = {
  list: async (): Promise<Phase4Profile[]> =>
    isTauriRuntime() ? invoke<Phase4Profile[]>(PHASE4_LIST_PROFILES) : [],
  create: (displayName: string): Promise<Phase4Profile> =>
    isTauriRuntime()
      ? invoke<Phase4Profile>(PHASE4_CREATE_PROFILE, { displayName })
      : Promise.reject(desktopOnly()),
  duplicate: (profileId: string, displayName: string): Promise<Phase4Profile> =>
    isTauriRuntime()
      ? invoke<Phase4Profile>(PHASE4_DUPLICATE_PROFILE, { profileId, displayName })
      : Promise.reject(desktopOnly()),
  archive: (profileId: string): Promise<Phase4Profile> =>
    isTauriRuntime()
      ? invoke<Phase4Profile>(PHASE4_ARCHIVE_PROFILE, { profileId })
      : Promise.reject(desktopOnly()),
  trash: (profileId: string): Promise<Phase4Profile> =>
    isTauriRuntime()
      ? invoke<Phase4Profile>(PHASE4_TRASH_PROFILE, { profileId })
      : Promise.reject(desktopOnly()),
  restore: (profileId: string): Promise<Phase4Profile> =>
    isTauriRuntime()
      ? invoke<Phase4Profile>(PHASE4_RESTORE_PROFILE, { profileId })
      : Promise.reject(desktopOnly()),
  setFavorite: (profileId: string, favorite: boolean): Promise<Phase4Profile> =>
    isTauriRuntime()
      ? invoke<Phase4Profile>(PHASE4_SET_PROFILE_FAVORITE, { profileId, favorite })
      : Promise.reject(desktopOnly()),
  cachePreview: (): Promise<Phase4CacheGcReport> =>
    isTauriRuntime()
      ? invoke<Phase4CacheGcReport>(PHASE4_CACHE_GC_PREVIEW)
      : Promise.resolve({
          scannedBlobs: 0,
          reachableBlobs: 0,
          eligibleForQuarantine: 0,
          eligibleBytes: 0,
          quarantinedThisRun: 0,
          restoredThisRun: 0,
          retainedInQuarantine: 0,
          deletionPolicy: "unconfigured",
        }),
  quarantineUnreferenced: (): Promise<Phase4CacheGcReport> =>
    isTauriRuntime()
      ? invoke<Phase4CacheGcReport>(PHASE4_QUARANTINE_UNREFERENCED_CACHE)
      : Promise.reject(desktopOnly()),
};
