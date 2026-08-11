import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../src/i18n/I18nProvider";

const mocks = vi.hoisted(() => ({
  snapshot: vi.fn(),
  startDeviceLogin: vi.fn(),
  completeDeviceLogin: vi.fn(),
  cancelDeviceLogin: vi.fn(),
  openMicrosoftVerification: vi.fn(),
}));

vi.mock("../../src/lib/authCommands", () => ({
  authCommands: {
    snapshot: mocks.snapshot,
    startDeviceLogin: mocks.startDeviceLogin,
    completeDeviceLogin: mocks.completeDeviceLogin,
    cancelDeviceLogin: mocks.cancelDeviceLogin,
    refreshAccount: vi.fn(),
    selectAccount: vi.fn(),
    removeAccount: vi.fn(),
    assignProfileAccount: vi.fn(),
  },
  openMicrosoftVerification: mocks.openMicrosoftVerification,
}));

import { AccountsPage } from "../../src/pages/AccountsPage";

beforeEach(() => {
  vi.clearAllMocks();
  mocks.snapshot.mockResolvedValue({
    accounts: [],
    activeAccountId: null,
    offlinePolicy: {
      policy: "unconfigured",
      eligible: false,
      reason: "offline_policy_unconfigured",
    },
  });
});

describe("phase 3 account authentication", () => {
  it("fails closed while the offline policy is unconfigured", async () => {
    render(<I18nProvider localeSetting="en"><AccountsPage /></I18nProvider>);
    expect(await screen.findByText("Offline launches remain blocked until a maximum validity period is explicitly approved.")).toBeVisible();
    expect(await screen.findByText("No account connected")).toBeVisible();
    expect(screen.getByRole("heading", { name: "S9Lab Cloud Sync" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Link S9Lab account" })).toBeDisabled();
    expect(mocks.snapshot).toHaveBeenCalledOnce();
  });

  it("shows only the public device prompt and completes it by opaque login id", async () => {
    const user = userEvent.setup();
    mocks.startDeviceLogin.mockResolvedValue({
      loginId: "login-fixture",
      userCode: "ABCD-EFGH",
      verificationUri: "https://microsoft.com/link",
      expiresAtUnix: 2_000_000_000,
      intervalSeconds: 5,
    });
    mocks.completeDeviceLogin.mockResolvedValue({
      id: "0123456789abcdef0123456789abcdef",
      username: "VerifiedPlayer",
      kind: "microsoft",
      sessionState: "active",
      ownershipVerifiedAtUnix: 1_900_000_000,
      lastOnlineAuthAtUnix: 1_900_000_000,
      addedAtUnix: 1_900_000_000,
      lastUsedAtUnix: 1_900_000_000,
    });

    render(<I18nProvider localeSetting="en"><AccountsPage /></I18nProvider>);
    await screen.findByText("No account connected");
    await user.click(screen.getAllByRole("button", { name: "Connect Microsoft account" })[0]);
    expect(await screen.findByRole("dialog", { name: "Sign in with Microsoft" })).toBeVisible();
    expect(screen.getByText("ABCD-EFGH")).toBeVisible();
    expect(document.body.textContent).not.toContain(["device", "-secret-fixture"].join(""));

    await user.click(screen.getByRole("button", { name: "I completed sign-in" }));
    await waitFor(() => expect(mocks.completeDeviceLogin).toHaveBeenCalledWith("login-fixture"));
    expect(mocks.snapshot).toHaveBeenCalledTimes(2);
  });
});
