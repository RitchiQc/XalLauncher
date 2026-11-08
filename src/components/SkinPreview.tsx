import { useEffect, useRef } from "react";
import { IdleAnimation, SkinViewer } from "skinview3d";

export function SkinPreview({ skinUrl, capeUrl }: { skinUrl?: string; capeUrl?: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    if (!canvas.current || !skinUrl) return;
    const viewer = new SkinViewer({ canvas: canvas.current, width: 300, height: 390, skin: skinUrl });
    viewer.animation = new IdleAnimation();
    viewer.camera.position.set(0, 0, 38);
    viewer.controls.enablePan = false;
    viewer.controls.enableZoom = false;
    if (capeUrl) void viewer.loadCape(capeUrl);
    return () => viewer.dispose();
  }, [skinUrl, capeUrl]);
  return <div className="skin-preview">{skinUrl ? <canvas ref={canvas} aria-label="Aperçu du skin et de la cape" /> : <span>Aperçu indisponible</span>}</div>;
}
