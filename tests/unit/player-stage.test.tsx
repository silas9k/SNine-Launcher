import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../src/i18n/I18nProvider";

const mocks = vi.hoisted(() => ({
  viewers: [] as Array<Record<string, any>>,
  loadCape: vi.fn(),
  snapshot: vi.fn(),
}));

vi.mock("../../src/lib/authCommands", () => ({
  authCommands: {
    snapshot: mocks.snapshot,
  },
}));

vi.mock("skinview3d", () => {
  class Animation { speed = 1; }
  class SkinViewer {
    width = 0;
    height = 0;
    background: unknown = null;
    nameTag: unknown = null;
    animation: unknown = null;
    controls = { enableZoom: true, enablePan: true, enableDamping: false, dampingFactor: 0 };
    cameraLight = { intensity: 0 };
    globalLight = { intensity: 0 };
    playerObject = {
      rotation: { y: 0, set: vi.fn() },
      resetJoints: vi.fn(),
      skin: { modelType: "default", setOuterLayerVisible: vi.fn() },
    };
    loadSkin = vi.fn().mockResolvedValue(undefined);
    loadCape = mocks.loadCape.mockResolvedValue(undefined);
    resetCameraPose = vi.fn();
    dispose = vi.fn();
    constructor() { mocks.viewers.push(this as unknown as Record<string, any>); }
  }
  return {
    SkinViewer,
    IdleAnimation: Animation,
    WalkingAnimation: Animation,
    WaveAnimation: Animation,
  };
});

import { PlayerStage } from "../../src/components/player/PlayerStage";

beforeEach(() => {
  vi.stubGlobal("WebGLRenderingContext", class WebGLRenderingContext {});
  mocks.viewers.length = 0;
  mocks.loadCape.mockClear();
  mocks.snapshot.mockResolvedValue({
    accounts: [{ id: "account-one", username: "LocalPlayer" }],
    activeAccountId: "account-one",
    offlinePolicy: { policy: "unconfigured", eligible: false, reason: "offline_policy_unconfigured" },
  });
});

describe("phase 9 integrated player", () => {
  it("renders locally, disables zoom and exposes keyboard and explicit view controls", async () => {
    const user = userEvent.setup();
    render(<I18nProvider localeSetting="en"><PlayerStage /></I18nProvider>);
    expect(await screen.findByText("Ready locally")).toBeVisible();
    expect((await screen.findAllByText("LocalPlayer"))[0]).toBeVisible();
    const viewer = mocks.viewers[0] as any;
    expect(viewer.controls.enableZoom).toBe(false);
    expect(viewer.controls.enablePan).toBe(false);

    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(viewer.playerObject.rotation.y).toBe(Math.PI);
    const canvas = screen.getByLabelText("Interactive 3D preview for LocalPlayer");
    fireEvent.keyDown(canvas, { key: "ArrowLeft" });
    expect(viewer.playerObject.rotation.y).toBeLessThan(Math.PI);
    await user.click(screen.getByRole("button", { name: "Reset camera" }));
    expect(viewer.resetCameraPose).toHaveBeenCalledOnce();
  });

  it("switches skin layers and owned-preview back equipment without loading remote assets", async () => {
    const user = userEvent.setup();
    render(<I18nProvider localeSetting="en"><PlayerStage /></I18nProvider>);
    await screen.findByText("Ready locally");
    const viewer = mocks.viewers[0] as any;
    await user.click(screen.getByRole("switch", { name: "Skin layers" }));
    expect(viewer.playerObject.skin.setOuterLayerVisible).toHaveBeenLastCalledWith(false);
    await user.click(screen.getByRole("button", { name: "Wings" }));
    await waitFor(() => expect(mocks.loadCape).toHaveBeenLastCalledWith(expect.stringMatching(/^data:image\/svg\+xml/), { backEquipment: "elytra" }));
  });
});
