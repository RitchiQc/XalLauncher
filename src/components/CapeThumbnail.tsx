import { useEffect, useState } from "react";

export function CapeThumbnail({ url, alias }: { url: string; alias: string }) {
  const [legacy, setLegacy] = useState(false);

  useEffect(() => {
    setLegacy(false);
    let active = true;
    const image = new Image();
    image.onload = () => { if (active) setLegacy(image.naturalWidth > 0 && image.naturalWidth < 64); };
    image.src = url;
    return () => { active = false; };
  }, [url]);

  const scale = 4;
  const width = (legacy ? 22 : 64) * scale;
  const height = (legacy ? 17 : 32) * scale;
  const offsetX = -12 * scale;
  return <span className="cape-thumb" role="img" aria-label={`Cape ${alias}`} style={{
    backgroundImage: `url("${url}")`,
    backgroundSize: `${width}px ${height}px`,
    backgroundPosition: `${offsetX}px ${-1 * scale}px`,
  }} />;
}
