import { useMemo, useState } from "react";
import { Copy, Plus, Server, Trash2 } from "lucide-react";
import { Button, Card, Dialog, EmptyState, TextField } from "../components/ui";
import { useI18n } from "../i18n/I18nProvider";

interface SavedServer { id: string; name: string; address: string; }

export function ServerPage() {
  const { t } = useI18n();
  const [servers, setServers] = useState<SavedServer[]>([]);
  const [draft, setDraft] = useState<SavedServer | null>(null);
  const sorted = useMemo(() => [...servers].sort((a, b) => a.name.localeCompare(b.name)), [servers]);
  const canSave = Boolean(draft?.name.trim() && draft.address.trim());
  const copyAddress = async (address: string) => { await navigator.clipboard?.writeText(address); };
  const open = () => setDraft({ id: crypto.randomUUID(), name: "", address: "" });
  return <div className="page servers-page"><header className="page-heading"><div><p className="page-eyebrow">{t("app.name")}</p><h1>{t("servers.title")}</h1><p>{t("servers.description")}</p></div><Button variant="primary" onClick={open}><Plus aria-hidden="true" />{t("servers.add")}</Button></header>{sorted.length ? <section className="server-grid">{sorted.map((server) => <Card className="server-card" key={server.id}><Server aria-hidden="true" /><div><h2>{server.name}</h2><p>{server.address}</p></div><Button onClick={() => void copyAddress(server.address)}><Copy aria-hidden="true" />{t("servers.copy")}</Button><Button variant="danger" title={t("servers.remove")} onClick={() => setServers((current) => current.filter((item) => item.id !== server.id))}><Trash2 aria-hidden="true" /></Button></Card>)}</section> : <Card><EmptyState icon={<Server />} label={t("servers.title")} title={t("servers.emptyTitle")} description={t("servers.emptyDescription")} action={<Button variant="primary" onClick={open}>{t("servers.add")}</Button>} /></Card>}<Dialog open={Boolean(draft)} title={t("servers.addTitle")} description={t("servers.addDescription")} onClose={() => setDraft(null)} footer={<><Button onClick={() => setDraft(null)}>{t("app.cancel")}</Button><Button variant="primary" disabled={!canSave} onClick={() => { if (draft) setServers((current) => [...current, { ...draft, name: draft.name.trim(), address: draft.address.trim() }]); setDraft(null); }}>{t("app.save")}</Button></>}>{draft ? <div className="server-dialog-fields"><TextField autoFocus label={t("servers.name")} value={draft.name} onChange={(event) => setDraft({ ...draft, name: event.currentTarget.value })} /><TextField label={t("servers.address")} value={draft.address} placeholder={t("servers.addressPlaceholder")} onChange={(event) => setDraft({ ...draft, address: event.currentTarget.value })} /></div> : null}</Dialog></div>;
}
