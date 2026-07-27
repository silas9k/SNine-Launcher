import { useState } from "react";
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Button, DropdownMenu, MenuItem, Tabs } from "../../src/components/ui";

function TabsHarness() {
  const [value, setValue] = useState("first");
  return <Tabs label="Sections" value={value} onChange={setValue} items={[{ value: "first", label: "First" }, { value: "second", label: "Second" }]} />;
}

describe("keyboard-ready reusable controls", () => {
  it("moves tabs and selection with arrow keys", async () => {
    const user = userEvent.setup();
    render(<TabsHarness />);
    const first = screen.getByRole("tab", { name: "First" });
    const second = screen.getByRole("tab", { name: "Second" });
    first.focus();
    await user.keyboard("{ArrowRight}");
    expect(second).toHaveFocus();
    expect(second).toHaveAttribute("aria-selected", "true");
  });

  it("opens a dropdown from the keyboard and restores trigger focus on Escape", async () => {
    const user = userEvent.setup();
    render(<DropdownMenu label="Actions" trigger={<Button>Open</Button>}><MenuItem>Action</MenuItem></DropdownMenu>);
    const trigger = screen.getByRole("button", { name: "Open" });
    trigger.focus();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "Action" })).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(trigger).toHaveFocus();
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });
});
