import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ load: vi.fn() }));

vi.mock("../../src/lib/snineClientBridge", () => ({
  loadSNinePlayerSkin: mocks.load,
}));

import {
  composeMinecraftHead,
  invalidateMinecraftSkin,
  loadMinecraftSkin,
  minecraftHeadFromSnapshot,
  resetMinecraftSkinCachesForTests,
} from "../../src/lib/minecraftSkinCache";

const snapshot = (name: string, texture: string, model: "classic" | "slim" = "classic") => ({
  ok: true,
  playerName: name,
  textureDataUrl: texture,
  model,
  source: "minecraft-services-authenticated",
  statusMessage: "official_authenticated_minecraft_skin",
});

beforeEach(() => {
  resetMinecraftSkinCachesForTests();
  mocks.load.mockReset();
});

describe("Minecraft skin cache", () => {
  it("deduplicates one account while keeping different UUIDs isolated", async () => {
    mocks.load.mockImplementation(async (id: string, name: string) => snapshot(name, `data:image/png;base64,${id}`));
    const first = loadMinecraftSkin("account-a", "Alpha");
    const duplicate = loadMinecraftSkin("account-a", "Alpha");
    const second = loadMinecraftSkin("account-b", "Beta");

    expect(first).toBe(duplicate);
    await expect(first).resolves.toMatchObject({ playerName: "Alpha" });
    await expect(second).resolves.toMatchObject({ playerName: "Beta" });
    expect(mocks.load).toHaveBeenCalledTimes(2);
  });

  it("invalidates only the selected account", async () => {
    mocks.load.mockImplementation(async (id: string, name: string) => snapshot(name, `data:image/png;base64,${id}`));
    await loadMinecraftSkin("account-a", "Alpha");
    await loadMinecraftSkin("account-b", "Beta");
    invalidateMinecraftSkin("account-a");
    await loadMinecraftSkin("account-a", "Alpha");
    await loadMinecraftSkin("account-b", "Beta");
    expect(mocks.load).toHaveBeenCalledTimes(3);
  });

  it("composes the face and hat layer and caches the generated head by skin", async () => {
    const drawImage = vi.fn();
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
      imageSmoothingEnabled: true,
      clearRect: vi.fn(),
      drawImage,
    } as unknown as CanvasRenderingContext2D);
    vi.spyOn(HTMLCanvasElement.prototype, "toDataURL").mockReturnValue("data:image/png;base64,avatar");
    class TestImage {
      naturalWidth = 64;
      naturalHeight = 64;
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      set src(_value: string) { queueMicrotask(() => this.onload?.()); }
    }
    vi.stubGlobal("Image", TestImage);

    await expect(composeMinecraftHead("data:image/png;base64,skin", 48)).resolves.toBe("data:image/png;base64,avatar");
    expect(drawImage).toHaveBeenNthCalledWith(1, expect.any(TestImage), 8, 8, 8, 8, 0, 0, 48, 48);
    expect(drawImage).toHaveBeenNthCalledWith(2, expect.any(TestImage), 40, 8, 8, 8, 0, 0, 48, 48);

    const skin = snapshot("Alpha", "data:image/png;base64,skin", "slim");
    const one = minecraftHeadFromSnapshot("account-a", skin, 48);
    const two = minecraftHeadFromSnapshot("account-a", skin, 48);
    expect(one).toBe(two);
    await expect(one).resolves.toBe("data:image/png;base64,avatar");
  });
});
