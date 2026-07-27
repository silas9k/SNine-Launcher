import {
  Children,
  cloneElement,
  forwardRef,
  isValidElement,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type HTMLAttributes,
  type InputHTMLAttributes,
  type ReactElement,
  type ReactNode,
  type SelectHTMLAttributes,
} from "react";
import { createPortal } from "react-dom";
import { AlertCircle, CheckCircle2, ChevronDown, Info, LoaderCircle, Search, TriangleAlert, X } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";

export type Tone = "neutral" | "accent" | "success" | "warning" | "error" | "info";

export const Button = forwardRef<HTMLButtonElement, ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  loading?: boolean;
}>(({ variant = "secondary", loading = false, disabled, children, className = "", ...props }, ref) => (
  <button ref={ref} className={`ui-button ui-button--${variant} ${className}`} disabled={disabled || loading} aria-busy={loading || undefined} {...props}>
    {loading ? <LoaderCircle className="ui-spin" aria-hidden="true" /> : null}
    <span>{children}</span>
  </button>
));
Button.displayName = "Button";

export const IconButton = forwardRef<HTMLButtonElement, ButtonHTMLAttributes<HTMLButtonElement> & { label: string; size?: "small" | "medium" }>(
  ({ label, size = "medium", children, className = "", ...props }, ref) => (
    <button ref={ref} className={`ui-icon-button ui-icon-button--${size} ${className}`} aria-label={label} title={label} {...props}>{children}</button>
  ),
);
IconButton.displayName = "IconButton";

export const TextField = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement> & {
  label: string;
  description?: string;
  error?: string;
}>(({ label, description, error, id, className = "", ...props }, ref) => {
  const generated = useId();
  const inputId = id ?? generated;
  const descriptionId = description ? `${inputId}-description` : undefined;
  const errorId = error ? `${inputId}-error` : undefined;
  return (
    <label className={`ui-field ${error ? "ui-field--error" : ""} ${className}`} htmlFor={inputId}>
      <span className="ui-field__label">{label}</span>
      {description ? <span className="ui-field__description" id={descriptionId}>{description}</span> : null}
      <input ref={ref} id={inputId} aria-describedby={[descriptionId, errorId].filter(Boolean).join(" ") || undefined} aria-invalid={Boolean(error)} {...props} />
      {error ? <span className="ui-field__error" id={errorId} role="alert">{error}</span> : null}
    </label>
  );
});
TextField.displayName = "TextField";

export const SearchField = forwardRef<HTMLInputElement, Omit<InputHTMLAttributes<HTMLInputElement>, "type"> & { label: string }>(
  ({ label, className = "", ...props }, ref) => (
    <label className={`ui-search ${className}`}>
      <span className="sr-only">{label}</span>
      <Search aria-hidden="true" />
      <input ref={ref} type="search" aria-label={label} {...props} />
    </label>
  ),
);
SearchField.displayName = "SearchField";

export const SelectField = forwardRef<HTMLSelectElement, SelectHTMLAttributes<HTMLSelectElement> & { label: string; description?: string }>(
  ({ label, description, id, children, ...props }, ref) => {
    const generated = useId();
    const selectId = id ?? generated;
    return (
      <label className="ui-field" htmlFor={selectId}>
        <span className="ui-field__label">{label}</span>
        {description ? <span className="ui-field__description">{description}</span> : null}
        <span className="ui-select-wrap"><select ref={ref} id={selectId} {...props}>{children}</select><ChevronDown aria-hidden="true" /></span>
      </label>
    );
  },
);
SelectField.displayName = "SelectField";

export function Checkbox({ label, description, ...props }: InputHTMLAttributes<HTMLInputElement> & { label: string; description?: string }) {
  return (
    <label className="ui-check">
      <input type="checkbox" {...props} />
      <span className="ui-check__box" aria-hidden="true" />
      <span><strong>{label}</strong>{description ? <small>{description}</small> : null}</span>
    </label>
  );
}

export function Switch({ label, description, ...props }: InputHTMLAttributes<HTMLInputElement> & { label: string; description?: string }) {
  return (
    <label className="ui-switch">
      <span><strong>{label}</strong>{description ? <small>{description}</small> : null}</span>
      <input type="checkbox" role="switch" {...props} />
      <span className="ui-switch__track" aria-hidden="true"><span /></span>
    </label>
  );
}

export function Tabs({ label, value, onChange, items }: { label: string; value: string; onChange: (value: string) => void; items: Array<{ value: string; label: string }> }) {
  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    const tabs = [...event.currentTarget.querySelectorAll<HTMLButtonElement>("button[role='tab']")];
    const current = tabs.indexOf(document.activeElement as HTMLButtonElement);
    if (current < 0 || tabs.length === 0) return;
    event.preventDefault();
    const next = event.key === "Home"
      ? 0
      : event.key === "End"
        ? tabs.length - 1
        : (current + (event.key === "ArrowRight" ? 1 : -1) + tabs.length) % tabs.length;
    tabs[next].focus();
    onChange(items[next].value);
  };
  return <div className="ui-tabs" role="tablist" aria-label={label} onKeyDown={onKeyDown}>{items.map((item) => <button key={item.value} role="tab" aria-selected={value === item.value} tabIndex={value === item.value ? 0 : -1} onClick={() => onChange(item.value)}>{item.label}</button>)}</div>;
}

export function Card({ className = "", ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={`ui-card ${className}`} {...props} />;
}

export function Badge({ tone = "neutral", children }: { tone?: Tone; children: ReactNode }) {
  return <span className={`ui-badge ui-badge--${tone}`}>{children}</span>;
}

export function Status({ tone, label, children }: { tone: Exclude<Tone, "neutral" | "accent">; label: string; children: ReactNode }) {
  const Icon = tone === "success" ? CheckCircle2 : tone === "warning" ? TriangleAlert : tone === "error" ? AlertCircle : Info;
  return <div className={`ui-status ui-status--${tone}`} role="status" aria-label={label}><Icon aria-hidden="true" /><span>{children}</span></div>;
}

export function Tooltip({ text, children }: { text: string; children: ReactElement }) {
  const id = useId();
  return <span className="ui-tooltip">{isValidElement(children) ? cloneElement(children, { "aria-describedby": id } as object) : children}<span role="tooltip" id={id}>{text}</span></span>;
}

export function DropdownMenu({ label, trigger, children }: { label: string; trigger: ReactElement; children: ReactNode }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const close = (event: MouseEvent) => { if (!root.current?.contains(event.target as Node)) setOpen(false); };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, []);

  useEffect(() => {
    if (!open) return;
    root.current?.querySelector<HTMLElement>("[role='menuitem']")?.focus();
  }, [open]);

  const onMenuKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const items = [...event.currentTarget.querySelectorAll<HTMLElement>("[role='menuitem']:not([disabled])")];
    if (event.key === "Escape") {
      event.preventDefault();
      setOpen(false);
      root.current?.querySelector<HTMLElement>("[aria-haspopup='menu']")?.focus();
      return;
    }
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key) || items.length === 0) return;
    event.preventDefault();
    const current = Math.max(0, items.indexOf(document.activeElement as HTMLElement));
    const next = event.key === "Home"
      ? 0
      : event.key === "End"
        ? items.length - 1
        : (current + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length;
    items[next].focus();
  };

  return <div className="ui-dropdown" ref={root}>{cloneElement(trigger, { "aria-haspopup": "menu", "aria-expanded": open, onClick: () => setOpen((value) => !value), onKeyDown: (event: React.KeyboardEvent) => { if (event.key === "ArrowDown") { event.preventDefault(); setOpen(true); } } } as object)}{open ? <div className="ui-dropdown__menu" role="menu" aria-label={label} onKeyDown={onMenuKeyDown}>{children}</div> : null}</div>;
}

export function MenuItem({ children, ...props }: ButtonHTMLAttributes<HTMLButtonElement>) {
  return <button className="ui-menu-item" role="menuitem" {...props}>{children}</button>;
}

export function Dialog({ open, title, description, onClose, children, footer }: { open: boolean; title: string; description?: string; onClose: () => void; children?: ReactNode; footer?: ReactNode }) {
  const { t } = useI18n();
  const panel = useRef<HTMLDivElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);
  const titleId = useId();
  const descriptionId = useId();

  useLayoutEffect(() => {
    if (!open) return;
    previousFocus.current = document.activeElement as HTMLElement;
    const focusable = panel.current?.querySelector<HTMLElement>("button:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex='-1'])");
    focusable?.focus();
    const keydown = (event: KeyboardEvent) => {
      if (event.key === "Escape") { event.preventDefault(); onClose(); return; }
      if (event.key !== "Tab" || !panel.current) return;
      const focusable = [...panel.current.querySelectorAll<HTMLElement>("button:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex='-1'])")];
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
    };
    document.addEventListener("keydown", keydown);
    return () => {
      document.removeEventListener("keydown", keydown);
      previousFocus.current?.focus();
    };
  }, [open, onClose]);

  if (!open) return null;
  return createPortal(
    <div className="ui-dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
      <div ref={panel} className="ui-dialog" role="dialog" aria-modal="true" aria-labelledby={titleId} aria-describedby={description ? descriptionId : undefined}>
        <header><div><h2 id={titleId}>{title}</h2>{description ? <p id={descriptionId}>{description}</p> : null}</div><IconButton label={t("dialog.close")} onClick={onClose}><X aria-hidden="true" /></IconButton></header>
        {children ? <div className="ui-dialog__body">{children}</div> : null}
        {footer ? <footer>{footer}</footer> : null}
      </div>
    </div>, document.body,
  );
}

export function ConfirmDialog({ open, title, description, confirmLabel, cancelLabel, onConfirm, onClose, loading = false }: { open: boolean; title: string; description: string; confirmLabel: string; cancelLabel: string; onConfirm: () => void; onClose: () => void; loading?: boolean }) {
  return <Dialog open={open} title={title} description={description} onClose={onClose} footer={<><Button onClick={onClose}>{cancelLabel}</Button><Button variant="primary" loading={loading} onClick={onConfirm}>{confirmLabel}</Button></>}/>;
}

export function Progress({ label, value }: { label: string; value: number }) {
  const safe = Math.max(0, Math.min(100, value));
  return <div className="ui-progress"><div><span>{label}</span><span>{safe}%</span></div><div role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={safe}><span style={{ width: `${safe}%` }} /></div></div>;
}

export function Skeleton({ width = "100%", height = "1rem" }: { width?: string; height?: string }) {
  return <span className="ui-skeleton" aria-hidden="true" style={{ width, height }} />;
}

export function EmptyState({ icon, label, title, description, action }: { icon: ReactNode; label: string; title: string; description: string; action?: ReactNode }) {
  return <div className="ui-empty" role="status" aria-label={label}><div className="ui-empty__icon" aria-hidden="true">{icon}</div><h2>{title}</h2><p>{description}</p>{action}</div>;
}

export function ErrorState({ title, description, action }: { title: string; description: string; action?: ReactNode }) {
  return <div className="ui-empty ui-empty--error" role="alert"><div className="ui-empty__icon" aria-hidden="true"><AlertCircle /></div><h2>{title}</h2><p>{description}</p>{action}</div>;
}

export function DataTable({ label, headers, rows }: { label: string; headers: string[]; rows: ReactNode[][] }) {
  const { t } = useI18n();
  return <div className="ui-table-wrap"><table><caption className="sr-only">{label}</caption><thead><tr>{headers.map((header) => <th key={header} scope="col">{header}</th>)}</tr></thead><tbody>{rows.length ? rows.map((row, rowIndex) => <tr key={rowIndex}>{Children.map(row, (cell) => <td>{cell}</td>)}</tr>) : <tr><td colSpan={headers.length}>{t("table.empty")}</td></tr>}</tbody></table></div>;
}
