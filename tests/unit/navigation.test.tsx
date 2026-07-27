import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { Navigation } from "../../src/components/shell/Navigation";
import { I18nProvider } from "../../src/i18n/I18nProvider";
import { useShellStore } from "../../src/app/shellStore";
import { DEFAULT_SHELL_SETTINGS } from "../../src/theme/types";

beforeEach(() => useShellStore.setState({ page: "home", settings: { ...DEFAULT_SHELL_SETTINGS }, mobileNavigationOpen: false, toasts: [] }));

describe("navigation", () => {
  it("exposes labels and changes the current page by keyboard/click", async () => {
    const user = userEvent.setup();
    render(<I18nProvider localeSetting="en"><Navigation /></I18nProvider>);
    const library = screen.getByRole("button", { name: "Library" });
    await user.click(library);
    expect(library).toHaveAttribute("aria-current", "page");
    expect(useShellStore.getState().page).toBe("library");
  });
});
