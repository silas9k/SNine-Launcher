import { useReleaseText } from "../../i18n/releaseUiText";
import { useEffect, useRef, useState, type CSSProperties } from "react";
import { Box } from "lucide-react";
import { launcherBadgeIconUrl, type LauncherCosmeticAsset } from "../../lib/snineClientBridge";
import { loadMinecraftSkin } from "../../lib/minecraftSkinCache";
import { LauncherSkinRenderer, loadSkinImage, type LoadedRendererCosmetic } from "./launcherSkinRenderer";

interface LauncherSkinPreviewProps {
  accountId?: string | null;
  playerName: string;
  reducedMotion: boolean;
  cosmetics?: LauncherCosmeticAsset[];
  badgeIconUrl?: string | null;
  onModelResolved?: (model: "slim" | "classic") => void;
  cameraYaw?: number;
  cameraPitch?: number;
  cameraDistance?: number;
  cameraTargetY?: number;
}

const FALLBACK_SKIN = "./skins/snine-default.png";

export function LauncherSkinPreview({
  accountId,
  playerName,
  reducedMotion,
  cosmetics = [],
  badgeIconUrl,
  onModelResolved,
  cameraYaw = 0,
  cameraPitch = 1.5,
  cameraDistance = 62,
  cameraTargetY = 16,
}: LauncherSkinPreviewProps) {
  const rt = useReleaseText();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const nametagPlaneRef = useRef<HTMLDivElement>(null);
  const rendererRef = useRef<LauncherSkinRenderer | null>(null);
  const skinGenerationRef = useRef(0);
  const cosmeticGenerationRef = useRef(0);
  const [state, setState] = useState<"loading" | "ready" | "fallback">("loading");
  const [resolvedName, setResolvedName] = useState(playerName);
  const [nametagAnchor, setNametagAnchor] = useState<{ x: number; y: number; visible: boolean } | null>(null);
  // Create WebGL exactly once. Cosmetic live-pushes must not recreate the player
  // or restart the skin request every time an equipped item changes.
  useEffect(() => {
    if (!canvasRef.current) return;
    let renderer: LauncherSkinRenderer;
    try {
      renderer = new LauncherSkinRenderer(canvasRef.current, reducedMotion);
    } catch (error) {
      console.warn("[SNine Launcher] WebGL player preview unavailable", error);
      setState("fallback");
      return;
    }
    renderer.setNametagAnchorListener?.(setNametagAnchor);
    // Keep the nametag fixed over the projected head anchor and only change its
    // horizontal facing as the camera orbits the player. Using an orthographic
    // cosine squash avoids the odd CSS-perspective warp/orbiting motion from a
    // rotateY() plane while preserving the desired Minecraft-like states:
    // 0° = normal, 90° = edge-on, 180° = mirrored/backwards.
    renderer.setCameraYawListener?.((yaw) => {
      const plane = nametagPlaneRef.current;
      if (!plane) return;
      const facing = Math.cos((yaw * Math.PI) / 180);
      const sign = facing < 0 ? -1 : 1;
      const scaleX = sign * Math.max(Math.abs(facing), 0.025);
      plane.style.transform = `scaleX(${scaleX})`;
    });
    renderer.setCameraPreset(cameraYaw, cameraPitch, cameraDistance, cameraTargetY);
    rendererRef.current = renderer;
    const observer = typeof IntersectionObserver === "undefined" ? null : new IntersectionObserver((entries) => {
      renderer.setViewportVisible(entries.some((entry) => entry.isIntersecting));
    });
    observer?.observe(canvasRef.current);
    return () => {
      observer?.disconnect();
      renderer.setNametagAnchorListener?.(null);
      renderer.setCameraYawListener?.(null);
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
    setResolvedName(playerName);

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
      const skinSnapshot = await loadMinecraftSkin(accountId, playerName, !showLoading);
      if (!alive || generation !== skinGenerationRef.current) return;

      const resolvedModel = skinSnapshot.model === "slim" ? "slim" : "classic";
      setResolvedName(skinSnapshot.playerName || playerName);
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
  }, [accountId, playerName, onModelResolved]);

  // Cosmetics are hot-swappable. Live updates only replace the cosmetic
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

  const resolvedBadgeIconUrl = badgeIconUrl || launcherBadgeIconUrl();
  const nametagStyle = {
    "--launcher-nametag-x": `${nametagAnchor?.x ?? 0}px`,
    "--launcher-nametag-y": `${nametagAnchor?.y ?? 0}px`,
    opacity: nametagAnchor?.visible ? 1 : 0,
    // The outer element only owns the projected head anchor. The visual nametag
    // is the inner 3D plane so its background, icon and text rotate together.
    padding: 0,
    gap: 0,
    background: "transparent",
  } as CSSProperties;

  const nametagPlaneStyle = {
    display: "inline-flex",
    alignItems: "center",
    gap: "4px",
    padding: "2px 4px",
    background: "rgba(0, 0, 0, .25)",
    transformOrigin: "50% 50%",
    transformStyle: "preserve-3d",
    backfaceVisibility: "visible",
    willChange: "transform",
  } as CSSProperties;

  return (
    <div className="launcher-skin" data-state={state}>
      <div
        className="launcher-skin__nametag"
        aria-label={`Minecraft player ${resolvedName}`}
        data-badge-icon={resolvedBadgeIconUrl}
        style={nametagStyle}
      >
        <div ref={nametagPlaneRef} style={nametagPlaneStyle}>
          <span className="launcher-skin__nametag-icon" aria-hidden="true">
            <img src={resolvedBadgeIconUrl} alt="" draggable={false} />
          </span>
          <span className="launcher-skin__nametag-text">{resolvedName}</span>
        </div>
      </div>
      <canvas ref={canvasRef} className="launcher-skin__canvas" aria-label={`${resolvedName} skin preview`} />
      {state === "loading" ? (
        <div className="launcher-skin__loading">
          <Box aria-hidden="true" />
          <span>{rt("PLAYER WIRD GELADEN")}</span>
        </div>
      ) : null}
      <div className="launcher-skin__hint" aria-hidden="true">{rt("DRAG · ROTATE&nbsp;&nbsp; / &nbsp;&nbsp;DOUBLE CLICK · FRONT&nbsp;&nbsp; / &nbsp;&nbsp;SCROLL · ZOOM")}</div>
    </div>
  );
}
