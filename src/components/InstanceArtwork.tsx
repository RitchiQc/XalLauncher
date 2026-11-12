import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Clock3, Gamepad2 } from "lucide-react";
import defaultCover from "@/assets/instance-cover.jpg";
import logoUrl from "../../design/xallauncher-portal.svg";
import { api } from "@/lib/api";
import type { Instance } from "@/types";
import { formatDate, formatDuration, loaderLabels } from "@/lib/format";

export function InstanceArtwork({ instance, compact = false }: { instance: Instance; compact?: boolean }) {
  const [coverSrc, setCoverSrc] = useState<string>();
  const [iconSrc, setIconSrc] = useState<string>();
  useEffect(() => {
    setCoverSrc(undefined);
    setIconSrc(undefined);
    if (instance.iconUrl) { setIconSrc(instance.iconUrl); return; }
    if (!instance.iconFile) return;
    let active = true;
    void api.instanceIconPath(instance.id).then((path) => {
      if (active && path) setCoverSrc(convertFileSrc(path));
    }).catch(() => undefined);
    return () => { active = false; };
  }, [instance.id, instance.iconUrl, instance.iconFile]);
  return <div className={`instance-art ${compact ? "compact" : ""}`} style={{ "--instance-color": instance.color } as React.CSSProperties}>
    <div className="art-grid" /><div className="art-orb" />
    {coverSrc ? <img className="art-cover" src={coverSrc} alt="" onError={() => setCoverSrc(undefined)} /> : <img className="art-cover art-cover-default" src={defaultCover} alt="" />}
    <div className="art-cover-shade" />
    {coverSrc ? null : <div className="art-icon-wrap">{iconSrc ? <img className="art-icon" src={iconSrc} alt="" onError={() => setIconSrc(undefined)} /> : <img className="art-icon art-icon-default" src={logoUrl} alt="" />}</div>}
    <div className="art-content">
      <span className="instance-loader">{loaderLabels[instance.loader]} · {instance.versionId}</span>
      <h2>{instance.name}</h2>
      {!compact ? <div className="art-meta"><span><Clock3 size={14} />{formatDuration(instance.totalPlaySeconds)}</span><span><Gamepad2 size={14} />{formatDate(instance.lastPlayedAt)}</span></div> : null}
    </div>
  </div>;
}
