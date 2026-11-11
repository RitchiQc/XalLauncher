import { useEffect, useState } from "react";
import { avatarFallback, avatarUrl } from "@/lib/format";

type Props = { skinUrl?: string; uuid: string; name: string; size?: number; className?: string };
type Texture = { url: string; width: number; height: number };

export function PlayerHead({ skinUrl, uuid, name, size = 43, className = "" }: Props) {
  const [texture, setTexture] = useState<Texture>();
  const [remote, setRemote] = useState(false);
  const [remoteStep, setRemoteStep] = useState(0);

  useEffect(() => {
    setTexture(undefined);
    setRemote(false);
    setRemoteStep(0);
    if (!skinUrl) { setRemote(true); return; }
    let active = true;
    const image = new Image();
    image.onload = () => {
      if (!active) return;
      // Le visage occupe (8,8)-(16,16) quelle que soit la taille : 64x32, 64x64 ou HD.
      if (image.naturalWidth >= 64 && image.naturalHeight >= 32) setTexture({ url: skinUrl, width: image.naturalWidth, height: image.naturalHeight });
      else setRemote(true);
    };
    image.onerror = () => { if (active) setRemote(true); };
    image.src = skinUrl;
    return () => { active = false; };
  }, [skinUrl, uuid]);

  if (texture) {
    const scale = size / 8;
    const backgroundSize = `${texture.width * scale}px ${texture.height * scale}px`;
    const offset = (x: number, y: number) => `${-x * scale}px ${-y * scale}px`;
    return <span className={`player-head ${className}`} role="img" aria-label={`Tête de ${name}`} style={{
      width: size, height: size,
      backgroundImage: `url("${texture.url}")`,
      backgroundSize,
      backgroundPosition: offset(8, 8),
    }}>
      <span className="player-head-overlay" aria-hidden="true" style={{
        backgroundImage: `url("${texture.url}")`,
        backgroundSize,
        backgroundPosition: offset(40, 8),
      }} />
    </span>;
  }

  if (!remote) return <span className={`player-head player-head-loading ${className}`} style={{ width: size, height: size }} aria-hidden="true" />;

  const sources = [avatarUrl(uuid, size), avatarFallback(uuid, size)];
  if (remoteStep >= sources.length) {
    return <span className={`player-head player-head-empty ${className}`} style={{ width: size, height: size }} role="img" aria-label={`Tête de ${name}`}>{name.slice(0, 1).toUpperCase()}</span>;
  }
  return <span className={`player-head ${className}`} style={{ width: size, height: size }}>
    <img src={sources[remoteStep]} alt={`Tête de ${name}`} onError={() => setRemoteStep((step) => step + 1)} />
  </span>;
}
