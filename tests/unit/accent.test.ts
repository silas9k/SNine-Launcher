import { describe, expect, it } from "vitest";
import { contrastRatio, resolveAccentPalette } from "../../src/theme/accent";

describe("accent palette", () => {
  it("rejects malformed colors", () => {
    expect(resolveAccentPalette("red").valid).toBe(false);
  });

  it("produces readable text and UI contrast", () => {
    for (const input of ["#ffffff", "#000000", "#c83f49", "#777777", "#00ff00"]) {
      const palette = resolveAccentPalette(input);
      expect(palette.valid).toBe(true);
      expect(palette.lightContrast).toBeGreaterThanOrEqual(3);
      expect(palette.darkContrast).toBeGreaterThanOrEqual(3);
      expect(contrastRatio(palette.accent, palette.onAccent)).toBeGreaterThanOrEqual(4.5);
    }
  });
});
