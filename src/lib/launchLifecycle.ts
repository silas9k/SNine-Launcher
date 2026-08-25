import type { Phase5LaunchStatus } from "./generated/ipc-contracts";
import type { TranslationKey } from "../i18n/messages";

export type LaunchUiState = "idle" | Phase5LaunchStatus["state"];

const ACTIVE_STATES = new Set<Phase5LaunchStatus["state"]>([
  "preparing",
  "checking-files",
  "downloading",
  "starting",
  "running",
  "stopping",
]);

export function isLaunchActive(state: LaunchUiState): boolean {
  return state !== "idle" && ACTIVE_STATES.has(state);
}

export function isLaunchLocked(state: LaunchUiState): boolean {
  return isLaunchActive(state);
}

export function latestLaunchForProfile(
  statuses: Phase5LaunchStatus[],
  profileId: string,
): Phase5LaunchStatus | null {
  return statuses
    .filter((status) => status.profileId === profileId)
    .sort((left, right) => (
      Number(isLaunchActive(right.state)) - Number(isLaunchActive(left.state))
      || right.startedAtUnix - left.startedAtUnix
      || right.launchId.localeCompare(left.launchId)
    ))[0] ?? null;
}

export function launchButtonKey(state: LaunchUiState): TranslationKey {
  switch (state) {
    case "preparing": return "launcher.home.preparing";
    case "checking-files": return "launcher.home.checkingFiles";
    case "downloading": return "launcher.home.downloading";
    case "starting": return "launcher.home.starting";
    case "running": return "launcher.home.running";
    case "stopping": return "launcher.home.stopping";
    case "failed": return "launcher.home.retry";
    default: return "launcher.home.launch";
  }
}

export function didLaunchCrash(status: Phase5LaunchStatus): boolean {
  return status.state === "failed"
    || (status.state === "exited" && status.exitCode != null && status.exitCode !== 0);
}

export function tryAcquireLaunchGuard(guard: { current: boolean }): boolean {
  if (guard.current) return false;
  guard.current = true;
  return true;
}
