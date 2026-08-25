import { AlertCircle, CheckCircle2, Info, TriangleAlert, X } from "lucide-react";
import { useShellStore } from "../../app/shellStore";
import { useI18n } from "../../i18n/I18nProvider";
import { IconButton } from "../ui";

export function Toasts() {
  const { t } = useI18n();
  const toasts = useShellStore((state) => state.toasts);
  const dismiss = useShellStore((state) => state.dismissToast);
  return <div className="toast-region" aria-live="polite" aria-atomic="false">{toasts.map((toast) => {
    const Icon = toast.tone === "success" ? CheckCircle2 : toast.tone === "warning" ? TriangleAlert : toast.tone === "error" ? AlertCircle : Info;
    return <div className={`shell-toast shell-toast--${toast.tone}`} key={toast.id} role={toast.tone === "error" ? "alert" : "status"}><Icon aria-hidden="true" /><span>{t(toast.messageKey, toast.params)}</span><IconButton size="small" label={t("toast.dismiss")} onClick={() => dismiss(toast.id)}><X aria-hidden="true" /></IconButton></div>;
  })}</div>;
}
