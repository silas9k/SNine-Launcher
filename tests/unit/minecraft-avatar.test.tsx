import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  load: vi.fn(),
  head: vi.fn(),
}));

vi.mock("../../src/lib/minecraftSkinCache", () => ({
  loadMinecraftSkin: mocks.load,
  minecraftHeadFromSnapshot: mocks.head,
}));

import { MinecraftAvatar } from "../../src/components/player/MinecraftAvatar";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

beforeEach(() => {
  mocks.load.mockReset();
  mocks.head.mockReset();
  mocks.head.mockImplementation(async (_id: string, value: { playerName: string }) => `data:image/png;base64,${value.playerName}`);
});

describe("MinecraftAvatar", () => {
  it("ignores a late result from the previous account", async () => {
    const alpha = deferred<any>();
    const beta = deferred<any>();
    mocks.load.mockImplementation((id: string) => id === "a" ? alpha.promise : beta.promise);
    const view = render(<MinecraftAvatar accountId="a" username="Alpha" decorative={false} />);
    view.rerender(<MinecraftAvatar accountId="b" username="Beta" decorative={false} />);

    beta.resolve({ playerName: "Beta", textureDataUrl: "beta", model: "slim", ok: true });
    expect(await screen.findByRole("img", { name: "Beta avatar" })).toHaveAttribute("src", "data:image/png;base64,Beta");
    alpha.resolve({ playerName: "Alpha", textureDataUrl: "alpha", model: "classic", ok: true });
    await Promise.resolve();
    expect(screen.getByRole("img", { name: "Beta avatar" })).toHaveAttribute("src", "data:image/png;base64,Beta");
  });

  it("keeps the local fallback when loading fails", async () => {
    mocks.load.mockRejectedValue(new Error("offline"));
    const { container } = render(<MinecraftAvatar accountId="a" username="Alpha" />);
    await waitFor(() => expect(container.querySelector("[data-avatar-state='fallback'] svg")).toBeInTheDocument());
    expect(container.querySelector("img")).not.toBeInTheDocument();
  });
});
