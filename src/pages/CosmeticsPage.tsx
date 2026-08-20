import { useCallback, useEffect, useState } from "react";
import { ChevronRight, CircleUserRound, Layers3, RefreshCw, Sparkles, Wifi, WifiOff } from "lucide-react";
import { authCommands } from "../lib/authCommands";
import { profileCommands } from "../lib/profileCommands";
import type { Phase3Account } from "../lib/generated/ipc-contracts";
import { useWorkspaceStore } from "../app/workspaceStore";
import {
  loadSNineLauncherCosmetics,
  type LauncherCosmeticSnapshot,
} from "../lib/snineClientBridge";

const EMPTY: LauncherCosmeticSnapshot = {
  ok: false,
  playerName: "SNine",
  online: false,
  equipped: [],
  source: "",
  statusMessage: "not_connected",
  liveSync: null,
};

function isSnapshotSource(source: string): boolean {
  return source.toLowerCase().includes("offline") || source.toLowerCase().includes("snapshot");
}

export function CosmeticsPage() {
  const [account, setAccount] = useState<Phase3Account | null>(null);
  const [snapshot, setSnapshot] = useState<LauncherCosmeticSnapshot>(EMPTY);
  const [loading, setLoading] = useState(true);
  const selectedProfileId = useWorkspaceStore((state) => state.selectedProfileId);

  const sync = useCallback(async () => {
    setLoading(true);
    try {
      const [auth, profiles] = await Promise.all([
        authCommands.snapshot().catch(() => null),
        profileCommands.list().catch(() => []),
      ]);
      const selectedProfile = profiles.find((profile) => profile.id === selectedProfileId) ?? null;
      const accountId = selectedProfile?.accountId ?? auth?.activeAccountId ?? null;
      const player = auth?.accounts.find((item) => item.id === accountId) ?? null;
      setAccount(player);
      if (!player) {
        setSnapshot(EMPTY);
        return;
      }
      setSnapshot(await loadSNineLauncherCosmetics(player.id, player.username, selectedProfile?.id ?? selectedProfileId));
    } finally {
      setLoading(false);
    }
  }, [selectedProfileId]);

  useEffect(() => { void sync(); }, [sync]);

  const snapshotFallback = isSnapshotSource(snapshot.source);
  const NetworkIcon = snapshot.online ? Wifi : WifiOff;
  const statusLabel = loading ? "SYNCING" : snapshot.online ? "LIVE" : snapshotFallback ? "SNAPSHOT" : "OFFLINE";

  return (
    <div className="page snine-prism-cosmetics-page">
      <header className="snine-prism-pagehead">
        <div>
          <span><Sparkles aria-hidden="true" /> SNINE // LOADOUT</span>
          <h1>COSMETICS</h1>
          <p>Dein ausgerüsteter SNine-Look. Live vom Backend, mit lokalem Snapshot-Fallback wenn die Verbindung fehlt.</p>
        </div>
        <button type="button" onClick={() => void sync()} disabled={loading || !account} title={snapshot.statusMessage}>
          <RefreshCw className={loading ? "ui-spin" : ""} aria-hidden="true" /> SYNC NOW
        </button>
      </header>

      <section className="snine-prism-cosmetics-summary">
        <div><CircleUserRound aria-hidden="true" /><span><small>PLAYER</small><strong>{account?.username ?? "NO ACCOUNT"}</strong></span></div>
        <div className={snapshot.online ? "is-live" : ""}><NetworkIcon aria-hidden="true" /><span><small>DATA SOURCE</small><strong>{statusLabel}</strong></span></div>
        <div><Layers3 aria-hidden="true" /><span><small>EQUIPPED</small><strong>{String(snapshot.equipped.length).padStart(2, "0")}</strong></span></div>
      </section>

      <section className="snine-prism-cosmetics-grid" aria-label="Equipped cosmetics">
        {loading ? (
          <div className="snine-prism-cosmetics-empty"><RefreshCw className="ui-spin" aria-hidden="true" /><strong>LOADOUT SYNC</strong><span>IDs, Modelle und Texturen werden geladen.</span></div>
        ) : snapshot.equipped.length ? snapshot.equipped.map((item, index) => (
          <article className="snine-prism-cosmetic-card" key={`${item.kind}:${item.id}`}>
            <div className="snine-prism-cosmetic-card__index">{String(index + 1).padStart(2, "0")}</div>
            <div className="snine-prism-cosmetic-card__preview">
              {item.textureDataUrl ? <img src={item.textureDataUrl} alt="" /> : <Layers3 aria-hidden="true" />}
            </div>
            <div className="snine-prism-cosmetic-card__copy">
              <small>{item.kind.toUpperCase()}</small>
              <h2>{item.name || item.id}</h2>
              <code>{item.id}</code>
            </div>
            <span className="snine-prism-cosmetic-card__render"><i /> PLAYER RENDER</span>
            <ChevronRight aria-hidden="true" />
          </article>
        )) : (
          <div className="snine-prism-cosmetics-empty"><Sparkles aria-hidden="true" /><strong>NO LOADOUT</strong><span>{snapshot.statusMessage.replaceAll("_", " ")}</span></div>
        )}
      </section>
    </div>
  );
}
