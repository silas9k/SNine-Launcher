import { useReleaseText } from "../i18n/releaseUiText";
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import {
  AlertTriangle,
  Check,
  CheckCircle2,
  Download,
  Eye,
  Heart,
  LoaderCircle,
  Search,
  ShieldAlert,
  Sparkles,
  Upload,
  X,
} from "lucide-react";
import { LauncherSkinPreview } from "../components/player/LauncherSkinPreview";
import { authCommands } from "../lib/authCommands";
import {
  capeCommands,
  type CapeTemplate,
  type CustomCapeView,
  type VanillaCapeView,
} from "../lib/capeCommands";
import type { LauncherCosmeticAsset } from "../lib/snineClientBridge";

let guidelinesAcceptedForSession = false;
const MAX_CAPE_BYTES = 1024 * 1024;
type CapeTab = "all" | "mine" | "favorites" | "vanilla";
type Account = { id: string; username: string };

type PreviewCapeState =
  | {
      type: "custom";
      cape: CustomCapeView;
      texture: string | null;
    }
  | {
      type: "vanilla";
      cape: VanillaCapeView;
      texture: string | null;
    };

function messageForError(error: unknown): string {
  const raw = error instanceof Error ? error.message : String(error ?? "");
  if (raw.includes("custom_cape_submission_pending")) return "Du hast bereits ein Cape, das noch geprüft wird.";
  if (raw.includes("custom_cape_dimensions_invalid")) return "Cape-Texturen müssen ein 2:1-Format haben und dürfen maximal 512×256 Pixel groß sein.";
  if (raw.includes("custom_cape_too_large")) return "Das Cape ist zu groß. Maximal erlaubt sind 1 MiB.";
  if (raw.includes("custom_cape_not_png")) return "Bitte wähle eine echte PNG-Datei aus.";
  if (raw.includes("minecraft_session")) return "Dein Minecraft-Login ist abgelaufen. Melde dich im Launcher erneut an.";
  if (raw.includes("custom_cape_http_503")) return "Das SNine Cape-Backend ist gerade nicht verfügbar.";
  return raw || "Die Cape-Anfrage ist fehlgeschlagen.";
}

async function validateCapeFile(file: File): Promise<string> {
  if (!file.name.toLowerCase().endsWith(".png") || (file.type && file.type !== "image/png")) {
    throw new Error("Bitte wähle eine PNG-Datei aus.");
  }
  if (file.size <= 0 || file.size > MAX_CAPE_BYTES) {
    throw new Error("Das Cape darf maximal 1 MiB groß sein.");
  }
  const dataUrl = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("Die PNG-Datei konnte nicht gelesen werden."));
    reader.onload = () => resolve(String(reader.result));
    reader.readAsDataURL(file);
  });
  await new Promise<void>((resolve, reject) => {
    const image = new Image();
    image.onerror = () => reject(new Error("Die Datei ist keine gültige PNG-Grafik."));
    image.onload = () => {
      const valid = image.width >= 64 && image.width <= 512
        && image.height >= 32 && image.height <= 256
        && image.width === image.height * 2
        && image.width % 64 === 0
        && image.height % 32 === 0;
      if (valid) resolve();
      else reject(new Error("Erlaubt sind 2:1-Cape-Texturen bis maximal 512×256 Pixel (64×32, 128×64, 256×128 oder 512×256)."));
    };
    image.src = dataUrl;
  });
  return dataUrl;
}

function CapeTexturePreview({ src, label }: { src: string | null | undefined; label: string }) {
  const canvas = useRef<HTMLCanvasElement | null>(null);
  useEffect(() => {
    const element = canvas.current;
    if (!element || !src) return;
    let disposed = false;
    const image = new Image();
    image.onload = () => {
      if (disposed || !element) return;
      const scale = image.width / 64;
      const sx = Math.max(0, Math.round(scale));
      const sy = Math.max(0, Math.round(scale));
      const sw = Math.max(1, Math.round(10 * scale));
      const sh = Math.max(1, Math.round(16 * scale));
      element.width = 132;
      element.height = 196;
      const context = element.getContext("2d");
      if (!context) return;
      context.clearRect(0, 0, element.width, element.height);
      context.imageSmoothingEnabled = false;
      context.drawImage(image, sx, sy, sw, sh, 0, 0, element.width, element.height);
    };
    image.src = src;
    return () => { disposed = true; };
  }, [src]);

  return src
    ? <canvas ref={canvas} className="snine-cape-texture-preview" role="img" aria-label={label} />
    : <div className="snine-cape-texture-placeholder" aria-label={label}><span>?</span></div>;
}

function GuidelinesDialog({ onClose, onAccept }: { onClose: () => void; onAccept: () => void }) {
  const rt = useReleaseText();
  return (
    <div className="snine-cape-modal-backdrop" role="presentation">
      <section className="snine-cape-guidelines" role="dialog" aria-modal="true" aria-labelledby="cape-guidelines-title">
        <header>
          <div><ShieldAlert aria-hidden="true" /><strong id="cape-guidelines-title">{rt("CAPE GUIDELINES")}</strong></div>
          <button type="button" onClick={onClose} aria-label={rt("Schließen")}><X aria-hidden="true" /></button>
        </header>
        <div className="snine-cape-guidelines__body">
          <small>{rt("BEFORE YOU UPLOAD")}</small>
          <h2>{rt("Bitte lies und akzeptiere die Regeln für Custom Capes.")}</h2>
          <div className="snine-cape-guidelines__rules">
            <ul>
              <li>{rt("Keine urheberrechtlich geschützten Capes von anderen Clients, Marken oder Spielen.")}</li>
              <li>{rt("Keine expliziten, beleidigenden oder diskriminierenden Inhalte.")}</li>
              <li>{rt("Keine politischen oder extremistischen Inhalte.")}</li>
              <li>{rt("Keine Werbung, Scam-Links oder absichtlich irreführenden Motive.")}</li>
              <li>{rt("Du musst das Recht haben, die hochgeladene Grafik zu verwenden.")}</li>
              <li>{rt("Die PNG muss im 2:1-Format vorliegen und darf maximal 512×256 Pixel groß sein.")}</li>
            </ul>
            <p><AlertTriangle aria-hidden="true" /> {rt("Verstöße können zu einer temporären oder permanenten Upload-Sperre führen.")}</p>
          </div>
        </div>
        <footer>
          <button type="button" className="is-primary" onClick={onAccept}><CheckCircle2 aria-hidden="true" /> {rt("AKZEPTIEREN & WEITER")}</button>
        </footer>
      </section>
    </div>
  );
}

function UploadDialog({ account, initialTemplate, onClose, onUploaded }: { account: Account; initialTemplate: CapeTemplate; onClose: () => void; onUploaded: () => void }) {
  const rt = useReleaseText();
  const [capeName, setCapeName] = useState("");
  const [template, setTemplate] = useState<CapeTemplate>(initialTemplate);
  const [file, setFile] = useState<File | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [templateDownload, setTemplateDownload] = useState("");

  const chooseFile = async (next: File | null) => {
    setFile(next);
    setPreview(null);
    setError("");
    if (!next) return;
    try {
      setPreview(await validateCapeFile(next));
      if (!capeName.trim()) setCapeName(next.name.replace(/\.png$/i, "").slice(0, 32));
    } catch (cause) {
      setFile(null);
      setError(messageForError(cause));
    }
  };

  const downloadTemplate = async () => {
    setError("");
    setTemplateDownload("");
    try {
      const saved = await capeCommands.saveTemplate();
      setTemplateDownload(`Vorlage gespeichert: ${saved}`);
    } catch (cause) {
      setError(messageForError(cause));
    }
  };

  const submit = async () => {
    if (!file || !preview) { setError("Wähle zuerst eine gültige Cape-PNG aus."); return; }
    if (!capeName.trim()) { setError("Gib deinem Cape einen Namen."); return; }
    setSubmitting(true);
    setError("");
    try {
      await capeCommands.upload({ accountId: account.id, username: account.username }, capeName.trim(), template, preview);
      onUploaded();
    } catch (cause) {
      setError(messageForError(cause));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="snine-cape-modal-backdrop" role="presentation">
      <section className="snine-cape-upload-dialog" role="dialog" aria-modal="true" aria-labelledby="cape-upload-title">
        <header>
          <div><small>{rt("CUSTOM CAPE")}</small><h2 id="cape-upload-title">{rt("Cape hochladen")}</h2></div>
          <button type="button" onClick={onClose} disabled={submitting} aria-label={rt("Schließen")}><X aria-hidden="true" /></button>
        </header>
        <div className="snine-cape-upload-dialog__body">
          <div className="snine-cape-upload-preview">
            <CapeTexturePreview src={preview} label="Cape Vorschau" />
            <span>{file ? `${file.name} · ${(file.size / 1024).toFixed(0)} KiB` : "PNG · 2:1 · max. 512×256 · max. 1 MiB"}</span>
          </div>
          <div className="snine-cape-upload-fields">
            <label><span>{rt("CAPE NAME")}</span><input value={capeName} maxLength={32} onChange={(event) => setCapeName(event.target.value)} placeholder={rt("Mein Cape")} /></label>
            <fieldset>
              <legend>{rt("VORLAGE")}</legend>
              <button type="button" className={template === "CAPE" ? "is-active" : ""} onClick={() => setTemplate("CAPE")}>{rt("NUR CAPE")}</button>
              <button type="button" className={template === "CAPE_ELYTRA" ? "is-active" : ""} onClick={() => setTemplate("CAPE_ELYTRA")}>{rt("CAPE + ELYTRA")}</button>
            </fieldset>
            <button type="button" className="snine-cape-template-download" onClick={() => void downloadTemplate()}>
              <Download aria-hidden="true" />
              <span><strong>{rt("VORLAGE HERUNTERLADEN")}</strong><small>{rt("Offizielles SNine Template · 512×256 PNG")}</small></span>
            </button>
            {templateDownload ? <p className="snine-cape-template-download__status"><CheckCircle2 aria-hidden="true" /> {templateDownload}</p> : null}
            <label className={`snine-cape-file${file ? " has-file" : ""}`}>
              <Upload aria-hidden="true" />
              <span><strong>{file ? "Andere PNG wählen" : "PNG auswählen"}</strong><small>{rt("Die Datei wird vor dem Upload lokal geprüft.")}</small></span>
              <input type="file" accept="image/png,.png" onChange={(event) => void chooseFile(event.target.files?.[0] ?? null)} />
            </label>
            {error ? <p className="snine-cape-form-error">{error}</p> : null}
          </div>
        </div>
        <footer>
          <span>{rt("Nach dem Upload prüft ein SNine-Admin dein Cape über Discord.")}</span>
          <div><button type="button" onClick={onClose} disabled={submitting}>{rt("ABBRECHEN")}</button><button type="button" className="is-primary" onClick={() => void submit()} disabled={submitting || !preview}>{submitting ? <LoaderCircle className="ui-spin" /> : <Upload />} {rt("HOCHLADEN")}</button></div>
        </footer>
      </section>
    </div>
  );
}

function CapeInspectDialog({
  account,
  preview,
  onClose,
  onEquip,
  onUnequip,
}: {
  account: Account;
  preview: PreviewCapeState;
  onClose: () => void;
  onEquip: (cape: CustomCapeView) => Promise<void>;
  onUnequip: () => Promise<void>;
}) {
  const rt = useReleaseText();
  const [busy, setBusy] = useState(false);
  const [cameraYaw, setCameraYaw] = useState(180);

  const cosmeticAssets = useMemo<LauncherCosmeticAsset[]>(() => {
    if (!preview.texture) return [];
    const template = preview.type === "custom" ? preview.cape.template : "CAPE";
    return [{
      id: preview.cape.id,
      kind: "cape",
      name: preview.type === "custom" ? preview.cape.capeName : preview.cape.name,
      textureDataUrl: preview.texture,
      model: null,
      definition: {
        id: preview.cape.id,
        type: "cape",
        template,
        source: preview.type,
      },
    }];
  }, [preview]);

  const title = preview.type === "custom" ? preview.cape.capeName : preview.cape.name;
  const subtitle = preview.type === "custom" ? `von ${preview.cape.ownerName}` : "Vanilla Minecraft";
  const active = preview.type === "custom" ? preview.cape.selected : preview.cape.state === "ACTIVE";

  const handleEquip = async () => {
    if (preview.type !== "custom" || preview.cape.status !== "APPROVED") return;
    setBusy(true);
    try {
      await onEquip(preview.cape);
      onClose();
    } finally {
      setBusy(false);
    }
  };

  const handleUnequip = async () => {
    setBusy(true);
    try {
      await onUnequip();
      onClose();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="snine-cape-modal-backdrop" role="presentation" onClick={onClose}>
      <section className="snine-cape-inspect-dialog" role="dialog" aria-modal="true" aria-labelledby="cape-inspect-title" onClick={(event) => event.stopPropagation()}>
        <header>
          <div>
            <small>{preview.type === "custom" ? "COMMUNITY CAPE" : "VANILLA CAPE"}</small>
            <h2 id="cape-inspect-title">{title}</h2>
            <p>{subtitle}</p>
          </div>
          <button type="button" onClick={onClose} aria-label={rt("Schließen")}><X aria-hidden="true" /></button>
        </header>

        <div className="snine-cape-inspect-dialog__body">
          <div className="snine-cape-inspect-dialog__stage">
            <div className="snine-cape-inspect-dialog__viewer">
              <LauncherSkinPreview
                accountId={account.id}
                playerName={account.username}
                reducedMotion={false}
                cosmetics={cosmeticAssets}
                cameraYaw={cameraYaw}
                cameraPitch={1.5}
                cameraDistance={76}
                cameraTargetY={16}
              />
              <div className="snine-cape-inspect-dialog__view-tabs" aria-label={rt("Ansicht wählen")}>
                <button type="button" className={cameraYaw === 0 ? "is-active" : ""} onClick={() => setCameraYaw(0)}>{rt("VORNE")}</button>
                <button type="button" className={cameraYaw === 180 ? "is-active" : ""} onClick={() => setCameraYaw(180)}>{rt("RÜCKSEITE")}</button>
              </div>
            </div>
            <div className="snine-cape-inspect-dialog__stage-note">
              <Eye aria-hidden="true" />
              <span>{rt("Ziehe zum Drehen, Mausrad zum Zoomen. Doppelklick setzt die Ansicht zurück.")}</span>
            </div>
          </div>

          <aside className="snine-cape-inspect-dialog__meta">
            <div className="snine-cape-inspect-dialog__sheet">
              <CapeTexturePreview src={preview.texture} label={`${title} Textur`} />
              <div>
                <strong>{title}</strong>
                <span>{subtitle}</span>
                <small>{preview.type === "custom" ? (preview.cape.template === "CAPE_ELYTRA" ? "CAPE + ELYTRA" : "NUR CAPE") : preview.cape.state}</small>
              </div>
            </div>

            <dl className="snine-cape-inspect-dialog__stats">
              <div>
                <dt>{rt("STATUS")}</dt>
                <dd>{preview.type === "custom" ? (preview.cape.status === "APPROVED" ? "FREIGEGEBEN" : preview.cape.status === "PENDING" ? "IN PRÜFUNG" : "ABGELEHNT") : preview.cape.state}</dd>
              </div>
              <div>
                <dt>{rt("TYP")}</dt>
                <dd>{preview.type === "custom" ? "COMMUNITY" : "VANILLA"}</dd>
              </div>
              <div>
                <dt>{rt("AKTIV")}</dt>
                <dd>{active ? "JA" : "NEIN"}</dd>
              </div>
              {preview.type === "custom" ? (
                <div>
                  <dt>{rt("VERWENDUNGEN")}</dt>
                  <dd>{preview.cape.uses.toLocaleString("de-DE")}</dd>
                </div>
              ) : null}
            </dl>

            {preview.type === "custom" && preview.cape.rejectionReason ? (
              <p className="snine-cape-inspect-dialog__rejection"><AlertTriangle aria-hidden="true" /> {preview.cape.rejectionReason}</p>
            ) : null}

            <div className="snine-cape-inspect-dialog__actions">
              {preview.type === "custom" && preview.cape.status === "APPROVED" ? (
                active ? (
                  <button type="button" onClick={() => void handleUnequip()} disabled={busy}>
                    {busy ? <LoaderCircle className="ui-spin" /> : <X aria-hidden="true" />}
                    {rt("ABLEGEN")}
                  </button>
                ) : (
                  <button type="button" className="is-primary" onClick={() => void handleEquip()} disabled={busy}>
                    {busy ? <LoaderCircle className="ui-spin" /> : <Sparkles aria-hidden="true" />}
                    {rt("AUSRÜSTEN")}
                  </button>
                )
              ) : null}
              <button type="button" onClick={onClose}>{rt("SCHLIESSEN")}</button>
            </div>
          </aside>
        </div>
      </section>
    </div>
  );
}

function toCardKeyboardHandler(callback: () => void) {
  return (event: KeyboardEvent<HTMLElement>) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      callback();
    }
  };
}

export function CapesPage() {
  const rt = useReleaseText();
  const [account, setAccount] = useState<Account | null>(null);
  const [tab, setTab] = useState<CapeTab>("all");
  const [query, setQuery] = useState("");
  const [capes, setCapes] = useState<CustomCapeView[]>([]);
  const [vanilla, setVanilla] = useState<VanillaCapeView[]>([]);
  const [selected, setSelected] = useState<CustomCapeView | null>(null);
  const [textures, setTextures] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [guidelinesOpen, setGuidelinesOpen] = useState(false);
  const [uploadOpen, setUploadOpen] = useState(false);
  const [uploadTemplate, setUploadTemplate] = useState<CapeTemplate>("CAPE");
  const [previewCape, setPreviewCape] = useState<PreviewCapeState | null>(null);

  useEffect(() => {
    let disposed = false;
    void authCommands.snapshot().then((snapshot) => {
      const current = snapshot.accounts.find((item) => item.id === snapshot.activeAccountId) ?? snapshot.accounts[0];
      if (!disposed) setAccount(current ? { id: current.id, username: current.username } : null);
    }).catch((cause) => !disposed && setError(messageForError(cause)));
    return () => { disposed = true; };
  }, []);

  const ensureTexture = useCallback(async (cape: CustomCapeView) => {
    if (textures[cape.id]) return textures[cape.id];
    const src = cape.status === "APPROVED"
      ? await capeCommands.texture(cape.id)
      : await capeCommands.preview({ accountId: account!.id, username: account!.username }, cape.id);
    setTextures((current) => ({ ...current, [cape.id]: src }));
    return src;
  }, [account, textures]);

  const loadCustom = useCallback(async () => {
    if (!account || tab === "vanilla") return;
    setLoading(true);
    setError("");
    try {
      const response = await capeCommands.list({ accountId: account.id, username: account.username }, tab, query);
      setCapes(response.capes ?? []);
      setSelected(response.selected ?? null);
      const missing = (response.capes ?? []).filter((cape) => !textures[cape.id]);
      if (missing.length) {
        const loaded = await Promise.all(missing.map(async (cape) => {
          try {
            const src = cape.status === "APPROVED"
              ? await capeCommands.texture(cape.id)
              : await capeCommands.preview({ accountId: account.id, username: account.username }, cape.id);
            return [cape.id, src] as const;
          } catch {
            return null;
          }
        }));
        setTextures((current) => {
          const next = { ...current };
          for (const item of loaded) if (item) next[item[0]] = item[1];
          return next;
        });
      }
    } catch (cause) {
      setError(messageForError(cause));
    } finally {
      setLoading(false);
    }
  }, [account, query, tab, textures]);

  useEffect(() => {
    if (!account || tab === "vanilla") return;
    const timer = window.setTimeout(() => void loadCustom(), 180);
    return () => window.clearTimeout(timer);
  }, [account, tab, query]);

  useEffect(() => {
    if (!account || tab !== "vanilla") return;
    let disposed = false;
    setLoading(true);
    setError("");
    void capeCommands.vanilla(account.id).then((response) => {
      if (!disposed) setVanilla(response.capes ?? []);
    }).catch((cause) => !disposed && setError(messageForError(cause))).finally(() => !disposed && setLoading(false));
    return () => { disposed = true; };
  }, [account, tab]);

  const visibleVanilla = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    return normalized ? vanilla.filter((cape) => cape.name.toLowerCase().includes(normalized)) : vanilla;
  }, [query, vanilla]);

  const startUpload = (template: CapeTemplate = "CAPE") => {
    setUploadTemplate(template);
    if (guidelinesAcceptedForSession) setUploadOpen(true);
    else setGuidelinesOpen(true);
  };

  const favorite = async (cape: CustomCapeView) => {
    if (!account || cape.status !== "APPROVED") return;
    try {
      await capeCommands.favorite({ accountId: account.id, username: account.username }, cape.id, !cape.favorite);
      await loadCustom();
    } catch (cause) {
      setError(messageForError(cause));
    }
  };

  const equip = useCallback(async (cape: CustomCapeView) => {
    if (!account || cape.status !== "APPROVED") return;
    try {
      await capeCommands.equip({ accountId: account.id, username: account.username }, cape.id);
      window.dispatchEvent(new CustomEvent("snine-cape-selection-changed", { detail: { capeId: cape.id } }));
      await loadCustom();
    } catch (cause) {
      setError(messageForError(cause));
      throw cause;
    }
  }, [account, loadCustom]);

  const unequip = useCallback(async () => {
    if (!account) return;
    try {
      await capeCommands.unequip({ accountId: account.id, username: account.username });
      window.dispatchEvent(new CustomEvent("snine-cape-selection-changed", { detail: { capeId: null } }));
      setSelected(null);
      await loadCustom();
    } catch (cause) {
      setError(messageForError(cause));
      throw cause;
    }
  }, [account, loadCustom]);

  const openCustomPreview = async (cape: CustomCapeView) => {
    if (!account) return;
    try {
      const texture = await ensureTexture(cape);
      setPreviewCape({ type: "custom", cape, texture });
    } catch (cause) {
      setError(messageForError(cause));
      setPreviewCape({ type: "custom", cape, texture: null });
    }
  };

  const openVanillaPreview = (cape: VanillaCapeView) => {
    setPreviewCape({ type: "vanilla", cape, texture: cape.textureDataUrl ?? null });
  };

  return (
    <section className="snine-capes-page">
      <div className="snine-capes-page__inner">
        <header className="snine-capes-heading">
          <div>
            <small>{rt("SNINE LAUNCHER / COSMETICS")}</small>
            <h1>{rt("Capes")}</h1>
            <p>{rt("Entdecke kostenlose Community-Capes, lade dein eigenes hoch und schau dir jedes Cape direkt auf deinem Skin im Launcher an.")}</p>
          </div>
          <div className="snine-capes-heading__actions">
            <button type="button" className="snine-capes-upload" onClick={() => startUpload("CAPE")}><Upload aria-hidden="true" /> {rt("HOCHLADEN")}</button>
          </div>
        </header>

        <div className="snine-capes-tabs" role="tablist">
          {([['all', 'ALLE'], ['mine', 'MEINE CAPES'], ['favorites', 'FAVORITEN'], ['vanilla', 'VANILLA']] as Array<[CapeTab, string]>).map(([value, label]) => (
            <button type="button" key={value} className={tab === value ? "is-active" : ""} onClick={() => { setTab(value); setQuery(""); }}>
              {label}
            </button>
          ))}
        </div>

        <div className="snine-capes-toolbar">
          <label>
            <Search aria-hidden="true" />
            <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={tab === "vanilla" ? "Vanilla-Capes durchsuchen..." : "Nach Spielername oder Cape suchen..."} />
            {query ? <button type="button" onClick={() => setQuery("")}><X aria-hidden="true" /></button> : null}
          </label>
          {selected ? (
            <div className="snine-capes-selected">
              <Check aria-hidden="true" />
              <span>{rt("AKTIV:")} <strong>{selected.capeName}</strong></span>
              <button type="button" onClick={() => void unequip()}>{rt("ABLEGEN")}</button>
            </div>
          ) : null}
        </div>

        {!account ? <div className="snine-capes-empty"><ShieldAlert /><h2>{rt("Kein Minecraft-Account aktiv")}</h2><p>{rt("Melde dich zuerst im Launcher mit deinem Minecraft-Account an.")}</p></div> : null}
        {error ? <div className="snine-capes-error"><AlertTriangle aria-hidden="true" /> {error}</div> : null}
        {loading && account ? <div className="snine-capes-loading"><LoaderCircle className="ui-spin" /> {rt("CAPES WERDEN GELADEN...")}</div> : null}

        {!loading && account && tab !== "vanilla" ? (
          <div className="snine-capes-grid">
            {capes.map((cape) => (
              <article
                className={`snine-cape-card${cape.selected ? " is-selected" : ""}`}
                key={cape.id}
                role="button"
                tabIndex={0}
                onClick={() => void openCustomPreview(cape)}
                onKeyDown={toCardKeyboardHandler(() => void openCustomPreview(cape))}
              >
                <div className="snine-cape-card__preview">
                  <CapeTexturePreview src={textures[cape.id]} label={cape.capeName} />
                  <button
                    type="button"
                    className={`snine-cape-favorite${cape.favorite ? " is-active" : ""}`}
                    disabled={cape.status !== "APPROVED"}
                    onClick={(event) => {
                      event.stopPropagation();
                      void favorite(cape);
                    }}
                    aria-label={rt("Favorit")}
                  >
                    <Heart aria-hidden="true" />
                  </button>
                  {cape.status !== "APPROVED" ? <span className={`snine-cape-status is-${cape.status.toLowerCase()}`}>{cape.status === "PENDING" ? "PRÜFUNG LÄUFT" : "ABGELEHNT"}</span> : null}
                </div>
                <footer>
                  <div className="snine-cape-card__copy">
                    <strong>{cape.capeName}</strong>
                    <span>{rt("von")} {cape.ownerName}</span>
                    <small>{cape.template === "CAPE_ELYTRA" ? "CAPE + ELYTRA" : "NUR CAPE"} · {cape.uses.toLocaleString("de-DE")} {rt("VERWENDUNGEN")}</small>
                    {cape.status === "REJECTED" && cape.rejectionReason ? <em>{cape.rejectionReason}</em> : null}
                  </div>
                  <div className="snine-cape-card__actions">
                    <button type="button" className="snine-cape-card__ghost" onClick={(event) => { event.stopPropagation(); void openCustomPreview(cape); }}>
                      <Eye aria-hidden="true" /> {rt("ANSEHEN")}
                    </button>
                    {cape.status === "APPROVED" ? (
                      <button
                        type="button"
                        className={cape.selected ? "is-selected" : ""}
                        onClick={(event) => {
                          event.stopPropagation();
                          void (cape.selected ? unequip() : equip(cape));
                        }}
                      >
                        {cape.selected ? "ABLEGEN" : "AUSRÜSTEN"}
                      </button>
                    ) : null}
                  </div>
                </footer>
              </article>
            ))}
            {!capes.length ? <div className="snine-capes-empty snine-capes-empty--grid"><h2>{tab === "favorites" ? "Noch keine Favoriten" : tab === "mine" ? "Noch kein Custom Cape" : "Keine Capes gefunden"}</h2><p>{tab === "mine" ? "Lade oben dein erstes eigenes Cape hoch." : "Versuche einen anderen Suchbegriff."}</p></div> : null}
          </div>
        ) : null}

        {!loading && account && tab === "vanilla" ? (
          <div className="snine-capes-grid">
            {visibleVanilla.map((cape) => (
              <article
                className="snine-cape-card snine-cape-card--vanilla"
                key={cape.id}
                role="button"
                tabIndex={0}
                onClick={() => openVanillaPreview(cape)}
                onKeyDown={toCardKeyboardHandler(() => openVanillaPreview(cape))}
              >
                <div className="snine-cape-card__preview">
                  <CapeTexturePreview src={cape.textureDataUrl} label={cape.name} />
                  {cape.state === "ACTIVE" ? <span className="snine-cape-status is-approved">{rt("MINECRAFT AKTIV")}</span> : null}
                </div>
                <footer>
                  <div className="snine-cape-card__copy">
                    <strong>{cape.name}</strong>
                    <span>{rt("Vanilla Minecraft")}</span>
                    <small>{cape.state}</small>
                  </div>
                  <div className="snine-cape-card__actions">
                    <button type="button" className="snine-cape-card__ghost" onClick={(event) => { event.stopPropagation(); openVanillaPreview(cape); }}>
                      <Eye aria-hidden="true" /> {rt("ANSEHEN")}
                    </button>
                  </div>
                </footer>
              </article>
            ))}
            {!visibleVanilla.length ? <div className="snine-capes-empty snine-capes-empty--grid"><h2>{rt("Keine Vanilla-Capes gefunden")}</h2><p>{rt("Hier erscheinen die offiziellen Capes deines Microsoft-/Minecraft-Accounts.")}</p></div> : null}
          </div>
        ) : null}
      </div>

      {guidelinesOpen ? <GuidelinesDialog onClose={() => setGuidelinesOpen(false)} onAccept={() => { guidelinesAcceptedForSession = true; setGuidelinesOpen(false); setUploadOpen(true); }} /> : null}
      {uploadOpen && account ? <UploadDialog account={account} initialTemplate={uploadTemplate} onClose={() => setUploadOpen(false)} onUploaded={() => { setUploadOpen(false); setTab("mine"); void loadCustom(); }} /> : null}
      {previewCape && account ? <CapeInspectDialog account={account} preview={previewCape} onClose={() => setPreviewCape(null)} onEquip={equip} onUnequip={unequip} /> : null}
    </section>
  );
}
