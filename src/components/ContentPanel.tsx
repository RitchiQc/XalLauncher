import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Download, FileBox, Image, Power, RefreshCw, Search, Trash2, Upload } from "lucide-react";
import { api } from "@/lib/api";
import { confirmAction } from "@/lib/dialog";
import { formatBytes, formatDate } from "@/lib/format";
import { confirmCodeExecution } from "@/lib/security";
import { useLauncher } from "@/state/LauncherContext";
import type { ContentEntry, Instance } from "@/types";
import { ProjectBrowser } from "./ProjectBrowser";
import { Button, EmptyState } from "./UI";

const labels: Record<string, string> = { mod: "mod", resourcepack: "resource pack", shader: "shader", world: "monde", screenshot: "capture" };

export function ContentPanel({ instance, kind }: { instance: Instance; kind: "mod" | "resourcepack" | "shader" | "world" | "screenshot" }) {
  const instanceId = instance.id;
  const { perform } = useLauncher();
  const [view, setView] = useState<"installed" | "browse">("installed");
  const downloadable = kind === "mod" || kind === "resourcepack" || kind === "shader";
  const [items, setItems] = useState<ContentEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [paths, setPaths] = useState<Record<string, string>>({});
  const refresh = useCallback(async () => {
    setLoading(true);
    try { setItems(await api.content(instanceId, kind)); } finally { setLoading(false); }
  }, [instanceId, kind]);
  useEffect(() => { void refresh(); }, [refresh]);
  useEffect(() => {
    if (kind !== "screenshot") return;
    items.forEach((item) => { void api.contentPath(instanceId, kind, item.fileName).then((path) => setPaths((current) => ({ ...current, [item.fileName]: convertFileSrc(path) }))); });
  }, [instanceId, items, kind]);

  async function importFile() {
    if (kind === "mod" && !(await confirmCodeExecution("Ce mod"))) return;
    const extensions = kind === "mod" ? ["jar"] : ["zip"];
    const path = await open({ multiple: false, filters: [{ name: labels[kind], extensions }] });
    if (!path) return;
    const result = await perform(() => api.importContent(instanceId, kind, path), "Contenu importé");
    if (result) await refresh();
  }
  async function toggle(item: ContentEntry) {
    const result = await perform(() => api.toggleContent(instanceId, kind, item.fileName));
    if (result) await refresh();
  }
  async function remove(item: ContentEntry) {
    if (!(await confirmAction(`Supprimer « ${item.projectTitle ?? item.fileName} » ?`, "Supprimer un contenu"))) return;
    const result = await perform(() => api.deleteContent(instanceId, kind, item.fileName), "Contenu supprimé");
    if (result !== undefined) await refresh();
  }

  return <section className="content-section">
    {downloadable ? <div className="segmented content-view-switch"><button className={view === "installed" ? "active" : ""} onClick={() => setView("installed")}><FileBox size={15} />Installés</button><button className={view === "browse" ? "active" : ""} onClick={() => setView("browse")}><Search size={15} />Ajouter</button></div> : null}
    {view === "browse" && downloadable ? <ProjectBrowser kind={kind} instance={instance} installedContent={items} onContentInstalled={refresh} /> : <>
    <div className="section-toolbar"><div><h3>{kind === "mod" ? "Mods installés" : kind === "world" ? "Vos mondes" : kind === "screenshot" ? "Captures d’écran" : kind === "shader" ? "Shader packs" : "Resource packs"}</h3><span className="muted">{items.length} élément{items.length > 1 ? "s" : ""}</span></div><div><Button variant="ghost" className="icon-button" onClick={() => void refresh()} title="Actualiser"><RefreshCw size={17} /></Button>{!['world', 'screenshot'].includes(kind) ? <Button variant="secondary" onClick={() => void importFile()}><Upload size={16} />Importer</Button> : null}</div></div>
    {loading ? <div className="panel-loading"><RefreshCw className="spin" />Chargement…</div> : items.length === 0 ? <EmptyState icon={kind === "screenshot" ? <Image /> : <FileBox />} title="Rien ici pour le moment" description={kind === "world" ? "Vos mondes Minecraft apparaîtront après votre première partie." : kind === "screenshot" ? "Vos captures réalisées en jeu apparaîtront ici." : "Installez du contenu depuis l’explorateur ou importez un fichier local."} /> : kind === "screenshot" ? <div className="screenshot-grid">{items.map((item) => <button className="screenshot-card" key={item.fileName} title={item.fileName}>{paths[item.fileName] ? <img src={paths[item.fileName]} alt={item.fileName} /> : null}<span>{formatDate(item.modifiedAt)}</span><Button variant="danger" className="icon-button" onClick={() => void remove(item)}><Trash2 size={15} /></Button></button>)}</div> : <div className="content-list">{items.map((item) => <article className={item.enabled ? "content-row" : "content-row disabled"} key={item.fileName}>
      <div className="content-icon">{item.iconUrl ? <img src={item.iconUrl} alt="" /> : kind === "world" ? <Image /> : <FileBox />}</div>
      <div className="content-info"><strong>{item.projectTitle ?? item.fileName}</strong><span>{item.fileName} · {formatBytes(item.size)}</span></div>
      <span className="content-date">{formatDate(item.modifiedAt)}</span>
      {item.projectId ? <span className="provider-badge"><Download size={12} />Modrinth</span> : null}
      {!['world', 'screenshot'].includes(kind) ? <Button variant={item.enabled ? "secondary" : "ghost"} className="icon-button" title={item.enabled ? "Désactiver" : "Activer"} onClick={() => void toggle(item)}><Power size={17} /></Button> : null}
      <Button variant="ghost" className="icon-button danger-icon" title="Supprimer" onClick={() => void remove(item)}><Trash2 size={17} /></Button>
    </article>)}</div>}</>}
  </section>;
}
