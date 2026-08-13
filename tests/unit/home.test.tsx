import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../src/i18n/I18nProvider";
import { useWorkspaceStore } from "../../src/app/workspaceStore";
import type {
  Phase5ComponentCatalog,
  Phase5RuntimeCatalog,
  Phase5RuntimeStatus,
} from "../../src/lib/generated/ipc-contracts";

const mocks = vi.hoisted(() => ({
  list: vi.fn(),
  catalog: vi.fn(),
  componentCatalog: vi.fn(),
  status: vi.fn(),
  install: vi.fn(),
  repair: vi.fn(),
  launch: vi.fn(),
  stop: vi.fn(),
  launchStatuses: vi.fn(),
  setComponent: vi.fn(),
}));

vi.mock("../../src/lib/profileCommands", () => ({
  profileCommands: {
    list: mocks.list,
  },
}));

vi.mock("../../src/lib/runtimeCommands", () => ({
  runtimeCommands: {
    catalog: mocks.catalog,
    componentCatalog: mocks.componentCatalog,
    status: mocks.status,
    install: mocks.install,
    repair: mocks.repair,
    launch: mocks.launch,
    stop: mocks.stop,
    launchStatuses: mocks.launchStatuses,
    setComponent: mocks.setComponent,
  },
}));

import { HomePage } from "../../src/pages/HomePage";

const profiles = [
  {
    id: "profile-one",
    displayName: "Fabric profile",
    lifecycleState: "active" as const,
    activeRevisionId: "revision-00000001",
    accountId: "account-one",
    favorite: false,
    verificationState: "verified" as const,
    sourceProfileId: null,
    createdAtUnix: 1_900_000_000,
    updatedAtUnix: 1_900_000_000,
  },
  {
    id: "profile-two",
    displayName: "Vanilla profile",
    lifecycleState: "active" as const,
    activeRevisionId: "revision-00000002",
    accountId: null,
    favorite: false,
    verificationState: "verified" as const,
    sourceProfileId: null,
    createdAtUnix: 1_900_000_001,
    updatedAtUnix: 1_900_000_001,
  },
];

const unavailableCapability = {
  capabilityId: "runtime.unavailable",
  state: "unconfigured" as const,
  reasonCode: "provider_unconfigured",
};

const catalog: Phase5RuntimeCatalog = {
  minecraftVersions: [
    { version: "1.21.4", releaseType: "release" },
    { version: "25w02a", releaseType: "snapshot" },
  ],
  fabricVersions: [],
  neoforgeCapability: {
    capabilityId: "runtime.neoforge",
    state: "unconfigured",
    reasonCode: "runtime_neoforge_installation_pipeline_unavailable",
  },
  s9labComponentCapability: {
    capabilityId: "s9lab.components",
    state: "unconfigured",
    reasonCode: "component_provider_origin_unconfigured",
  },
};

function runtimeStatus(
  overrides: Partial<Phase5RuntimeStatus> = {},
): Phase5RuntimeStatus {
  return {
    profileId: "profile-one",
    activeRevisionId: "revision-00000001",
    lifecycleState: "active",
    installState: "not-configured",
    runtime: null,
    component: null,
    launches: [],
    s9labComponentCapability: catalog.s9labComponentCapability,
    ...overrides,
  };
}

const installedRuntime = {
  minecraftVersion: "1.21.4",
  loader: {
    kind: "fabric" as const,
    loaderVersion: "0.16.10",
  },
  java: {
    mode: "managed" as const,
    majorVersion: 21 as const,
  },
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.list.mockResolvedValue(profiles);
  mocks.catalog.mockImplementation(async (minecraftVersion?: string) => ({
    ...catalog,
    fabricVersions: minecraftVersion
      ? [{ version: "0.16.10", stable: true }]
      : [],
  }));
  mocks.status.mockResolvedValue(runtimeStatus());
  mocks.componentCatalog.mockResolvedValue({
    capability: catalog.s9labComponentCapability,
    entries: [],
  } satisfies Phase5ComponentCatalog);
  mocks.install.mockResolvedValue({
    operationId: "op-install",
    profileId: "profile-one",
    revisionId: "revision-runtime",
    installState: "installed",
  });
  mocks.repair.mockResolvedValue({
    operationId: "op-repair",
    profileId: "profile-one",
    revisionId: "revision-repair",
    installState: "installed",
  });
  mocks.setComponent.mockResolvedValue({
    operationId: "op-component",
    profileId: "profile-one",
    revisionId: "revision-component",
    installState: "installed",
  });
  useWorkspaceStore.setState({ selectedProfileId: null });
});

describe("home workspace", () => {
  it("loads real profiles and their runtime before showing state, then supports listbox arrows", async () => {
    const user = userEvent.setup();
    render(<I18nProvider localeSetting="en"><HomePage /></I18nProvider>);
    expect(screen.getByRole("status", { name: "Preparing SNine Launcher …" })).toBeInTheDocument();

    const first = await screen.findByRole("option", { name: /Fabric profile/ });
    const second = screen.getByRole("option", { name: /Vanilla profile/ });
    expect(first).toHaveAttribute("aria-selected", "true");
    expect(useWorkspaceStore.getState().selectedProfileId).toBe("profile-one");
    await waitFor(() => expect(mocks.status).toHaveBeenCalledWith("profile-one"));
    expect(await screen.findByText("Not configured")).toBeVisible();

    first.focus();
    await user.keyboard("{ArrowDown}");
    expect(second).toHaveFocus();
    expect(second).toHaveAttribute("aria-selected", "true");
    expect(useWorkspaceStore.getState().selectedProfileId).toBe("profile-two");
    await waitFor(() => expect(mocks.status).toHaveBeenCalledWith("profile-two"));
  });

  it("shows a retryable error instead of pretending that a failed profile request is empty", async () => {
    mocks.list.mockRejectedValueOnce(new Error("offline")).mockResolvedValueOnce(profiles);
    const user = userEvent.setup();
    render(<I18nProvider localeSetting="en"><HomePage /></I18nProvider>);

    expect(await screen.findByRole("alert")).toHaveTextContent("The interface could not be loaded");
    expect(screen.queryByText("No profiles yet")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("option", { name: /Fabric profile/ })).toBeVisible();
    await waitFor(() => expect(mocks.list).toHaveBeenCalledTimes(2));
  });

  it("installs a controlled Fabric runtime with an explicit Java policy", async () => {
    const user = userEvent.setup();
    render(<I18nProvider localeSetting="en"><HomePage /></I18nProvider>);

    await screen.findByText("Not configured");
    await waitFor(() =>
      expect(screen.getByRole("combobox", { name: "Minecraft" })).toHaveValue("1.21.4")
    );
    await user.selectOptions(screen.getByRole("combobox", { name: "Mod loader" }), "fabric");
    const loader = await screen.findByRole("combobox", { name: "Loader version" });
    await waitFor(() => expect(within(loader).getByRole("option", { name: "0.16.10" })).toBeInTheDocument());
    await user.selectOptions(loader, "0.16.10");
    await user.selectOptions(screen.getByRole("combobox", { name: "Java source" }), "system");
    await user.selectOptions(screen.getByRole("combobox", { name: "Java" }), "17");
    await user.click(screen.getByRole("button", { name: "Install safely" }));

    await waitFor(() => expect(mocks.install).toHaveBeenCalledWith(
      "profile-one",
      {
        minecraftVersion: "1.21.4",
        loader: { kind: "fabric", loaderVersion: "0.16.10" },
        java: { mode: "system", majorVersion: 17 },
      },
      { mode: "disabled" },
    ));
  });

  it("offers only verified compatible component releases and installs the selected entry", async () => {
    const availableCapability = {
      capabilityId: "s9lab.components",
      state: "available" as const,
      reasonCode: "",
    };
    mocks.catalog.mockImplementation(async (minecraftVersion?: string) => ({
      ...catalog,
      fabricVersions: minecraftVersion
        ? [{ version: "0.16.10", stable: true }]
        : [],
      s9labComponentCapability: availableCapability,
    }));
    mocks.status.mockResolvedValue(runtimeStatus({
      s9labComponentCapability: availableCapability,
    }));
    mocks.componentCatalog.mockImplementation(async (intent) => ({
      capability: availableCapability,
      entries: intent.loader.kind === "fabric" && intent.loader.loaderVersion === "0.16.10"
        ? [
            {
              componentId: "s9lab_client",
              componentVersion: "1.0.7",
              minecraftVersion: "1.21.4",
              loader: { kind: "fabric", loaderVersion: "0.16.10" },
              sizeBytes: 4095,
              sha256: "a".repeat(64),
            },
            {
              componentId: "s9lab_client",
              componentVersion: "1.0.8",
              minecraftVersion: "1.21.4",
              loader: { kind: "fabric", loaderVersion: "0.16.10" },
              sizeBytes: 4096,
              sha256: "b".repeat(64),
            },
          ]
        : [],
    } satisfies Phase5ComponentCatalog));
    const user = userEvent.setup();
    render(<I18nProvider localeSetting="en"><HomePage /></I18nProvider>);

    await screen.findByText("Not configured");
    await waitFor(() =>
      expect(screen.getByRole("combobox", { name: "Minecraft" })).toHaveValue("1.21.4")
    );
    await user.selectOptions(screen.getByRole("combobox", { name: "Mod loader" }), "fabric");
    const loader = await screen.findByRole("combobox", { name: "Loader version" });
    await waitFor(() =>
      expect(within(loader).getByRole("option", { name: "0.16.10" })).toBeInTheDocument()
    );
    await user.selectOptions(loader, "0.16.10");

    const componentSwitch = screen.getByRole("switch", { name: /Use SNine component/ });
    await waitFor(() => expect(componentSwitch).toBeEnabled());
    await user.click(componentSwitch);
    expect(screen.queryByRole("textbox", { name: "Component ID" })).not.toBeInTheDocument();
    const componentId = screen.getByRole("combobox", { name: "Component ID" });
    const componentVersion = screen.getByRole("combobox", { name: "Component version" });
    expect(within(componentId).getAllByRole("option")).toHaveLength(1);
    expect(within(componentVersion).getAllByRole("option")).toHaveLength(2);
    await user.selectOptions(componentVersion, "1.0.8");
    await user.click(screen.getByRole("button", { name: "Install safely" }));

    await waitFor(() => expect(mocks.componentCatalog).toHaveBeenCalledWith({
      minecraftVersion: "1.21.4",
      loader: { kind: "fabric", loaderVersion: "0.16.10" },
      java: { mode: "system", majorVersion: 21 },
    }));
    expect(mocks.install).toHaveBeenCalledWith(
      "profile-one",
      {
        minecraftVersion: "1.21.4",
        loader: { kind: "fabric", loaderVersion: "0.16.10" },
        java: { mode: "system", majorVersion: 21 },
      },
      {
        mode: "catalog",
        componentId: "s9lab_client",
        componentVersion: "1.0.8",
      },
    );
  });

  it("starts and stops only the returned launch instance", async () => {
    mocks.status.mockResolvedValue(runtimeStatus({
      installState: "installed",
      runtime: installedRuntime,
    }));
    mocks.launch.mockResolvedValue({
      launchId: "launch-target",
      profileId: "profile-one",
      state: "running",
      processId: 4832,
      accountName: "PlayerOne",
      startedAtUnix: 1_900_000_100,
      exitCode: null,
    });
    mocks.stop.mockResolvedValue({
      launchId: "launch-target",
      profileId: "profile-one",
      state: "exited",
      processId: null,
      accountName: "PlayerOne",
      startedAtUnix: 1_900_000_100,
      exitCode: 0,
    });
    const user = userEvent.setup();
    render(<I18nProvider localeSetting="en"><HomePage /></I18nProvider>);

    await user.click(await screen.findByRole("button", { name: "Launch Minecraft" }));
    await waitFor(() => expect(mocks.launch).toHaveBeenCalledWith("profile-one", 4096));
    expect(await screen.findByText("Signed in as PlayerOne")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Stop this instance" }));
    await waitFor(() => expect(mocks.stop).toHaveBeenCalledWith("launch-target"));
  });

  it("offers an explicit repair without silently launching a damaged revision", async () => {
    mocks.status
      .mockResolvedValueOnce(runtimeStatus({
        installState: "repair-required",
        runtime: installedRuntime,
      }))
      .mockResolvedValue(runtimeStatus({
        installState: "installed",
        runtime: installedRuntime,
      }));
    const user = userEvent.setup();
    render(<I18nProvider localeSetting="en"><HomePage /></I18nProvider>);

    expect(await screen.findByText("Repair required")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Launch Minecraft" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Repair profile" }));
    await waitFor(() => expect(mocks.repair).toHaveBeenCalledWith("profile-one"));
    expect(await screen.findByRole("button", { name: "Launch Minecraft" })).toBeEnabled();
  });

  it("removes an installed S9Lab component through the targeted component command", async () => {
    mocks.status.mockResolvedValue(runtimeStatus({
      installState: "installed",
      runtime: installedRuntime,
      component: {
        componentId: "s9lab-client",
        componentVersion: "1.0.8",
      },
    }));
    const user = userEvent.setup();
    render(<I18nProvider localeSetting="en"><HomePage /></I18nProvider>);

    expect(await screen.findByText("s9lab-client")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Remove" }));
    await waitFor(() =>
      expect(mocks.setComponent).toHaveBeenCalledWith("profile-one", { mode: "disabled" })
    );
  });

  it("explains the account requirement and keeps launch disabled", async () => {
    mocks.list.mockResolvedValue([{ ...profiles[0], accountId: null }]);
    mocks.status.mockResolvedValue(runtimeStatus({
      installState: "installed",
      runtime: installedRuntime,
    }));
    render(<I18nProvider localeSetting="en"><HomePage /></I18nProvider>);

    expect(await screen.findByText("Assign a verified Minecraft account to this profile.")).toBeVisible();
    expect(screen.getByRole("button", { name: "Launch Minecraft" })).toBeDisabled();
    expect(mocks.launch).not.toHaveBeenCalled();
  });

  it("keeps unconfigured NeoForge and S9Lab capabilities visibly disabled", async () => {
    mocks.catalog.mockResolvedValue({
      ...catalog,
      neoforgeCapability: unavailableCapability,
      s9labComponentCapability: unavailableCapability,
    });
    mocks.status.mockResolvedValue(runtimeStatus({
      s9labComponentCapability: unavailableCapability,
    }));
    render(<I18nProvider localeSetting="en"><HomePage /></I18nProvider>);

    await screen.findByText("Not configured");
    expect(screen.getByRole("option", { name: "NeoForge" })).toBeDisabled();
    expect(screen.getByRole("switch", { name: /Use SNine component/ })).toBeDisabled();
    expect(screen.getByText(/No trusted component provider is configured/)).toBeVisible();
    expect(screen.getByText(/NeoForge remains disabled/)).toBeVisible();
    expect(mocks.componentCatalog).not.toHaveBeenCalled();
  });
});
