import { useEffect, useMemo, useRef, useState } from "react";
import { Search, Terminal } from "lucide-react";
import { api } from "@/lib/api";
import { useLauncher } from "@/state/LauncherContext";
import { Button } from "./UI";

export function LogPanel({ instanceId }: { instanceId: string }) {
  const { logs } = useLauncher();
  const [stored, setStored] = useState("");
  const [query, setQuery] = useState("");
  const [errorsOnly, setErrorsOnly] = useState(false);
  const bottom = useRef<HTMLDivElement>(null);
  useEffect(() => { void api.log(instanceId).then(setStored); }, [instanceId]);
  const lines = useMemo(() => {
    const live = logs.filter((log) => log.instanceId === instanceId).map((log) => `[${log.stream}] ${log.line}`);
    return [...stored.split("\n").filter(Boolean), ...live].filter((line) => (!query || line.toLowerCase().includes(query.toLowerCase())) && (!errorsOnly || /error|exception|caused by|\[stderr\]/i.test(line)));
  }, [stored, logs, instanceId, query, errorsOnly]);
  useEffect(() => { bottom.current?.scrollIntoView({ block: "end" }); }, [lines.length]);

  return <section className="content-section"><div className="section-toolbar"><div><h3>Console de l’instance</h3><span className="muted">Sortie Minecraft en direct et dernier journal local.</span></div><Button variant={errorsOnly ? "primary" : "secondary"} onClick={() => setErrorsOnly(!errorsOnly)}>Erreurs uniquement</Button></div>
    <label className="search-field log-search"><Search size={17} /><input placeholder="Rechercher dans les journaux…" value={query} onChange={(event) => setQuery(event.target.value)} /></label>
    <div className="log-console">{lines.length ? lines.map((line, index) => <div className={/error|exception|\[stderr\]/i.test(line) ? "log-line error" : "log-line"} key={`${index}-${line.slice(0, 30)}`}><span>{String(index + 1).padStart(4, "0")}</span>{line}</div>) : <div className="log-empty"><Terminal /><p>Aucune ligne à afficher. Lancez le jeu pour voir sa console.</p></div>}<div ref={bottom} /></div>
  </section>;
}
