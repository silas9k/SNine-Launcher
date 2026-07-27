import { useLayoutEffect, useRef } from "react";
import { Download, X } from "lucide-react";
import { useShellStore } from "../../app/shellStore";
import { useI18n } from "../../i18n/I18nProvider";
import { EmptyState, IconButton } from "../ui";

export function TaskCenter() {
  const { t } = useI18n();
  const open = useShellStore((state) => state.taskCenterOpen);
  const setOpen = useShellStore((state) => state.setTaskCenterOpen);
  const panel = useRef<HTMLElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);

  useLayoutEffect(() => {
    if (!open) return;
    previousFocus.current = document.activeElement as HTMLElement;
    panel.current?.querySelector<HTMLElement>("button:not([disabled]), [tabindex]:not([tabindex='-1'])")?.focus();
    const keydown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        setOpen(false);
        return;
      }
      if (event.key !== "Tab" || !panel.current) return;
      const focusable = [...panel.current.querySelectorAll<HTMLElement>("button:not([disabled]), [tabindex]:not([tabindex='-1'])")];
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", keydown);
    return () => {
      document.removeEventListener("keydown", keydown);
      previousFocus.current?.focus();
    };
  }, [open, setOpen]);

  return (
    <>
      {open ? <button className="task-center__scrim" aria-label={t("tasks.close")} onClick={() => setOpen(false)} /> : null}
      <aside ref={panel} className={`task-center ${open ? "task-center--open" : ""}`} role="dialog" aria-modal={open || undefined} aria-hidden={!open} aria-label={t("tasks.title")}>
        <header><div><h2>{t("tasks.title")}</h2><p>{t("tasks.description")}</p></div><IconButton label={t("tasks.close")} onClick={() => setOpen(false)}><X aria-hidden="true" /></IconButton></header>
        <div className="task-center__body"><EmptyState icon={<Download />} label={t("empty.previewLabel")} title={t("tasks.emptyTitle")} description={t("tasks.emptyDescription")} /></div>
      </aside>
    </>
  );
}
