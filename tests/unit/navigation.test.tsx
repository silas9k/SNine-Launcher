import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { Navigation } from "../../src/components/shell/Navigation";
import { I18nProvider } from "../../src/i18n/I18nProvider";
import { useShellStore } from "../../src/app/shellStore";
import { DEFAULT_SHELL_SETTINGS } from "../../src/theme/types";

beforeEach(() => {
  useShellStore.setState({
    page: "home",
    settings: { ...DEFAULT_SHELL_SETTINGS },
    mobileNavigationOpen: false,
    toasts: [],
  });
});

describe("navigation", () => {
  it("renders the current customer navigation labels", () => {
    render(
      <>
        <I18nProvider localeSetting="en">
          <Navigation />
        </I18nProvider>
        <main id="main-content" tabIndex={-1} />
      </>,
    );

    expect(screen.getByRole("button", { name: "Home" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Profiles" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Cosmetics" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Skins" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Mods" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Settings" })).toBeVisible();
  });

  it("changes the selected page and focuses main content", async () => {
    const user = userEvent.setup();

    render(
      <>
        <I18nProvider localeSetting="en">
          <Navigation />
        </I18nProvider>
        <main id="main-content" tabIndex={-1} />
      </>,
    );

    const profiles = screen.getByRole("button", { name: "Profiles" });

    await user.click(profiles);

    expect(useShellStore.getState().page).toBe("profiles");
    expect(profiles).toHaveAttribute("aria-current", "page");

    await waitFor(() => {
      expect(document.getElementById("main-content")).toHaveFocus();
    });
  });

  it("exposes the current developer stats destination present in the launcher", () => {
    render(
      <>
        <I18nProvider localeSetting="en">
          <Navigation />
        </I18nProvider>
        <main id="main-content" tabIndex={-1} />
      </>,
    );

    expect(
      screen.getByRole("button", { name: "Developer Stats" }),
    ).toBeVisible();
  });
});
