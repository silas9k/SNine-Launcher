import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useShellStore } from "../../src/app/shellStore";
import { I18nProvider } from "../../src/i18n/I18nProvider";
import { authCommands } from "../../src/lib/authCommands";
import { instanceCommands, DEFAULT_INSTANCE_SETTINGS } from "../../src/lib/instanceCommands";
import { profileCommands } from "../../src/lib/profileCommands";
import { runtimeCommands } from "../../src/lib/runtimeCommands";
import { InstancesPage } from "../../src/pages/InstancesPage";
import { DEFAULT_SHELL_SETTINGS } from "../../src/theme/types";

const catalog = {
  minecraftVersions: [
    { version: "1.21.11", releaseType: "release" as const },
    { version: "26w10a", releaseType: "snapshot" as const },
    { version: "1.12.2", releaseType: "release" as const },
  ],
  fabricVersions: [{ version: "0.19.3", stable: true }],
  selectedMinecraftJavaMajor: 21 as const,
  neoforgeCapability: { capabilityId: "runtime.neoforge", state: "unconfigured" as const, reasonCode: "runtime_neoforge_pipeline_unavailable" },
  s9labComponentCapability: { capabilityId: "s9lab.components", state: "unconfigured" as const, reasonCode: "component_provider_origin_unconfigured" },
};

describe("instance manager", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useShellStore.setState({ settings: { ...DEFAULT_SHELL_SETTINGS }, initialized: true, loading: false });
    vi.spyOn(profileCommands, "list").mockResolvedValue([]);
    vi.spyOn(runtimeCommands, "launchStatuses").mockResolvedValue([]);
    vi.spyOn(runtimeCommands, "catalog").mockResolvedValue(catalog);
    vi.spyOn(authCommands, "snapshot").mockResolvedValue({
      accounts: [{ id: "account-a", username: "Silas9k", kind: "microsoft", sessionState: "active", ownershipVerifiedAtUnix: 1, lastOnlineAuthAtUnix: 1, addedAtUnix: 1, lastUsedAtUnix: 1 }],
      activeAccountId: "account-a",
      offlinePolicy: { policy: "unconfigured", eligible: false, reason: "offline_policy_unconfigured" },
    });
  });

  it("opens the real profile installer and exposes only safe loaders", async () => {
    const user = userEvent.setup();
    render(<I18nProvider localeSetting="en"><InstancesPage /></I18nProvider>);
    await user.click((await screen.findAllByRole("button", { name: /Install profile/i }))[0]);
    expect(await screen.findByRole("dialog", { name: "Install Minecraft profile" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /Vanilla/i })).toBeEnabled();
    expect(screen.getByRole("radio", { name: /Fabric/i })).toBeEnabled();
    expect(screen.getByRole("radio", { name: /^Forge\b/i })).toBeDisabled();
    expect(screen.getByRole("radio", { name: /^NeoForge\b/i })).toBeDisabled();
    expect(screen.getByRole("radio", { name: /^OptiFine\b/i })).toBeDisabled();
  });

  it("keeps snapshots and old releases hidden by default", async () => {
    const user = userEvent.setup();
    render(<I18nProvider localeSetting="en"><InstancesPage /></I18nProvider>);
    await user.click((await screen.findAllByRole("button", { name: /Install profile/i }))[0]);
    expect(await screen.findByRole("checkbox", { name: "Show snapshots" })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Show old versions" })).not.toBeChecked();
    expect(screen.queryByRole("option", { name: /26w10a/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /1.12.2/ })).not.toBeInTheDocument();
  });

  it("installs one profile and assigns the active Minecraft account", async () => {
    const user = userEvent.setup();
    const created = { id: "profile-new", displayName: "Minecraft 1.21.11", lifecycleState: "active" as const, activeRevisionId: "revision-1", accountId: null, favorite: false, verificationState: "verified" as const, sourceProfileId: null, createdAtUnix: 1, updatedAtUnix: 1 };
    const create = vi.spyOn(profileCommands, "create").mockResolvedValue(created);
    vi.spyOn(profileCommands, "trash").mockResolvedValue({ ...created, lifecycleState: "trash" });
    const save = vi.spyOn(instanceCommands, "saveSettings").mockResolvedValue({ ...DEFAULT_INSTANCE_SETTINGS });
    const assign = vi.spyOn(authCommands, "assignProfileAccount").mockResolvedValue();
    const install = vi.spyOn(runtimeCommands, "install").mockResolvedValue({ operationId: "install-1", profileId: created.id, revisionId: "revision-2", installState: "installed" });

    render(<I18nProvider localeSetting="en"><InstancesPage /></I18nProvider>);
    await user.click((await screen.findAllByRole("button", { name: /Install profile/i }))[0]);
    const dialog = await screen.findByRole("dialog", { name: "Install Minecraft profile" });
    const installButton = within(dialog).getByRole("button", { name: /^Install$/i });
    await waitFor(() => expect(installButton).toBeEnabled());
    await user.click(installButton);

    await waitFor(() => expect(install).toHaveBeenCalledTimes(1));
    expect(create).toHaveBeenCalledWith("Minecraft 1.21.11");
    expect(save).toHaveBeenCalledWith(created.id, expect.objectContaining({ maxRamMb: 4096 }));
    expect(assign).toHaveBeenCalledWith(created.id, "account-a");
    expect(install).toHaveBeenCalledWith(created.id, expect.objectContaining({ minecraftVersion: "1.21.11", loader: { kind: "vanilla" }, java: { mode: "managed", majorVersion: 21 } }));
  });
});
