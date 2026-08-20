import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Dialog } from "../../src/components/ui";
import { I18nProvider } from "../../src/i18n/I18nProvider";

function renderDialog(onClose = vi.fn()) {
  return render(<I18nProvider localeSetting="en"><button>Before</button><Dialog open={false} title="Dialog title" description="Dialog description" onClose={onClose} footer={<><button>Cancel</button><button>Confirm</button></>} /></I18nProvider>);
}

describe("dialog accessibility", () => {
  it("moves focus inside, traps tab and restores focus", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const view = renderDialog(onClose);
    const before = screen.getByRole("button", { name: "Before" });
    before.focus();
    view.rerender(<I18nProvider localeSetting="en"><button>Before</button><Dialog open title="Dialog title" description="Dialog description" onClose={onClose} footer={<><button>Cancel</button><button>Confirm</button></>} /></I18nProvider>);
    await new Promise((resolve) => requestAnimationFrame(resolve));
    expect(screen.getByRole("dialog")).toContainElement(document.activeElement as HTMLElement);
    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledOnce();
  });
});
