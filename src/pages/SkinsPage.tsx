import { useEffect, useMemo, useRef, useState } from "react";
import * as skinview3d from "skinview3d";
import {
  Check,
  FolderOpen,
  Pencil,
  Plus,
  Search,
  ShieldCheck,
  Trash2,
  Upload,
  X,
} from "lucide-react";
import { authCommands } from "../lib/authCommands";
import { importSNinePlayerSkin, loadSNinePlayerSkin } from "../lib/snineClientBridge";

type SkinModel = "classic" | "slim";
type Skin = { id: string; name: string; url: string; model: SkinModel; source?: "file" | "url" };

const LIBRARY_KEY = "snine.skin.library";
const ACTIVE_SKIN_KEY = "snine.active.skin";
const ACTIVE_MODEL_KEY = "snine.active.skin.model";
const SKIN_CHANGE_EVENT = "snine-active-skin-changed";

function readLibrary(): Skin[] {
  try {
    const parsed = JSON.parse(localStorage.getItem(LIBRARY_KEY) || "[]") as Array<Partial<Skin>>;
    return parsed
      .filter((skin): skin is Partial<Skin> & { url: string } => typeof skin.url === "string" && Boolean(skin.url))
      .map((skin, index) => ({
        id: skin.id || `legacy-${index}-${skin.url.slice(-16)}`,
        name: skin.name || `Skin ${index + 1}`,
        url: skin.url,
        model: skin.model === "slim" ? "slim" : "classic",
        source: skin.source === "url" ? "url" : "file",
      }));
  } catch {
    return [];
  }
}

function applySkin(skin?: Skin) {
  if (skin) {
    localStorage.setItem(ACTIVE_SKIN_KEY, skin.url);
    localStorage.setItem(ACTIVE_MODEL_KEY, skin.model);
  } else {
    localStorage.removeItem(ACTIVE_SKIN_KEY);
    localStorage.removeItem(ACTIVE_MODEL_KEY);
  }
  window.dispatchEvent(new Event(SKIN_CHANGE_EVENT));
}

function SkinPreview({ skin, model }: { skin: string; model: SkinModel }) {
  const ref = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    if (!ref.current || !skin) return;
    const viewer = new skinview3d.SkinViewer({ canvas: ref.current, width: 250, height: 330, skin });
    viewer.controls.enableRotate = true;
    viewer.controls.enableZoom = false;
    viewer.zoom = 0.82;
    viewer.animation = new skinview3d.IdleAnimation();
    void viewer.loadSkin(skin, { model: model === "slim" ? "slim" : "default" });
    return () => viewer.dispose();
  }, [model, skin]);

  return <canvas ref={ref} />;
}

async function fileToDataUrl(file: File): Promise<string> {
  const dataUrl = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("Die PNG-Datei konnte nicht gelesen werden."));
    reader.onload = () => resolve(String(reader.result));
    reader.readAsDataURL(file);
  });
  await new Promise<void>((resolve, reject) => {
    const image = new Image();
    image.onload = () => {
      if (image.width === 64 && (image.height === 64 || image.height === 32)) resolve();
      else reject(new Error("Minecraft-Skins müssen 64×64 oder 64×32 Pixel groß sein."));
    };
    image.onerror = () => reject(new Error("Die Datei ist keine gültige PNG-Grafik."));
    image.src = dataUrl;
  });
  return dataUrl;
}

function remoteSkinUrl(value: string): string {
  const input = value.trim();
  if (/^https?:\/\//i.test(input) || input.startsWith("data:image/png")) return input;
  return `https://mc-heads.net/skin/${encodeURIComponent(input)}`;
}

export function SkinsPage() {
  const [skins, setSkins] = useState<Skin[]>(readLibrary);
  const [active, setActive] = useState(() => localStorage.getItem(ACTIVE_SKIN_KEY) || "");
  const [query, setQuery] = useState("");
  const [dialogOpen, setDialogOpen] = useState(false);
  const [skinName, setSkinName] = useState("");
  const [skinInput, setSkinInput] = useState("");
  const [skinFile, setSkinFile] = useState<File | null>(null);
  const [skinModel, setSkinModel] = useState<SkinModel>("classic");
  const [formError, setFormError] = useState("");
  const [saving, setSaving] = useState(false);
  const [officialSkin, setOfficialSkin] = useState<{ name: string; url: string; model: SkinModel } | null>(null);

  useEffect(() => {
    let disposed = false;
    void authCommands.snapshot().then(async (snapshot) => {
      const account = snapshot.accounts.find((item) => item.id === snapshot.activeAccountId) ?? snapshot.accounts[0];
      if (!account) return;
      const skin = await loadSNinePlayerSkin(account.id, account.username);
      if (!disposed && skin.textureDataUrl) {
        setOfficialSkin({ name: account.username, url: skin.textureDataUrl, model: skin.model });
      }
    }).catch((error) => console.warn("[SNine Launcher] Skin library account load failed", error));
    return () => { disposed = true; };
  }, []);

  const visibleSkins = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    return normalized ? skins.filter((skin) => skin.name.toLocaleLowerCase().includes(normalized)) : skins;
  }, [query, skins]);

  const saveLibrary = (next: Skin[]) => {
    setSkins(next);
    localStorage.setItem(LIBRARY_KEY, JSON.stringify(next));
  };

  const closeDialog = () => {
    if (saving) return;
    setDialogOpen(false);
    setSkinName("");
    setSkinInput("");
    setSkinFile(null);
    setSkinModel("classic");
    setFormError("");
  };

  const addSkin = async () => {
    if (!skinFile && !skinInput.trim()) {
      setFormError("Wähle eine PNG-Datei oder gib einen Spielernamen beziehungsweise eine URL ein.");
      return;
    }
    setSaving(true);
    setFormError("");
    try {
      const directUrl = /^https?:\/\//i.test(skinInput.trim()) || skinInput.trim().startsWith("data:image/png");
      const imported = !skinFile && !directUrl ? await importSNinePlayerSkin(skinInput.trim()) : null;
      if (imported && (!imported.ok || !imported.textureDataUrl)) throw new Error("Für diesen Minecraft-Spieler wurde kein Skin gefunden.");
      const url = skinFile ? await fileToDataUrl(skinFile) : imported?.textureDataUrl || remoteSkinUrl(skinInput);
      const fallbackName = skinFile?.name.replace(/\.png$/i, "") || imported?.playerName || skinInput.trim().replace(/^https?:\/\//i, "").split(/[/?#]/)[0];
      const skin: Skin = {
        id: crypto.randomUUID(),
        name: skinName.trim() || fallbackName || "Neuer Skin",
        url,
        model: imported?.model || skinModel,
        source: skinFile ? "file" : "url",
      };
      saveLibrary([skin, ...skins]);
      closeDialog();
    } catch (error) {
      setFormError(error instanceof Error ? error.message : "Der Skin konnte nicht hinzugefügt werden.");
    } finally {
      setSaving(false);
    }
  };

  const equip = (skin?: Skin) => {
    applySkin(skin);
    setActive(skin?.url || "");
  };

  const remove = (skin: Skin) => {
    const next = skins.filter((item) => item.id !== skin.id);
    saveLibrary(next);
    if (active === skin.url) equip(undefined);
  };

  const toggleModel = (skin: Skin) => {
    const model: SkinModel = skin.model === "classic" ? "slim" : "classic";
    const next = skins.map((item) => item.id === skin.id ? { ...item, model } : item);
    saveLibrary(next);
    if (active === skin.url) {
      localStorage.setItem(ACTIVE_MODEL_KEY, model);
      window.dispatchEvent(new Event(SKIN_CHANGE_EVENT));
    }
  };

  return (
    <section className="snine-skins-page">
      <div className="snine-skins-page__inner">
        <header className="snine-skins-heading">
          <div>
            <small>SNINE LAUNCHER / SPIELER</small>
            <h1>Deine Skin-Sammlung</h1>
            <p>Importiere Minecraft-Skins, prüfe sie in 3D und wechsle dein Aussehen direkt im Launcher.</p>
          </div>
          <button type="button" className="snine-skins-add" onClick={() => setDialogOpen(true)}><Plus aria-hidden="true" /> SKIN HINZUFÜGEN</button>
        </header>

        <div className="snine-skins-toolbar">
          <label className="snine-skins-search">
            <Search aria-hidden="true" />
            <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Skins durchsuchen..." />
            {query ? <button type="button" onClick={() => setQuery("")} aria-label="Suche leeren"><X aria-hidden="true" /></button> : null}
          </label>
          <div className="snine-skins-count"><span>{skins.length + (officialSkin ? 1 : 0)}</span> SKINS GESPEICHERT</div>
        </div>

        <div className="snine-skins-grid">
          {!query && officialSkin ? (
            <article className={`snine-skin-card snine-skin-card--official${active === "" ? " is-active" : ""}`}>
              <div className="snine-skin-card__badges"><span><ShieldCheck aria-hidden="true" /> MICROSOFT</span>{active === "" ? <b><Check aria-hidden="true" /> AKTIV</b> : null}</div>
              <div className="snine-skin-card__preview"><SkinPreview skin={officialSkin.url} model={officialSkin.model} /></div>
              <footer>
                <div><strong>{officialSkin.name}</strong><small>{officialSkin.model === "slim" ? "SCHLANK" : "KLASSISCH"} · ACCOUNT-SKIN</small></div>
                <button type="button" onClick={() => equip(undefined)}>{active === "" ? "AUSGERÜSTET" : "AUSRÜSTEN"}</button>
              </footer>
            </article>
          ) : null}

          {!query ? (
            <button type="button" className="snine-skin-import-card" onClick={() => setDialogOpen(true)}>
              <span><Upload aria-hidden="true" /></span>
              <strong>Neuen Skin hinzufügen</strong>
              <small>PNG hochladen oder über Spielernamen importieren</small>
            </button>
          ) : null}

          {visibleSkins.map((skin) => (
            <article className={`snine-skin-card${active === skin.url ? " is-active" : ""}`} key={skin.id}>
              <div className="snine-skin-card__badges">
                <span>{skin.source === "url" ? "ONLINE" : "LOKAL"}</span>
                {active === skin.url ? <b><Check aria-hidden="true" /> AKTIV</b> : null}
              </div>
              <div className="snine-skin-card__tools">
                <button type="button" onClick={() => toggleModel(skin)} title="Arm-Modell wechseln"><Pencil aria-hidden="true" /></button>
                <button type="button" onClick={() => remove(skin)} title="Skin löschen"><Trash2 aria-hidden="true" /></button>
              </div>
              <div className="snine-skin-card__preview"><SkinPreview skin={skin.url} model={skin.model} /></div>
              <footer>
                <div><strong>{skin.name}</strong><small>{skin.model === "slim" ? "SCHLANK" : "KLASSISCH"}</small></div>
                <button type="button" onClick={() => equip(skin)}>{active === skin.url ? "AUSGERÜSTET" : "AUSRÜSTEN"}</button>
              </footer>
            </article>
          ))}
        </div>

        {query && visibleSkins.length === 0 ? <div className="snine-skins-empty">Kein Skin passt zu „{query}“.</div> : null}
      </div>

      {dialogOpen ? (
        <div className="snine-skin-dialog-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) closeDialog(); }}>
          <section className="snine-skin-dialog" role="dialog" aria-modal="true" aria-labelledby="snine-skin-dialog-title">
            <header><div><small>NEUER EINTRAG</small><h2 id="snine-skin-dialog-title">Skin hinzufügen</h2></div><button type="button" onClick={closeDialog} aria-label="Schließen"><X aria-hidden="true" /></button></header>
            <div className="snine-skin-dialog__body">
              <label><span>ANZEIGENAME</span><input value={skinName} onChange={(event) => setSkinName(event.target.value)} placeholder="Zum Beispiel: Mein Main Skin" /></label>
              <label><span>SPIELERNAME ODER DIREKTE PNG-URL</span><input value={skinInput} onChange={(event) => { setSkinInput(event.target.value); setSkinFile(null); }} placeholder="silasO5xe oder https://.../skin.png" /></label>
              <div className="snine-skin-dialog__divider"><span>ODER</span></div>
              <label className={`snine-skin-file${skinFile ? " has-file" : ""}`}>
                <FolderOpen aria-hidden="true" />
                <span><strong>{skinFile?.name || "PNG-Datei auswählen"}</strong><small>64×64 oder klassisch 64×32 Pixel</small></span>
                <input type="file" accept="image/png" onChange={(event) => { setSkinFile(event.target.files?.[0] ?? null); setSkinInput(""); }} />
              </label>
              <fieldset><legend>ARM-MODELL</legend><button type="button" className={skinModel === "classic" ? "is-active" : ""} onClick={() => setSkinModel("classic")}>KLASSISCH</button><button type="button" className={skinModel === "slim" ? "is-active" : ""} onClick={() => setSkinModel("slim")}>SCHLANK</button></fieldset>
              {formError ? <p className="snine-skin-dialog__error">{formError}</p> : null}
            </div>
            <footer><button type="button" onClick={closeDialog}>ABBRECHEN</button><button type="button" className="is-primary" onClick={() => void addSkin()} disabled={saving}>{saving ? "WIRD GELADEN..." : "SKIN HINZUFÜGEN"}</button></footer>
          </section>
        </div>
      ) : null}
    </section>
  );
}
