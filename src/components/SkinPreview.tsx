import { useEffect, useRef, useState } from "react";
import { IdleAnimation, SkinViewer } from "skinview3d";

export type SkinModel = "default" | "slim" | "auto-detect";

export function SkinPreview({ skinUrl, capeUrl, model }: { skinUrl?: string; capeUrl?: string; model?: SkinModel }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => { setFailed(false); }, [skinUrl]);
  useEffect(() => {
    if (failed || !canvas.current || !skinUrl) return;
    const selected = model ?? "auto-detect";
    const viewer = new SkinViewer({ canvas: canvas.current, width: 300, height: 390, model: selected });
    viewer.animation = new IdleAnimation();
    viewer.camera.position.set(0, 0, 38);
    viewer.playerWrapper.rotation.y = Math.PI;
    viewer.controls.enablePan = false;
    viewer.controls.enableZoom = false;
    viewer.loadSkin(skinUrl, { model: selected }).catch(() => setFailed(true));
    if (capeUrl) void viewer.loadCape(capeUrl).catch(() => undefined);
    return () => viewer.dispose();
  }, [skinUrl, capeUrl, model, failed]);
  return <div className="skin-preview">{skinUrl && !failed ? <canvas ref={canvas} aria-label="Aperçu du skin et de la cape" /> : <span>{failed ? "Skin officiel indisponible : actualisez le profil" : "Aperçu indisponible"}</span>}</div>;
}
