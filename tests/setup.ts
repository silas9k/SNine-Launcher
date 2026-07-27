import "@testing-library/jest-dom/vitest";
import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

afterEach(() => cleanup());

Object.defineProperty(window, "matchMedia", {
  writable: true,
  value: (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: () => undefined,
    removeListener: () => undefined,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    dispatchEvent: () => false,
  }),
});

if (!window.requestAnimationFrame) window.requestAnimationFrame = (callback) => window.setTimeout(() => callback(performance.now()), 0);
if (!window.cancelAnimationFrame) window.cancelAnimationFrame = (id) => window.clearTimeout(id);

document.documentElement.lang = "en";
document.title = "S9Lab";
