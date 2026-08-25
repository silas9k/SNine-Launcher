import { useEffect, useRef, useState } from "react";
import { UserRound } from "lucide-react";
import { loadMinecraftSkin, minecraftHeadFromSnapshot } from "../../lib/minecraftSkinCache";

interface MinecraftAvatarProps {
  accountId?: string | null;
  username?: string | null;
  className?: string;
  size?: number;
  decorative?: boolean;
}

export function MinecraftAvatar({
  accountId,
  username,
  className = "minecraft-avatar",
  size = 64,
  decorative = true,
}: MinecraftAvatarProps) {
  const requestRef = useRef(0);
  const [source, setSource] = useState<string | null>(null);

  useEffect(() => {
    const request = ++requestRef.current;
    let alive = true;
    setSource(null);
    if (!accountId || !username) return () => { alive = false; };

    void loadMinecraftSkin(accountId, username)
      .then((snapshot) => minecraftHeadFromSnapshot(accountId, snapshot, size))
      .then((avatar) => {
        if (alive && request === requestRef.current) setSource(avatar);
      })
      .catch(() => {
        if (alive && request === requestRef.current) setSource(null);
      });

    return () => { alive = false; };
  }, [accountId, size, username]);

  return (
    <span className={className} data-avatar-state={source ? "ready" : "fallback"}>
      {source
        ? <img src={source} alt={decorative ? "" : `${username ?? "Minecraft"} avatar`} draggable={false} />
        : <UserRound aria-hidden="true" />}
    </span>
  );
}
