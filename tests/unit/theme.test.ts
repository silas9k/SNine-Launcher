import { describe, expect, it } from "vitest";
import { applyShellTheme, resolveAppearance } from "../../src/theme/applyTheme";
import { DEFAULT_SHELL_SETTINGS } from "../../src/theme/types";

describe("theme application", () => {
  it("resolves system appearance", () => {
    expect(resolveAppearance("system", true)).toBe("dark");
    expect(resolveAppearance("system", false)).toBe("light");
  });

  it("sets all shell data attributes and accent tokens", () => {
    const root = document.createElement("div");
    applyShellTheme({ ...DEFAULT_SHELL_SETTINGS, appearance: "contrast", density: "compact", navigationMode: "compact", backgroundVariant: "grid", reducedMotion: true }, root);
    expect(root.dataset.theme).toBe("contrast");
    expect(root.dataset.density).toBe("compact");
    expect(root.dataset.navigation).toBe("compact");
    expect(root.dataset.background).toBe("grid");
    expect(root.dataset.reducedMotion).toBe("true");
    expect(root.style.getPropertyValue("--color-accent")).toMatch(/^#[0-9a-f]{6}$/);
  });
});
