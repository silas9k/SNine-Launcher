import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import axe from "axe-core";
import App from "../../src/App";
import { useShellStore } from "../../src/app/shellStore";
import { DEFAULT_SHELL_SETTINGS } from "../../src/theme/types";

describe("app shell accessibility", () => {
  it("has no serious or critical axe violations", async () => {
    useShellStore.setState({ initialized: true, loading: false, page: "home", settings: { ...DEFAULT_SHELL_SETTINGS }, taskCenterOpen: false, mobileNavigationOpen: false, dialog: null, toasts: [] });
    render(<App />);
    expect(screen.getByRole("main")).toBeInTheDocument();
    const results = await axe.run(document, { runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21aa"] }, rules: { "color-contrast": { enabled: false } } });
    const blocking = results.violations.filter((violation) => violation.impact === "serious" || violation.impact === "critical");
    expect(blocking).toEqual([]);
  });
});
