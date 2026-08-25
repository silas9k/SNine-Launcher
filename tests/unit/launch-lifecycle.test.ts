import { describe, expect, it } from "vitest";
import type { Phase5LaunchStatus } from "../../src/lib/generated/ipc-contracts";
import {
  didLaunchCrash,
  isLaunchActive,
  latestLaunchForProfile,
  launchButtonKey,
  tryAcquireLaunchGuard,
} from "../../src/lib/launchLifecycle";
import { DEFAULT_SHELL_SETTINGS } from "../../src/theme/types";

function status(overrides: Partial<Phase5LaunchStatus> = {}): Phase5LaunchStatus {
  return {
    launchId: "launch-1",
    profileId: "profile-1",
    state: "preparing",
    processId: null,
    accountName: "Player",
    startedAtUnix: 1,
    finishedAtUnix: null,
    exitCode: null,
    failureCode: null,
    ...overrides,
  };
}

describe("Minecraft launch lifecycle", () => {
  it("locks every non-terminal launch phase", () => {
    for (const state of ["preparing", "checking-files", "downloading", "starting", "running", "stopping"] as const) {
      expect(isLaunchActive(state)).toBe(true);
    }
    expect(isLaunchActive("idle")).toBe(false);
    expect(isLaunchActive("exited")).toBe(false);
    expect(isLaunchActive("failed")).toBe(false);
  });

  it("allows exactly one of five synchronous launch attempts", () => {
    const guard = { current: false };
    expect(Array.from({ length: 5 }, () => tryAcquireLaunchGuard(guard))).toEqual([
      true, false, false, false, false,
    ]);
  });

  it("maps real backend states to play button text", () => {
    expect(launchButtonKey("preparing")).toBe("launcher.home.preparing");
    expect(launchButtonKey("checking-files")).toBe("launcher.home.checkingFiles");
    expect(launchButtonKey("downloading")).toBe("launcher.home.downloading");
    expect(launchButtonKey("starting")).toBe("launcher.home.starting");
    expect(launchButtonKey("failed")).toBe("launcher.home.retry");
  });

  it("keeps the newest completed session available for logs", () => {
    const latest = latestLaunchForProfile([
      status({ launchId: "old", state: "exited", startedAtUnix: 1, finishedAtUnix: 2, exitCode: 0 }),
      status({ launchId: "new", state: "failed", startedAtUnix: 3, finishedAtUnix: 4, failureCode: "runtime_java_not_found" }),
    ], "profile-1");
    expect(latest?.launchId).toBe("new");
    expect(didLaunchCrash(latest!)).toBe(true);
  });

  it("never lets same-second history hide an active session", () => {
    const latest = latestLaunchForProfile([
      status({ launchId: "z-old", state: "exited", startedAtUnix: 10, finishedAtUnix: 11, exitCode: 0 }),
      status({ launchId: "a-current", state: "preparing", startedAtUnix: 10 }),
    ], "profile-1");
    expect(latest?.launchId).toBe("a-current");
  });

  it("keeps automatic logs disabled by default", () => {
    expect(DEFAULT_SHELL_SETTINGS.autoOpenMinecraftLog).toBe(false);
  });
});
