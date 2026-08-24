import { useEffect, useRef, useState } from "react";
import { Box } from "lucide-react";
import type { LauncherCosmeticAsset } from "../../lib/snineClientBridge";
import { loadSNinePlayerSkin } from "../../lib/snineClientBridge";
import { LauncherSkinRenderer, loadSkinImage, type LoadedRendererCosmetic } from "./launcherSkinRenderer";

interface LauncherSkinPreviewProps {
  accountId?: string | null;
  playerName: string;
  reducedMotion: boolean;
  cosmetics?: LauncherCosmeticAsset[];
  onModelResolved?: (model: "slim" | "classic") => void;
  cameraYaw?: number;
  cameraPitch?: number;
  cameraDistance?: number;
  cameraTargetY?: number;
}

const FALLBACK_SKIN = "./skins/snine-default.png";
const ACTIVE_SKIN_KEY = "snine.active.skin";
const ACTIVE_MODEL_KEY = "snine.active.skin.model";
const SKIN_CHANGE_EVENT = "snine-active-skin-changed";

function loadActiveSkin(): { url: string; model: "slim" | "classic" } | null {
  const url = localStorage.getItem(ACTIVE_SKIN_KEY);
  if (!url) return null;
  return {
    url,
    model: localStorage.getItem(ACTIVE_MODEL_KEY) === "slim" ? "slim" : "classic",
  };
}

export function LauncherSkinPreview({
  accountId,
  playerName,
  reducedMotion,
  cosmetics = [],
  onModelResolved,
  cameraYaw = 0,
  cameraPitch = 1.5,
  cameraDistance = 62,
  cameraTargetY = 16,
}: LauncherSkinPreviewProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rendererRef = useRef<LauncherSkinRenderer | null>(null);
  const skinGenerationRef = useRef(0);
  const cosmeticGenerationRef = useRef(0);
  const [state, setState] = useState<"loading" | "ready" | "fallback">("loading");
  const [activeSkin, setActiveSkin] = useState(loadActiveSkin);

  useEffect(() => {
    const refresh = () => setActiveSkin(loadActiveSkin());
    window.addEventListener(SKIN_CHANGE_EVENT, refresh);
    window.addEventListener("storage", refresh);
    return () => {
      window.removeEventListener(SKIN_CHANGE_EVENT, refresh);
      window.removeEventListener("storage", refresh);
    };
  }, []);
  // Create WebGL exactly once. Cosmetic live-pushes must not recreate the player
  // or restart the skin request every time an equipped item changes.
  useEffect(() => {
    if (!canvasRef.current) return;
    const renderer = new LauncherSkinRenderer(canvasRef.current, reducedMotion);
    renderer.setCameraPreset(cameraYaw, cameraPitch, cameraDistance, cameraTargetY);
    rendererRef.current = renderer;
    return () => {
      rendererRef.current = null;
      renderer.dispose();
    };
  }, []);

  // The skin is tied only to the selected Minecraft account, not to cosmetics.
  useEffect(() => {
    const generation = ++skinGenerationRef.current;
    let alive = true;
    let retryTimer: number | null = null;
    let retryDelay = 2_500;

    const scheduleRetry = () => {
      if (!alive || generation !== skinGenerationRef.current || !accountId) return;
      if (retryTimer !== null) window.clearTimeout(retryTimer);
      retryTimer = window.setTimeout(() => {
        retryTimer = null;
        void load(false);
      }, retryDelay);
      retryDelay = Math.min(30_000, Math.round(retryDelay * 1.8));
    };

    const load = async (showLoading = true) => {
      if (showLoading) setState("loading");
      const skinSnapshot = activeSkin
        ? {
            ok: true,
            playerName,
            textureDataUrl: activeSkin.url,
            model: activeSkin.model,
            source: "launcher-skin-library",
            statusMessage: "custom_skin_active",
          }
        : await loadSNinePlayerSkin(accountId, playerName);
      if (!alive || generation !== skinGenerationRef.current) return;

      const resolvedModel = skinSnapshot.model === "slim" ? "slim" : "classic";
      onModelResolved?.(resolvedModel);

      let fallback = !skinSnapshot.ok || !skinSnapshot.textureDataUrl;
      let skinImage: HTMLImageElement;
      try {
        skinImage = await loadSkinImage(skinSnapshot.textureDataUrl || FALLBACK_SKIN);
      } catch (error) {
        console.warn("[SNine Launcher] Skin image decode failed; using bundled fallback", error);
        skinImage = await loadSkinImage(FALLBACK_SKIN);
        fallback = true;
      }

      if (!alive || generation !== skinGenerationRef.current) return;
      const renderer = rendererRef.current;
      if (!renderer) return;
      renderer.setSkin(skinImage, resolvedModel);
      setState(fallback ? "fallback" : "ready");

      if (fallback) {
        scheduleRetry();
      } else {
        retryDelay = 2_500;
        if (retryTimer !== null) {
          window.clearTimeout(retryTimer);
          retryTimer = null;
        }
      }
    };

    void load().catch((error) => {
      console.warn("[SNine Launcher] Player skin setup failed", error);
      if (alive && generation === skinGenerationRef.current) {
        setState("fallback");
        scheduleRetry();
      }
    });

    return () => {
      alive = false;
      if (retryTimer !== null) window.clearTimeout(retryTimer);
    };
  }, [accountId, activeSkin?.model, activeSkin?.url, playerName, onModelResolved]);

  // Cosmetics are hot-swappable. WebSocket updates only replace the cosmetic
  // scene graph and leave the already loaded Minecraft skin untouched.
  useEffect(() => {
    const generation = ++cosmeticGenerationRef.current;
    let alive = true;

    const load = async () => {
      const loadedCosmetics = await Promise.all(cosmetics.map(async (asset): Promise<LoadedRendererCosmetic> => {
        if (asset.kind.toLowerCase() === "glint") return { asset, image: null };
        if (!asset.textureDataUrl) return { asset, image: null };
        try {
          return { asset, image: await loadSkinImage(asset.textureDataUrl) };
        } catch (error) {
          console.warn(`[SNine Launcher] Cosmetic texture failed: ${asset.id}`, error);
          return { asset, image: null };
        }
      }));

      if (!alive || generation !== cosmeticGenerationRef.current) return;
      rendererRef.current?.setCosmetics(loadedCosmetics);
    };

    void load();
    return () => { alive = false; };
  }, [cosmetics]);

  useEffect(() => {
    rendererRef.current?.setCameraPreset(cameraYaw, cameraPitch, cameraDistance, cameraTargetY);
  }, [cameraYaw, cameraPitch, cameraDistance, cameraTargetY]);

  useEffect(() => {
    rendererRef.current?.setReducedMotion(reducedMotion);
  }, [reducedMotion]);

  return (
    <div className="launcher-skin" data-state={state}>
      <canvas ref={canvasRef} className="launcher-skin__canvas" aria-label={`${playerName} skin preview`} />
      {state === "loading" ? (
        <div className="launcher-skin__loading">
          <Box aria-hidden="true" />
          <span>PLAYER WIRD GELADEN</span>
        </div>
      ) : null}
      <div className="launcher-skin__hint" aria-hidden="true">DRAG · ROTATE&nbsp;&nbsp; / &nbsp;&nbsp;DOUBLE CLICK · FRONT&nbsp;&nbsp; / &nbsp;&nbsp;SCROLL · ZOOM</div>
    </div>
  );
}
