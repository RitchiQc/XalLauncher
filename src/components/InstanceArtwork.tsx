import { Box, Clock3, Gamepad2 } from "lucide-react";
import type { Instance } from "@/types";
import { formatDate, formatDuration, loaderLabels } from "@/lib/format";

export function InstanceArtwork({ instance, compact = false }: { instance: Instance; compact?: boolean }) {
  return <div className={`instance-art ${compact ? "compact" : ""}`} style={{ "--instance-color": instance.color } as React.CSSProperties}>
    <div className="art-grid" /><div className="art-orb" /><div className="art-cube"><Box strokeWidth={1.1} /></div>
    <div className="art-content">
      <span className="instance-loader">{loaderLabels[instance.loader]} · {instance.versionId}</span>
      <h2>{instance.name}</h2>
      {!compact ? <div className="art-meta"><span><Clock3 size={14} />{formatDuration(instance.totalPlaySeconds)}</span><span><Gamepad2 size={14} />{formatDate(instance.lastPlayedAt)}</span></div> : null}
    </div>
  </div>;
}
