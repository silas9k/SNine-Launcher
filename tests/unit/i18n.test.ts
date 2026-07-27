import { describe, expect, it } from "vitest";
import { en, de } from "../../src/i18n/messages";
import { interpolate, resolveLocale } from "../../src/i18n/I18nProvider";

describe("i18n", () => {
  it("keeps German and English keys identical", () => {
    expect(Object.keys(en).sort()).toEqual(Object.keys(de).sort());
  });

  it("interpolates known params and leaves development mistakes visible", () => {
    expect(interpolate("Hello {name}", { name: "S9Lab" })).toBe("Hello S9Lab");
    expect(interpolate("Hello {name}")).toBe("Hello {name}");
  });

  it("resolves system language deterministically", () => {
    expect(resolveLocale("system", "de-DE")).toBe("de");
    expect(resolveLocale("system", "fr-FR")).toBe("en");
    expect(resolveLocale("en", "de-DE")).toBe("en");
  });
});
