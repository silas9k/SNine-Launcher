import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../src/i18n/I18nProvider";
import { useWorkspaceStore } from "../../src/app/workspaceStore";

const mocks = vi.hoisted(() => ({
  list: vi.fn(),
  create: vi.fn(),

  snapshot: vi.fn(),
  assignProfileAccount: vi.fn(),
  selectAccount: vi.fn(),
  startDeviceLogin: vi.fn(),
  cancelDeviceLogin: vi.fn(),
  completeDeviceLogin: vi.fn(),

  launch: vi.fn(),
  stop: vi.fn(),
  launchStatuses: vi.fn(),

  updateCheck: vi.fn(),
  updateDownload: vi.fn(),

  cosmetics: vi.fn(),
  pollLive: vi.fn(),
  resolveLive: vi.fn(),
}));

vi.mock("../../src/lib/profileCommands", () => ({
  profileCommands: {
    list: mocks.list,
    create: mocks.create,
  },
}));

vi.mock("../../src/lib/authCommands", () => ({
  authCommands: {
    snapshot: mocks.snapshot,
    assignProfileAccount: mocks.assignProfileAccount,
    selectAccount: mocks.selectAccount,
    startDeviceLogin: mocks.startDeviceLogin,
    cancelDeviceLogin: mocks.cancelDeviceLogin,
    completeDeviceLogin: mocks.completeDeviceLogin,
  },
  openMicrosoftVerification: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../src/lib/runtimeCommands", () => ({
  runtimeCommands: {
    launch: mocks.launch,
    stop: mocks.stop,
    launchStatuses: mocks.launchStatuses,
  },
}));

vi.mock("../../src/lib/snineClientUpdate", () => ({
  snineClientUpdate: {
    check: mocks.updateCheck,
    download: mocks.updateDownload,
  },
}));

vi.mock("../../src/lib/snineClientBridge", () => ({
  launcherBadgeIconUrl: () => "./snine-icons/snine_icon.png",
  loadSNineLauncherCosmetics: mocks.cosmetics,
  pollSNineLauncherLiveState: mocks.pollLive,
  resolveSNineLiveCosmetics: mocks.resolveLive,
}));

vi.mock("../../src/components/player/LauncherSkinPreview", () => ({
  LauncherSkinPreview: ({ playerName }: { playerName: string }) => (
    <div data-testid="launcher-player-preview">{playerName}</div>
  ),
}));

vi.mock("../../src/components/player/MinecraftAvatar", () => ({
  MinecraftAvatar: ({ username }: { username?: string }) => (
    <span data-testid="minecraft-avatar">{username ?? "avatar"}</span>
  ),
}));

import { HomePage } from "../../src/pages/HomePage";

const profile = {
  id: "profile-one",
  displayName: "Fabric profile",
  lifecycleState: "active" as const,
  activeRevisionId: "revision-one",
  accountId: "account-one",
  favorite: false,
  verificationState: "verified" as const,
  sourceProfileId: null,
  createdAtUnix: 1_900_000_000,
  updatedAtUnix: 1_900_000_000,
};

const account = {
  id: "account-one",
  username: "PlayerOne",
  kind: "microsoft" as const,
  sessionState: "active" as const,
  ownershipVerifiedAtUnix: 1_900_000_000,
  lastOnlineAuthAtUnix: 1_900_000_000,
  addedAtUnix: 1_900_000_000,
  lastUsedAtUnix: 1_900_000_000,
};

const launchResult = {
  launchId: "launch-one",
  profileId: "profile-one",
  state: "running" as const,
  processId: 4242,
  accountName: "PlayerOne",
  startedAtUnix: 1_900_000_100,
  finishedAtUnix: null,
  exitCode: null,
  failureCode: null,
};

beforeEach(() => {
  vi.clearAllMocks();

  delete (window as any).__TAURI_INTERNALS__;

  useWorkspaceStore.setState({
    selectedProfileId: null,
  });

  mocks.list.mockResolvedValue([profile]);

  mocks.snapshot.mockResolvedValue({
    accounts: [account],
    activeAccountId: account.id,
    offlinePolicy: {
      policy: "unconfigured",
      eligible: false,
      reason: "offline_policy_unconfigured",
    },
  });

  mocks.selectAccount.mockResolvedValue(account);
  mocks.assignProfileAccount.mockResolvedValue(undefined);

  mocks.launchStatuses.mockResolvedValue([]);
  mocks.launch.mockResolvedValue(launchResult);
  mocks.stop.mockResolvedValue({
    ...launchResult,
    state: "exited",
    processId: null,
    finishedAtUnix: 1_900_000_200,
    exitCode: 0,
  });

  mocks.updateCheck.mockResolvedValue({
    reachable: true,
    updateAvailable: false,
    installedVersion: "1.1.5",
    latestVersion: "1.1.5",
  });

  mocks.updateDownload.mockResolvedValue({
    reachable: true,
    updateAvailable: false,
    installedVersion: "1.1.5",
    latestVersion: "1.1.5",
  });

  mocks.cosmetics.mockResolvedValue({
    ok: true,
    playerName: "PlayerOne",
    online: true,
    badgeIcon: "",
    plusActive: false,
    equipped: [],
    source: "test",
    statusMessage: "ok",
    liveSync: null,
  });

  mocks.pollLive.mockResolvedValue({
    online: true,
    badgeIcon: "",
    plusActive: false,
    equippedCosmetics: {},
  });

  mocks.resolveLive.mockResolvedValue([]);
});

describe("SNine one-button home", () => {
  it("loads the active Minecraft account and selected profile", async () => {
    render(
      <I18nProvider localeSetting="en">
        <HomePage />
      </I18nProvider>,
    );

    await waitFor(() => {
      expect(screen.getByTestId("launcher-player-preview")).toHaveTextContent("PlayerOne");
    });
    // The one-button Home no longer renders the profile display name; selection is verified through the workspace store.

    await waitFor(() => {
      expect(useWorkspaceStore.getState().selectedProfileId).toBe("profile-one");
    });

    expect(
      screen.getByRole("button", { name: /Start SNine Client/i }),
    ).toBeEnabled();
  });

  it("starts the selected SNine profile through the guarded launch path", async () => {
    const user = userEvent.setup();

    render(
      <I18nProvider localeSetting="en">
        <HomePage />
      </I18nProvider>,
    );

    const launch = await screen.findByRole("button", {
      name: /Start SNine Client/i,
    });

    await user.click(launch);

    await waitFor(() => {
      expect(mocks.launch).toHaveBeenCalledWith("profile-one", 4096);
    });
  });

  it("creates and binds the default SNine profile on first launch", async () => {
    const user = userEvent.setup();

    mocks.list.mockResolvedValue([]);

    const createdProfile = {
      ...profile,
      id: "profile-created",
      displayName: "SNine Client",
      activeRevisionId: "revision-created",
      accountId: null,
    };

    mocks.create.mockResolvedValue(createdProfile);

    mocks.launch.mockResolvedValue({
      ...launchResult,
      profileId: "profile-created",
    });

    render(
      <I18nProvider localeSetting="en">
        <HomePage />
      </I18nProvider>,
    );

    const launch = await screen.findByRole("button", {
      name: /Start SNine Client/i,
    });

    await user.click(launch);

    await waitFor(() => {
      expect(mocks.create).toHaveBeenCalledWith("SNine Client");
      expect(mocks.assignProfileAccount).toHaveBeenCalledWith(
        "profile-created",
        "account-one",
      );
      expect(mocks.launch).toHaveBeenCalledWith("profile-created", 4096);
    });
  });

  it("does not allow a game launch without a Minecraft account", async () => {
    mocks.snapshot.mockResolvedValue({
      accounts: [],
      activeAccountId: null,
      offlinePolicy: {
        policy: "unconfigured",
        eligible: false,
        reason: "offline_policy_unconfigured",
      },
    });

    render(
      <I18nProvider localeSetting="en">
        <HomePage />
      </I18nProvider>,
    );

    const launch = await screen.findByRole("button", {
      name: /Start SNine Client/i,
    });

    expect(launch).toBeDisabled();
    expect(mocks.launch).not.toHaveBeenCalled();
  });
});
