import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { Accessibility, Box, CircleOff, Eye, RotateCcw, ShieldCheck, Sparkles } from "lucide-react";
import { authCommands } from "../../lib/authCommands";
import { useI18n } from "../../i18n/I18nProvider";
import { useShellStore } from "../../app/shellStore";
import { Badge, Button, Switch } from "../ui";

type ViewerModule = typeof import("skinview3d");
type Viewer = import("skinview3d").SkinViewer;
type AnimationName = "idle" | "walk" | "wave";
type Equipment = "cape" | "wings" | "none";

function svgTexture(width: number, height: number, body: string): string {
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" shape-rendering="crispEdges">${body}</svg>`)}`;
}

const LOCAL_SKIN = svgTexture(64, 64, `
  <rect width="64" height="64" fill="none"/>
  <path fill="#d9a47e" d="M0 0h32v16H0z"/><path fill="#151823" d="M8 0h16v8H8z"/>
  <path fill="#202433" d="M16 16h24v16H16z"/><path fill="#a51f3d" d="M20 20h16v12H20z"/>
  <path fill="#d9a47e" d="M40 16h16v16H40zM32 48h16v16H32z"/>
  <path fill="#202433" d="M0 16h16v16H0zM16 48h16v16H16z"/>
  <path fill="#11131c" d="M8 8h8v2H8z"/><path fill="#dcecff" d="M9 9h2v2H9zM14 9h2v2h-2z"/>
  <path fill="#c62847" fill-opacity=".72" d="M32 0h32v16H32zM16 32h48v16H16z"/>
`);

const LOCAL_CAPE = svgTexture(64, 32, `
  <rect width="64" height="32" fill="#151823"/><rect x="1" y="1" width="62" height="30" fill="#232838"/>
  <path fill="#c62847" d="M24 5h16v4H24zM20 9h24v14H20zM24 23h16v4H24z"/>
  <path fill="#f4c7d1" d="M28 10h8v12h-8z"/>
`);

function animationFor(module: ViewerModule, name: AnimationName) {
  if (name === "walk") return new module.WalkingAnimation();
  if (name === "wave") return new module.WaveAnimation("right");
  return new module.IdleAnimation();
}

export function PlayerStage() {
  const { t } = useI18n();
  const reducedMotion = useShellStore((state) => state.settings.reducedMotion);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const anchorRef = useRef<HTMLDivElement>(null);
  const viewerRef = useRef<Viewer | null>(null);
  const moduleRef = useRef<ViewerModule | null>(null);
  const [state, setState] = useState<"loading" | "ready" | "fallback">("loading");
  const [playerName, setPlayerName] = useState("S9Lab");
  const [model, setModel] = useState<"classic" | "slim">("classic");
  const [outerLayer, setOuterLayer] = useState(true);
  const [equipment, setEquipment] = useState<Equipment>("cape");
  const [halo, setHalo] = useState(true);
  const [animation, setAnimation] = useState<AnimationName>("idle");

  useEffect(() => {
    void authCommands.snapshot().then((snapshot) => {
      const active = snapshot.accounts.find((account) => account.id === snapshot.activeAccountId);
      if (active?.username) setPlayerName(active.username.slice(0, 32));
    }).catch(() => undefined);
  }, []);

  useEffect(() => {
    let active = true;
    let observer: ResizeObserver | null = null;
    let viewer: Viewer | null = null;
    const initialize = async () => {
      if (!canvasRef.current || !anchorRef.current) return;
      if (typeof WebGLRenderingContext === "undefined") {
        if (active) setState("fallback");
        return;
      }
      try {
        const module = await import("skinview3d");
        if (!active || !canvasRef.current || !anchorRef.current) return;
        moduleRef.current = module;
        const resize = () => {
          if (!viewer || !anchorRef.current) return;
          viewer.width = Math.max(260, Math.round(anchorRef.current.clientWidth || 320));
          viewer.height = Math.max(340, Math.round(anchorRef.current.clientHeight || 420));
        };
        viewer = new module.SkinViewer({
          canvas: canvasRef.current,
          width: 320,
          height: 420,
          pixelRatio: Math.min(window.devicePixelRatio || 1, 1.5),
          background: 0x000000,
          zoom: 0.72,
          enableControls: true,
        });
        viewer.background = null;
        viewer.controls.enableZoom = false;
        viewer.controls.enablePan = false;
        viewer.controls.enableDamping = true;
        viewer.controls.dampingFactor = 0.08;
        viewer.cameraLight.intensity = 0.72;
        viewer.globalLight.intensity = 2.15;
        await viewer.loadSkin(LOCAL_SKIN, { model: "auto-detect" });
        await viewer.loadCape(LOCAL_CAPE, { backEquipment: "cape" });
        if (!active) {
          viewer.dispose();
          return;
        }
        viewer.nameTag = playerName;
        viewer.animation = reducedMotion ? null : animationFor(module, animation);
        viewer.playerObject.skin.setOuterLayerVisible(outerLayer);
        setModel(viewer.playerObject.skin.modelType === "slim" ? "slim" : "classic");
        viewerRef.current = viewer;
        resize();
        if (typeof ResizeObserver !== "undefined") {
          observer = new ResizeObserver(resize);
          observer.observe(anchorRef.current);
        }
        setState("ready");
      } catch {
        if (active) setState("fallback");
      }
    };
    void initialize();
    return () => {
      active = false;
      observer?.disconnect();
      viewerRef.current = null;
      moduleRef.current = null;
      viewer?.dispose();
    };
  }, []);

  useEffect(() => {
    if (viewerRef.current) viewerRef.current.nameTag = playerName;
  }, [playerName]);

  useEffect(() => {
    const viewer = viewerRef.current;
    const module = moduleRef.current;
    if (viewer && module) viewer.animation = reducedMotion ? null : animationFor(module, animation);
  }, [animation, reducedMotion]);

  useEffect(() => {
    viewerRef.current?.playerObject.skin.setOuterLayerVisible(outerLayer);
  }, [outerLayer]);

  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer) return;
    if (equipment === "none") void viewer.loadCape(null);
    else void viewer.loadCape(LOCAL_CAPE, { backEquipment: equipment === "wings" ? "elytra" : "cape" });
  }, [equipment]);

  const faceStyle = useMemo(() => ({ backgroundImage: `url("${LOCAL_SKIN}")` }), []);
  const setView = (rotation: number) => {
    if (viewerRef.current) viewerRef.current.playerObject.rotation.y = rotation;
  };
  const reset = () => {
    const viewer = viewerRef.current;
    if (!viewer) return;
    viewer.playerObject.rotation.set(0, 0, 0);
    viewer.playerObject.resetJoints();
    viewer.resetCameraPose();
  };
  const onCanvasKeyDown = (event: KeyboardEvent<HTMLCanvasElement>) => {
    if (!viewerRef.current || !["ArrowLeft", "ArrowRight"].includes(event.key)) return;
    event.preventDefault();
    viewerRef.current.playerObject.rotation.y += event.key === "ArrowLeft" ? -0.15 : 0.15;
  };

  return (
    <section className="home-panel player-stage" data-preview-surface="integrated" aria-labelledby="player-stage-title">
      <header><div className="player-stage__title"><h2 id="player-stage-title">{t("player.title")}</h2><span>{playerName}</span></div><Badge tone={state === "ready" ? "success" : "warning"}>{t(state === "ready" ? "player.localReady" : state === "loading" ? "player.loading" : "player.fallback")}</Badge></header>
      <div className="player-stage__viewport">
        <div className={`player-stage__halo ${halo ? "player-stage__halo--visible" : ""}`} aria-hidden="true" />
        <div ref={anchorRef} className="player-render-anchor">
          <canvas ref={canvasRef} className="player-canvas" tabIndex={0} aria-label={t("player.canvasLabel", { name: playerName })} onKeyDown={onCanvasKeyDown} />
          {state !== "ready" ? <div className="player-render-fallback" role="status"><Box aria-hidden="true" /><span>{t(state === "loading" ? "player.loadingDescription" : "player.fallbackDescription")}</span></div> : null}
        </div>
        <div className="player-stage__platform" aria-hidden="true" />
        <div className="player-stage__identity"><span className="player-stage__face" style={faceStyle} aria-hidden="true" /><span><strong>{playerName}</strong><small>{t(model === "slim" ? "player.modelSlim" : "player.modelClassic")}</small></span><ShieldCheck aria-label={t("player.localAsset")}/></div>
      </div>
      <div className="player-controls" aria-label={t("player.controls") }>
        <div className="player-controls__views"><Button onClick={() => setView(0)} disabled={state !== "ready"}><Eye aria-hidden="true" />{t("player.front")}</Button><Button onClick={() => setView(Math.PI)} disabled={state !== "ready"}><CircleOff aria-hidden="true" />{t("player.back")}</Button><Button onClick={reset} disabled={state !== "ready"}><RotateCcw aria-hidden="true" />{t("player.reset")}</Button></div>
        <div className="player-controls__options">
          <Switch label={t("player.skinLayers")} checked={outerLayer} onChange={(event) => setOuterLayer(event.target.checked)} />
          <Switch label={t("player.halo")} checked={halo} onChange={(event) => setHalo(event.target.checked)} />
        </div>
        <div className="player-control-pills" aria-label={t("player.cosmetics") }>
          {(["cape", "wings", "none"] as Equipment[]).map((item) => <button key={item} type="button" aria-pressed={equipment === item} onClick={() => setEquipment(item)}><Sparkles aria-hidden="true" />{t(item === "cape" ? "player.cape" : item === "wings" ? "player.wings" : "player.none")}</button>)}
        </div>
        <div className="player-control-pills" aria-label={t("player.animation") }>
          {(["idle", "walk", "wave"] as AnimationName[]).map((item) => <button key={item} type="button" aria-pressed={animation === item} disabled={reducedMotion} onClick={() => setAnimation(item)}><Accessibility aria-hidden="true" />{t(item === "idle" ? "player.idle" : item === "walk" ? "player.walk" : "player.wave")}</button>)}
        </div>
        <small>{t("player.rotationHint")}</small>
      </div>
    </section>
  );
}
