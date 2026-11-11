import { useEffect, useState } from "react";

type Texture = { url: string; width: number; height: number };

export function CapeThumbnail({ url, alias }: { url: string; alias: string }) {
  const [texture, setTexture] = useState<Texture>();

  useEffect(() => {
    setTexture(undefined);
    let active = true;
    const image = new Image();
    image.onload = () => {
      if (active && image.naturalWidth >= 22 && image.naturalHeight >= 17) setTexture({ url, width: image.naturalWidth, height: image.naturalHeight });
    };
    image.src = url;
    return () => { active = false; };
  }, [url]);

  if (!texture) return <span className="cape-thumb cape-thumb-loading" aria-hidden="true" />;

  // Face extérieure de la cape : colonnes 1-10 et lignes 1-16, en 64x32 comme en legacy 22x17.
  const scale = (texture.width / (texture.width >= 64 ? 64 : 22)) * 4;
  return <span className="cape-thumb" role="img" aria-label={`Cape ${alias}`} style={{
    backgroundImage: `url("${texture.url}")`,
    backgroundSize: `${texture.width * scale}px ${texture.height * scale}px`,
    backgroundPosition: `${-1 * scale}px ${-1 * scale}px`,
  }} />;
}
