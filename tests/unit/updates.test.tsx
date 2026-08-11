import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../src/i18n/I18nProvider";

const mocks = vi.hoisted(() => ({
  snapshot: vi.fn(),
  savePolicy: vi.fn(),
  preview: vi.fn(),
  createRestorePoint: vi.fn(),
  apply: vi.fn(),
  rollback: vi.fn(),
  restoreBackup: vi.fn(),
  runAutomatic: vi.fn(),
}));

vi.mock("../../src/lib/updateCommands", () => ({ updateCommands: mocks }));

import { UpdatesPage } from "../../src/pages/UpdatesPage";

const snapshot = {
  policy: { formatVersion: 1 as const, launcher: "manual" as const, profiles: "manual" as const, s9labComponent: "manual" as const, content: "manual" as const },
  channels: [
    { channel: "launcher" as const, mode: "manual" as const, state: "unconfigured" as const, reasonCode: "launcher_update_trust_not_configured" },
    { channel: "profiles" as const, mode: "manual" as const, state: "available" as const, reasonCode: null },
    { channel: "s9lab-component" as const, mode: "manual" as const, state: "unconfigured" as const, reasonCode: "s9lab_component_provider_unconfigured" },
    { channel: "content" as const, mode: "manual" as const, state: "available" as const, reasonCode: null },
  ],
  profiles: [{
    profileId: "profile-update",
    displayName: "Fabric Clean",
    activeRevisionId: "revision-current",
    revisions: [
      { revisionId: "revision-current", createdAtUnix: 1_900_000_000, active: true },
      { revisionId: "revision-previous", createdAtUnix: 1_899_000_000, active: false },
    ],
  }],
  restorePoints: [{
    backupId: "backup-safe",
    profileId: "profile-update",
    profileName: "Fabric Clean",
    sourceRevisionId: "revision-current",
    createdAtUnix: 1_900_000_000,
    fileCount: 3,
    sizeBytes: 1024,
  }],
};

const preview = {
  profileId: "profile-update",
  baseRevisionId: "revision-current",
  changes: [{
    channel: "content" as const,
    itemId: "sodium",
    displayName: "Sodium",
    currentVersion: "0.6.0",
    targetVersion: "0.6.1",
    verification: "modrinth-sha512-and-launcher-sha256",
  }],
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.snapshot.mockResolvedValue(snapshot);
  mocks.savePolicy.mockResolvedValue(snapshot);
  mocks.preview.mockResolvedValue(preview);
  mocks.createRestorePoint.mockResolvedValue(snapshot.restorePoints[0]);
  mocks.apply.mockResolvedValue({ operationId: "operation", profileId: "profile-update", revisionId: "revision-next", restorePointId: "backup-safe", appliedChanges: ["sodium"] });
  mocks.rollback.mockResolvedValue({ operationId: "rollback", profileId: "profile-update", revisionId: "revision-rollback", restorePointId: "backup-new", appliedChanges: ["rollback:revision-previous"] });
  mocks.restoreBackup.mockResolvedValue({ id: "profile-copy" });
  mocks.runAutomatic.mockResolvedValue([]);
});

function renderPage() {
  return render(<I18nProvider localeSetting="en"><UpdatesPage /></I18nProvider>);
}

describe("phase 7 update and recovery center", () => {
  it("previews verified changes and applies only the selected update", async () => {
    const user = userEvent.setup();
    renderPage();
    await screen.findByRole("heading", { name: "Update plan" });
    await user.click(screen.getByRole("button", { name: "Check now" }));
    expect(await screen.findByText("Sodium")).toBeVisible();
    expect(screen.getByText("0.6.0")).toBeVisible();
    expect(screen.getByText("0.6.1")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Update selection" }));
    await waitFor(() => expect(mocks.apply).toHaveBeenCalledWith("profile-update", ["sodium"]));
  });

  it("requires confirmation for rollback and always delegates to the typed recovery command", async () => {
    const user = userEvent.setup();
    renderPage();
    const restore = await screen.findByRole("button", { name: "Restore" });
    await user.click(restore);
    const dialog = screen.getByRole("dialog", { name: "Return to this revision?" });
    await user.click(within(dialog).getByRole("button", { name: "Back up & restore" }));
    await waitFor(() => expect(mocks.rollback).toHaveBeenCalledWith("profile-update", "revision-previous"));
  });

  it("offers selective restore as a new profile and has no serious accessibility violations", async () => {
    const user = userEvent.setup();
    renderPage();
    const backup = await screen.findByRole("button", { name: /3 user files/ });
    await user.click(backup);
    const dialog = screen.getByRole("dialog", { name: "Restore backup as a new profile" });
    await user.click(within(dialog).getByRole("checkbox", { name: /Include appearance settings/ }));
    await user.click(within(dialog).getByRole("button", { name: "Create as new profile" }));
    await waitFor(() => expect(mocks.restoreBackup).toHaveBeenCalledWith(
      "backup-safe",
      "Fabric Clean · Recovered",
      { includeAccount: true, includeSettings: true, includeFiles: true },
    ));
    const results = await axe.run(document, {
      runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21aa"] },
      rules: { "color-contrast": { enabled: false } },
    });
    expect(results.violations.filter((violation) => violation.impact === "serious" || violation.impact === "critical")).toEqual([]);
  });
});
