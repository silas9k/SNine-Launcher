import { act, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  load: vi.fn(),
  head: vi.fn(),
  loadImage: vi.fn(),
  renderers: [] as Array<Record<string, ReturnType<typeof vi.fn>>>,
}));

vi.mock("../../src/lib/minecraftSkinCache", () => ({
  loadMinecraftSkin: mocks.load,
  minecraftHeadFromSnapshot: mocks.head,
}));

vi.mock("../../src/components/player/launcherSkinRenderer", () => ({
  loadSkinImage: mocks.loadImage,
  LauncherSkinRenderer: class {
    setCameraPreset = vi.fn();
    setViewportVisible = vi.fn();
    setSkin = vi.fn();
    setCosmetics = vi.fn();
    setReducedMotion = vi.fn();
    setNametagAnchorListener = vi.fn();
    dispose = vi.fn();
    constructor() { mocks.renderers.push(this as unknown as Record<string, ReturnType<typeof vi.fn>>); }
  },
}));

import { LauncherSkinPreview } from "../../src/components/player/LauncherSkinPreview";
import { launcherBadgeIconUrl } from "../../src/lib/snineClientBridge";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => { resolve = yes; });
  return { promise, resolve };
}

const snapshot = (name: string, model: "classic" | "slim", texture = name) => ({
  ok: true,
  playerName: name,
  textureDataUrl: `data:image/png;base64,${texture}`,
  model,
  source: "minecraft-services-authenticated",
  statusMessage: "official_authenticated_minecraft_skin",
});

beforeEach(() => {
  mocks.load.mockReset();
  mocks.head.mockReset().mockResolvedValue("data:image/png;base64,avatar");
  mocks.loadImage.mockReset().mockResolvedValue({ width: 64, height: 64 });
  mocks.renderers.length = 0;
});

describe("LauncherSkinPreview", () => {
  it.each(["classic", "slim"] as const)("renders the authoritative %s model and nametag", async (model) => {
    mocks.load.mockResolvedValue(snapshot("Silas9k", model));
    render(<LauncherSkinPreview accountId="uuid-a" playerName="Silas9k" reducedMotion={false} badgeIconUrl="./snine-icons/snine_icon_blue.png" cameraYaw={180} />);
    const nametag = await screen.findByLabelText("Minecraft player Silas9k");
    expect(nametag).toHaveStyle("opacity: 0");
    const anchorListener = mocks.renderers[0].setNametagAnchorListener.mock.calls[0][0];
    act(() => anchorListener({ x: 142, y: 76, visible: true }));
    expect(nametag).toHaveStyle("--launcher-nametag-x: 142px; --launcher-nametag-y: 76px; opacity: 1");
    expect(nametag.querySelector("img")).toHaveAttribute("src", "./snine-icons/snine_icon_blue.png");
    await waitFor(() => expect(mocks.renderers[0].setSkin).toHaveBeenCalledWith(expect.anything(), model));
  });

  it("maps the authenticated backend badge selection to the client icon assets", () => {
    expect(launcherBadgeIconUrl("snine_icon_black_gradient", true)).toBe("./snine-icons/snine_icon_black_gradient.png");
    expect(launcherBadgeIconUrl("summer", true)).toBe("./snine-icons/snine_icon_summer.png");
    expect(launcherBadgeIconUrl("", true)).toBe("./snine-icons/snine_icon_blue.png");
    expect(launcherBadgeIconUrl("", false)).toBe("./snine-icons/snine_icon.png");
    expect(launcherBadgeIconUrl("snine_icon_blue", false)).toBe("./snine-icons/snine_icon_blue.png");
  });

  it("updates the live nametag icon without recreating the WebGL renderer", async () => {
    mocks.load.mockResolvedValue(snapshot("Silas9k", "classic"));
    const view = render(<LauncherSkinPreview accountId="uuid-a" playerName="Silas9k" reducedMotion badgeIconUrl="./snine-icons/snine_icon.png" />);
    await waitFor(() => expect(mocks.renderers).toHaveLength(1));
    expect(screen.getByLabelText("Minecraft player Silas9k").querySelector("img")).toHaveAttribute("src", "./snine-icons/snine_icon.png");

    view.rerender(<LauncherSkinPreview accountId="uuid-a" playerName="Silas9k" reducedMotion badgeIconUrl="./snine-icons/snine_icon_summer.png" />);

    expect(screen.getByLabelText("Minecraft player Silas9k").querySelector("img")).toHaveAttribute("src", "./snine-icons/snine_icon_summer.png");
    expect(mocks.renderers).toHaveLength(1);
  });

  it("cannot apply account A after a fast switch to account B", async () => {
    const alpha = deferred<any>();
    const beta = deferred<any>();
    mocks.load.mockImplementation((id: string) => id === "a" ? alpha.promise : beta.promise);
    const view = render(<LauncherSkinPreview accountId="a" playerName="Alpha" reducedMotion />);
    view.rerender(<LauncherSkinPreview accountId="b" playerName="Beta" reducedMotion />);
    beta.resolve(snapshot("Beta", "slim", "beta"));
    expect(await screen.findByLabelText("Minecraft player Beta")).toBeInTheDocument();
    alpha.resolve(snapshot("Alpha", "classic", "alpha"));
    await Promise.resolve();
    expect(screen.queryByLabelText("Minecraft player Alpha")).not.toBeInTheDocument();
    expect(mocks.renderers[0].setSkin).toHaveBeenCalledTimes(1);
  });

  it("disposes exactly one renderer on unmount and creates one on remount", async () => {
    mocks.load.mockResolvedValue(snapshot("Alpha", "classic"));
    const first = render(<LauncherSkinPreview accountId="a" playerName="Alpha" reducedMotion />);
    await waitFor(() => expect(mocks.renderers).toHaveLength(1));
    first.unmount();
    expect(mocks.renderers[0].setNametagAnchorListener).toHaveBeenLastCalledWith(null);
    expect(mocks.renderers[0].dispose).toHaveBeenCalledOnce();
    const second = render(<LauncherSkinPreview accountId="a" playerName="Alpha" reducedMotion />);
    await waitFor(() => expect(mocks.renderers).toHaveLength(2));
    second.unmount();
    expect(mocks.renderers[1].dispose).toHaveBeenCalledOnce();
  });
});
