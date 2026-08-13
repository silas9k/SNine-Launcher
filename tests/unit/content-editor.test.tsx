import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import axe from "axe-core";
import { I18nProvider } from "../../src/i18n/I18nProvider";

const mocks = vi.hoisted(() => ({
  snapshot: vi.fn(),
  checkUpdates: vi.fn(),
  search: vi.fn(),
  project: vi.fn(),
  install: vi.fn(),
  setEnabled: vi.fn(),
  remove: vi.fn(),
  update: vi.fn(),
  addLocal: vi.fn(),
  importModrinthPack: vi.fn(),
  exportProfile: vi.fn(),
  importProfile: vi.fn(),
}));

vi.mock("../../src/lib/contentCommands", async (importOriginal) => {
  const original = await importOriginal<typeof import("../../src/lib/contentCommands")>();
  return { ...original, contentCommands: mocks };
});

import { ContentEditor } from "../../src/components/content/ContentEditor";

const profile = {
  id: "profile-content",
  displayName: "Clean Fabric",
  lifecycleState: "active" as const,
  activeRevisionId: "revision-content",
  accountId: null,
  favorite: true,
  verificationState: "verified" as const,
  sourceProfileId: null,
  createdAtUnix: 1_900_000_000,
  updatedAtUnix: 1_900_000_000,
};

const available = { capabilityId: "content.local", state: "available" as const, reasonCode: "available" };
const snapshot = {
  profileId: profile.id,
  minecraftVersion: "1.21.1",
  loader: "fabric" as const,
  lockSha256: "a".repeat(64),
  localFileCapability: available,
  profileFormatCapability: available,
  content: [{
    contentId: "content-sodium",
    projectId: "project-sodium",
    versionId: "version-sodium-current",
    displayName: "Sodium",
    versionNumber: "0.6.0",
    contentType: "mod" as const,
    source: "modrinth" as const,
    enabled: true,
    managedByPack: false,
    sizeBytes: 2048,
    sha256: "b".repeat(64),
    dependencies: [{
      projectId: "fabric-api",
      displayName: "Fabric API",
      relation: "required" as const,
      satisfied: true,
    }],
    conflicts: [],
    update: { versionId: "version-sodium-next", versionNumber: "0.6.1" },
  }],
};

const operation = {
  operationId: "operation-content",
  profileId: profile.id,
  revisionId: "revision-next",
  changedContentIds: ["content-sodium"],
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.snapshot.mockResolvedValue(snapshot);
  mocks.checkUpdates.mockResolvedValue(snapshot);
  mocks.setEnabled.mockResolvedValue(operation);
  mocks.remove.mockResolvedValue(operation);
  mocks.update.mockResolvedValue(operation);
  mocks.install.mockResolvedValue(operation);
  mocks.addLocal.mockResolvedValue(operation);
  mocks.importModrinthPack.mockResolvedValue(operation);
  mocks.exportProfile.mockResolvedValue({
    operationId: "export-operation",
    profileId: profile.id,
    displayName: profile.displayName,
    fileName: "clean-fabric.s9profile",
  });
  mocks.search.mockResolvedValue({
    capability: { capabilityId: "content.modrinth", state: "available", reasonCode: "available" },
    total: 1,
    offset: 0,
    hits: [{
      projectId: "project-lithium",
      slug: "lithium",
      title: "Lithium",
      description: "Verified performance mod",
      contentType: "mod",
      author: "CaffeineMC",
      downloads: 42_000,
      follows: 4_200,
      iconUrl: "https://cdn.modrinth.com/data/project-lithium/icon.png",
      updatedAtUnix: 1_900_000_000,
      latestVersion: "0.14.0",
    }],
  });
  mocks.project.mockResolvedValue({
    projectId: "project-lithium",
    slug: "lithium",
    title: "Lithium",
    description: "Verified performance mod",
    contentType: "mod",
    author: "CaffeineMC",
    license: "LGPL-3.0",
    iconUrl: "https://cdn.modrinth.com/data/project-lithium/icon.png",
    downloads: 42_000,
    followers: 4_200,
    updatedAtUnix: 1_900_000_000,
    categories: ["optimization"],
    versions: [{
      versionId: "version-lithium",
      versionNumber: "0.14.0",
      name: "Lithium 0.14.0",
      publishedAtUnix: 1_900_000_000,
      compatible: true,
      incompatibilityReason: null,
      dependencies: [],
      conflicts: [],
    }],
  });
});

function renderEditor() {
  return render(
    <I18nProvider localeSetting="en">
      <ContentEditor profiles={[profile]} />
    </I18nProvider>,
  );
}

describe("phase 6 content editor", () => {
  it("has no serious or critical accessibility violations", async () => {
    renderEditor();
    await screen.findByRole("heading", { name: "Sodium" });
    const results = await axe.run(document, {
      runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21aa"] },
      rules: { "color-contrast": { enabled: false } },
    });
    expect(results.violations.filter((violation) =>
      violation.impact === "serious" || violation.impact === "critical"
    )).toEqual([]);
  });

  it("shows the reproducible inventory and changes enablement only through the typed command", async () => {
    const user = userEvent.setup();
    renderEditor();

    expect(await screen.findByRole("heading", { name: "Sodium" })).toBeVisible();
    expect(screen.getByText("Fabric API")).toBeVisible();
    expect(screen.getAllByText("No conflicts").length).toBeGreaterThan(0);
    await user.click(screen.getByRole("switch", { name: /^Enabled/ }));

    await waitFor(() => expect(mocks.setEnabled).toHaveBeenCalledWith(
      "profile-content",
      "content-sodium",
      false,
    ));
    expect(await screen.findByText("The content was disabled in a new verified revision.")).toBeVisible();
    expect(mocks.snapshot).toHaveBeenCalledTimes(2);
  });

  it("scopes Modrinth search to the profile and installs only the selected compatible version", async () => {
    const user = userEvent.setup();
    renderEditor();
    await screen.findByRole("heading", { name: "Sodium" });
    await user.click(screen.getByRole("tab", { name: "Discover" }));
    await user.type(screen.getByRole("searchbox", { name: "Search Modrinth" }), "lithium");
    await user.click(screen.getByRole("button", { name: "Search" }));

    await waitFor(() => expect(mocks.search).toHaveBeenCalledWith({
      query: "lithium",
      contentType: "mod",
      minecraftVersion: "1.21.1",
      loader: "fabric",
      offset: 0,
      limit: 20,
    }));
    const resultTitle = await screen.findByText("Lithium");
    await user.click(resultTitle.closest("button")!);
    await waitFor(() => expect(mocks.project).toHaveBeenCalledWith(
      "profile-content",
      "project-lithium",
    ));
    expect(await screen.findByRole("heading", { name: "Lithium" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Install verified" }));
    await waitFor(() => expect(mocks.install).toHaveBeenCalledWith(
      "profile-content",
      "project-lithium",
      "version-lithium",
    ));
  });

  it("reports a rejected local file action without displaying a success state", async () => {
    const user = userEvent.setup();
    mocks.addLocal.mockRejectedValue({
      code: "content_import_invalid",
      messageKey: "error.content_import_invalid",
      params: {},
    });
    const view = renderEditor();
    await screen.findByRole("heading", { name: "Sodium" });

    const input = view.container.querySelector<HTMLInputElement>('input[accept=".jar"]');
    expect(input).not.toBeNull();
    const file = new File(["not a jar"], "unsafe.jar", { type: "application/java-archive" });
    fireEvent.change(input!, { target: { files: [file] } });

    expect(await screen.findByText("The import archive is invalid or failed a security check.")).toBeVisible();
    expect(screen.queryByText("The local file was verified, hashed and added to the new revision.")).not.toBeInTheDocument();
  });

  it("allows the verified resolver to install required Modrinth dependencies", async () => {
    const user = userEvent.setup();
    const detailWithDependency = await mocks.project();
    mocks.project.mockResolvedValue({
      ...detailWithDependency,
      versions: [{
        ...detailWithDependency.versions[0],
        dependencies: [{
          projectId: "fabric-api",
          displayName: "Fabric API",
          relation: "required",
          satisfied: false,
        }],
      }],
    });
    mocks.project.mockClear();
    renderEditor();
    await screen.findByRole("heading", { name: "Sodium" });
    await user.click(screen.getByRole("tab", { name: "Discover" }));
    await user.type(screen.getByRole("searchbox", { name: "Search Modrinth" }), "lithium");
    await user.click(screen.getByRole("button", { name: "Search" }));
    await user.click(await screen.findByText("Lithium"));

    const install = await screen.findByRole("button", { name: "Install verified" });
    expect(install).toBeEnabled();
    await user.click(install);
    await waitFor(() => expect(mocks.install).toHaveBeenCalledWith(
      "profile-content",
      "project-lithium",
      "version-lithium",
    ));
  });

  it("keeps pack-managed members visibly read-only for update and removal", async () => {
    const managedSnapshot = {
      ...snapshot,
      content: [{ ...snapshot.content[0], managedByPack: true, update: null }],
    };
    mocks.snapshot.mockResolvedValue(managedSnapshot);
    mocks.checkUpdates.mockResolvedValue(managedSnapshot);
    renderEditor();

    expect((await screen.findAllByText("Managed by modpack")).length).toBeGreaterThan(0);
    expect(screen.getByRole("button", { name: "Check for updates" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Remove" })).toBeDisabled();
    expect(mocks.update).not.toHaveBeenCalled();
    expect(mocks.remove).not.toHaveBeenCalled();
  });

  it("checks update metadata without attempting a failing update when none is advertised", async () => {
    const user = userEvent.setup();
    const currentSnapshot = {
      ...snapshot,
      content: [{ ...snapshot.content[0], update: null }],
    };
    mocks.snapshot.mockResolvedValue(currentSnapshot);
    mocks.checkUpdates.mockResolvedValue(currentSnapshot);
    renderEditor();

    await user.click(await screen.findByRole("button", { name: "Check for updates" }));
    await waitFor(() => expect(mocks.checkUpdates).toHaveBeenCalled());
    expect(mocks.update).not.toHaveBeenCalled();
  });

  it("requires confirmation before removing an installed entry", async () => {
    const user = userEvent.setup();
    renderEditor();
    await screen.findByRole("heading", { name: "Sodium" });
    await user.click(screen.getByRole("button", { name: "Remove" }));
    const dialog = screen.getByRole("dialog", { name: "Remove content from this profile?" });
    await user.click(within(dialog).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(mocks.remove).toHaveBeenCalledWith("profile-content", "content-sodium"));
  });
});
