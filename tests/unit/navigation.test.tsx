import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { Navigation } from "../../src/components/shell/Navigation";
import { I18nProvider } from "../../src/i18n/I18nProvider";
import { useShellStore } from "../../src/app/shellStore";
import { DEFAULT_SHELL_SETTINGS } from "../../src/theme/types";

function setViewportMatches(matches: boolean) {
  window.matchMedia = () => ({
    matches,
    media: "(max-width: 860px)",
    onchange: null,
    addListener: () => undefined,
    removeListener: () => undefined,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    dispatchEvent: () => false,
  });
}

beforeEach(() => {
  setViewportMatches(false);
  useShellStore.setState({ page: "home", settings: { ...DEFAULT_SHELL_SETTINGS }, mobileNavigationOpen: false, toasts: [] });
});

describe("navigation", () => {
  it("exposes labels and changes the current page by keyboard/click", async () => {
    const user = userEvent.setup();
    render(<><I18nProvider localeSetting="en"><Navigation /></I18nProvider><main id="main-content" tabIndex={-1} /></>);
    const library = screen.getByRole("button", { name: "Library" });
    await user.click(library);
    expect(library).toHaveAttribute("aria-current", "page");
    expect(useShellStore.getState().page).toBe("library");
    await waitFor(() => expect(document.getElementById("main-content")).toHaveFocus());
    expect(screen.getByText("Current area: Library")).toHaveAttribute("aria-live", "polite");
  });

  it("makes the closed mobile drawer inert and traps focus while it is open", async () => {
    setViewportMatches(true);
    const user = userEvent.setup();
    render(
      <>
        <button type="button" onClick={() => useShellStore.getState().setMobileNavigationOpen(true)}>Open drawer</button>
        <I18nProvider localeSetting="en"><Navigation /></I18nProvider>
      </>,
    );
    const drawer = document.querySelector<HTMLElement>(".shell-nav");
    expect(drawer).not.toBeNull();
    expect(drawer).toHaveAttribute("aria-hidden", "true");
    expect(drawer).toHaveAttribute("inert");

    const trigger = screen.getByRole("button", { name: "Open drawer" });
    await user.click(trigger);
    const close = within(drawer!).getByRole("button", { name: "Close" });
    await waitFor(() => expect(close).toHaveFocus());
    expect(drawer).not.toHaveAttribute("aria-hidden");
    expect(drawer).not.toHaveAttribute("inert");

    await user.tab({ shift: true });
    expect(within(drawer!).getByRole("button", { name: "Collapse navigation" })).toHaveFocus();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(trigger).toHaveFocus());
    expect(drawer).toHaveAttribute("aria-hidden", "true");
    expect(drawer).toHaveAttribute("inert");
  });
});
