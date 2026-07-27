import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../src/i18n/I18nProvider";

const mocks = vi.hoisted(() => ({
  list: vi.fn(),
  create: vi.fn(),
  duplicate: vi.fn(),
  archive: vi.fn(),
  trash: vi.fn(),
  restore: vi.fn(),
  setFavorite: vi.fn(),
  cachePreview: vi.fn(),
  quarantineUnreferenced: vi.fn(),
}));

vi.mock("../../src/lib/profileCommands", () => ({
  profileCommands: {
    ...mocks,
    cachePreview: mocks.cachePreview,
    quarantineUnreferenced: mocks.quarantineUnreferenced,
  },
}));

import { LibraryPage } from "../../src/pages/LibraryPage";

const profile = {
  id: "profile-fixture",
  displayName: "Isolated profile",
  lifecycleState: "active" as const,
  activeRevisionId: "revision-01234567",
  accountId: null,
  favorite: false,
  verificationState: "verified" as const,
  sourceProfileId: null,
  createdAtUnix: 1_900_000_000,
  updatedAtUnix: 1_900_000_000,
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.list.mockResolvedValue([]);
  mocks.create.mockResolvedValue(profile);
  mocks.archive.mockResolvedValue({ ...profile, lifecycleState: "archived" });
  mocks.setFavorite.mockResolvedValue({ ...profile, favorite: true });
  mocks.cachePreview.mockResolvedValue({
    scannedBlobs: 3,
    reachableBlobs: 2,
    eligibleForQuarantine: 1,
    eligibleBytes: 128,
    quarantinedThisRun: 0,
    restoredThisRun: 0,
    retainedInQuarantine: 0,
    deletionPolicy: "unconfigured",
  });
  mocks.quarantineUnreferenced.mockResolvedValue({
    scannedBlobs: 3,
    reachableBlobs: 2,
    eligibleForQuarantine: 0,
    eligibleBytes: 0,
    quarantinedThisRun: 1,
    restoredThisRun: 0,
    retainedInQuarantine: 1,
    deletionPolicy: "unconfigured",
  });
});

describe("phase 4 profile library", () => {
  it("creates an isolated profile through the typed profile command", async () => {
    const user = userEvent.setup();
    render(<I18nProvider localeSetting="en"><LibraryPage /></I18nProvider>);
    expect(await screen.findByText("No matching profiles")).toBeVisible();
    await user.click(screen.getAllByRole("button", { name: "Create profile" })[0]);
    const dialog = await screen.findByRole("dialog", { name: "Create a new profile" });
    await user.type(within(dialog).getByRole("textbox", { name: "Profile name" }), "Fresh profile");
    const confirm = within(dialog).getByRole("button", { name: "Create profile" });
    await waitFor(() => expect(confirm).toBeEnabled());
    await user.click(confirm);
    await waitFor(() => expect(mocks.create).toHaveBeenCalledWith("Fresh profile"));
    expect(mocks.list).toHaveBeenCalledTimes(2);
  });

  it("exposes archive and favorite lifecycle actions without deleting a profile", async () => {
    const user = userEvent.setup();
    mocks.list.mockResolvedValue([profile]);
    render(<I18nProvider localeSetting="en"><LibraryPage /></I18nProvider>);
    expect(await screen.findByRole("heading", { name: "Isolated profile" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Add favorite" }));
    await waitFor(() => expect(mocks.setFavorite).toHaveBeenCalledWith("profile-fixture", true));
    await user.click(screen.getByRole("button", { name: "Archive" }));
    await waitFor(() => expect(mocks.archive).toHaveBeenCalledWith("profile-fixture"));
    expect(mocks.trash).not.toHaveBeenCalled();
  });

  it("shows a storage preview and only allows recoverable cache quarantine", async () => {
    const user = userEvent.setup();
    render(<I18nProvider localeSetting="en"><LibraryPage /></I18nProvider>);
    expect(await screen.findByRole("heading", { name: "Storage overview" })).toBeVisible();
    expect(screen.getByText("Permanent deletion disabled")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Review cleanup" }));
    const dialog = await screen.findByRole("dialog", { name: "Quarantine unreferenced cache objects?" });
    await user.click(within(dialog).getByRole("button", { name: "Move to quarantine" }));
    await waitFor(() => expect(mocks.quarantineUnreferenced).toHaveBeenCalledOnce());
    expect(mocks.cachePreview).toHaveBeenCalledTimes(2);
  });
});
