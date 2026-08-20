import { useCallback, useEffect, useState } from "react";
import { Check, ExternalLink, LogIn, RefreshCcw, ShieldCheck, Trash2, UserRound } from "lucide-react";
import { authCommands, openMicrosoftVerification } from "../lib/authCommands";
import { typedIpcError } from "../lib/shellCommands";
import type {
  Phase3Account,
  Phase3AuthSnapshot,
  Phase3DeviceLoginPrompt,
} from "../lib/generated/ipc-contracts";
import { Badge, Button, Card, ConfirmDialog, Dialog, EmptyState } from "../components/ui";
import { useI18n } from "../i18n/I18nProvider";
import type { TranslationKey } from "../i18n/messages";

export function AccountsPage() {
  const { t, locale, formatDate } = useI18n();
  const [snapshot, setSnapshot] = useState<Phase3AuthSnapshot | null>(null);
  const [prompt, setPrompt] = useState<Phase3DeviceLoginPrompt | null>(null);
  const [removeTarget, setRemoveTarget] = useState<Phase3Account | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [errorKey, setErrorKey] = useState<TranslationKey | null>(null);

  const reload = useCallback(async () => {
    setSnapshot(await authCommands.snapshot());
  }, []);

  useEffect(() => {
    void reload().catch(() => setErrorKey("error.internal_error"));
  }, [reload]);

  const report = (error: unknown) => {
    const typed = typedIpcError(error);
    setErrorKey((typed?.messageKey as TranslationKey | undefined) ?? "error.internal_error");
  };

  const startLogin = async () => {
    setBusy("start");
    setErrorKey(null);
    try {
      setPrompt(await authCommands.startDeviceLogin(locale));
    } catch (error) {
      report(error);
    } finally {
      setBusy(null);
    }
  };

  const finishLogin = async () => {
    if (!prompt) return;
    setBusy("complete");
    setErrorKey(null);
    try {
      await authCommands.completeDeviceLogin(prompt.loginId);
      setPrompt(null);
      await reload();
    } catch (error) {
      report(error);
    } finally {
      setBusy(null);
    }
  };

  const cancelLogin = async () => {
    const current = prompt;
    setPrompt(null);
    if (current) await authCommands.cancelDeviceLogin(current.loginId).catch(() => undefined);
  };

  const selectAccount = async (accountId: string) => {
    setBusy(`select:${accountId}`);
    try {
      await authCommands.selectAccount(accountId);
      await reload();
    } catch (error) {
      report(error);
    } finally {
      setBusy(null);
    }
  };

  const refreshAccount = async (accountId: string) => {
    setBusy(`refresh:${accountId}`);
    try {
      await authCommands.refreshAccount(accountId);
      await reload();
    } catch (error) {
      report(error);
    } finally {
      setBusy(null);
    }
  };

  const removeAccount = async () => {
    if (!removeTarget) return;
    setBusy(`remove:${removeTarget.id}`);
    try {
      await authCommands.removeAccount(removeTarget.id);
      setRemoveTarget(null);
      await reload();
    } catch (error) {
      report(error);
    } finally {
      setBusy(null);
    }
  };

  const accounts = snapshot?.accounts ?? [];
  return (
    <div className="page accounts-page">
      <header className="page-heading">
        <div>
          <p className="page-eyebrow">{t("app.name")}</p>
          <h1>{t("page.accounts.title")}</h1>
          <p>{t("accounts.description")}</p>
        </div>
        {accounts.length > 0 ? (
          <Button variant="primary" loading={busy === "start"} onClick={() => void startLogin()}>
            <LogIn aria-hidden="true" />{t("accounts.connect")}
          </Button>
        ) : null}
      </header>

      {errorKey ? <p className="accounts-page__error" role="alert">{t(errorKey)}</p> : null}

      {accounts.length === 0 ? (
        <Card>
          <EmptyState
            icon={<ShieldCheck />}
            label={t("empty.previewLabel")}
            title={t("page.accounts.emptyTitle")}
            description={t("accounts.emptyDescription")}
            action={<Button variant="primary" onClick={() => void startLogin()}>{t("accounts.connect")}</Button>}
          />
        </Card>
      ) : (
        <div className="account-grid" role="list" aria-label={t("accounts.listLabel")}>
          {accounts.map((account) => {
            const active = snapshot?.activeAccountId === account.id;
            const relogin = account.sessionState === "relogin-required";
            return (
              <Card className="account-card" role="listitem" key={account.id}>
                <div className="account-card__identity">
                  <span className="account-card__avatar" aria-hidden="true"><UserRound /></span>
                  <div><h2>{account.username}</h2><p>{t("accounts.microsoft")}</p></div>
                  <Badge tone={relogin ? "warning" : "success"}>
                    {relogin ? t("accounts.reloginRequired") : t("accounts.verified")}
                  </Badge>
                </div>
                <dl className="account-card__details">
                  <div><dt>{t("accounts.ownership")}</dt><dd>{formatDate(account.ownershipVerifiedAtUnix * 1000)}</dd></div>
                  <div><dt>{t("accounts.offlinePolicy")}</dt><dd>{t("accounts.notConfigured")}</dd></div>
                </dl>
                <div className="account-card__actions">
                  <Button
                    variant={active ? "secondary" : "primary"}
                    disabled={active || relogin}
                    loading={busy === `select:${account.id}`}
                    onClick={() => void selectAccount(account.id)}
                  >
                    {active ? <Check aria-hidden="true" /> : null}{active ? t("accounts.active") : t("accounts.select")}
                  </Button>
                  <Button loading={busy === `refresh:${account.id}`} onClick={() => void refreshAccount(account.id)}>
                    <RefreshCcw aria-hidden="true" />{t("accounts.refresh")}
                  </Button>
                  <Button variant="danger" onClick={() => setRemoveTarget(account)}>
                    <Trash2 aria-hidden="true" />{t("accounts.logout")}
                  </Button>
                </div>
              </Card>
            );
          })}
        </div>
      )}

      <Dialog
        open={Boolean(prompt)}
        title={t("accounts.deviceTitle")}
        description={t("accounts.deviceDescription")}
        onClose={() => void cancelLogin()}
        footer={<><Button onClick={() => void cancelLogin()}>{t("app.cancel")}</Button><Button variant="primary" loading={busy === "complete"} onClick={() => void finishLogin()}>{t("accounts.completed")}</Button></>}
      >
        {prompt ? <div className="device-login">
          <span className="device-login__code" aria-label={t("accounts.userCode")}>{prompt.userCode}</span>
          <Button onClick={() => void openMicrosoftVerification(prompt.verificationUri).catch(report)}>
            <ExternalLink aria-hidden="true" />{t("accounts.openMicrosoft")}
          </Button>
          <p>{t("accounts.secretNote")}</p>
        </div> : null}
      </Dialog>

      <ConfirmDialog
        open={Boolean(removeTarget)}
        title={t("accounts.logoutTitle")}
        description={t("accounts.logoutDescription", { name: removeTarget?.username ?? "" })}
        confirmLabel={t("accounts.logout")}
        cancelLabel={t("app.cancel")}
        loading={Boolean(removeTarget && busy === `remove:${removeTarget.id}`)}
        onClose={() => setRemoveTarget(null)}
        onConfirm={() => void removeAccount()}
      />
    </div>
  );
}
