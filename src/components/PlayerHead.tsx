import { useEffect, useState } from "react";
import { avatarFallback, avatarUrl, handleAvatarError } from "@/lib/format";

type Props = { skinUrl?: string; uuid: string; name: string; size?: number; className?: string };

export function PlayerHead({ skinUrl, uuid, name, size = 43, className = "" }: Props) {
  const [legacy, setLegacy] = useState(false);
  const [source, setSource] = useState<"skin" | "remote">(skinUrl ? "skin" : "remote");

  useEffect(() => {
    setSource(skinUrl ? "skin" : "remote");
    setLegacy(false);
  }, [skinUrl, uuid]);

  useEffect(() => {
    if (source !== "skin" || !skinUrl) return;
    let active = true;
    const image = new Image();
    image.onload = () => { if (active) setLegacy(image.naturalHeight > 0 && image.naturalHeight <= 32); };
    image.onerror = () => { if (active) setSource("remote"); };
    image.src = skinUrl;
    return () => { active = false; };
  }, [skinUrl, source]);

  if (source === "remote") {
    return <span className={`player-head ${className}`} style={{ width: size, height: size }}>
      <img src={avatarUrl(uuid, size)} data-fallback={avatarFallback(uuid, size)} onError={handleAvatarError} alt={`Tête de ${name}`} />
    </span>;
  }

  const scale = size / 8;
  const texture = skinUrl ?? "";
  const textureHeight = (legacy ? 32 : 64) * scale;
  const backgroundSize = `${64 * scale}px ${textureHeight}px`;
  return <span className={`player-head ${className}`} role="img" aria-label={`Tête de ${name}`} style={{
    width: size, height: size,
    backgroundImage: `url("${texture}")`,
    backgroundSize,
    backgroundPosition: `${-8 * scale}px ${-8 * scale}`,
  }}>
    <span className="player-head-overlay" aria-hidden="true" style={{
      backgroundImage: `url("${texture}")`,
      backgroundSize,
      backgroundPosition: `${-40 * scale}px ${-8 * scale}`,
    }} />
  </span>;
}
