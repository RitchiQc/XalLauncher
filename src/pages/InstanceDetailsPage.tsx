import { useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { ArrowLeft, Box, CircleStop, FolderOpen, Globe2, Image, Layers, Package, Pencil, Play, Save, Settings2, Sparkles, Terminal } from "lucide-react";
import { BackupPanel } from "@/components/BackupPanel";
import { ContentPanel } from "@/components/ContentPanel";
import { InstanceArtwork } from "@/components/InstanceArtwork";
import { InstanceForm } from "@/components/InstanceForm";
import { LogPanel } from "@/components/LogPanel";
import { Button, ProgressBar } from "@/components/UI";
import { api } from "@/lib/api";
import { formatDuration } from "@/lib/format";
import { useLauncher } from "@/state/LauncherContext";
import type { Instance } from "@/types";

const tabs = [
  { id: "overview", label: "Vue d’ensemble", icon: Box },
  { id: "mod", label: "Mods", icon: Package },
  { id: "resourcepack", label: "Resource packs", icon: Layers },
  { id: "shader", label: "Shaders", icon: Sparkles },
  { id: "world", label: "Mondes", icon: Globe2 },
  { id: "screenshot", label: "Captures", icon: Image },
  { id: "backups", label: "Sauvegardes", icon: Save },
  { id: "logs", label: "Journaux", icon: Terminal },
];

export function InstanceDetailsPage() {
  const { id = "" } = useParams();
  const navigate = useNavigate();
  const { instances, running, startGame, stopGame, progress, refreshInstances, setActiveInstanceId, perform } = useLauncher();
  const [tab, setTab] = useState("overview");
  const [editing, setEditing] = useState(false);
  const [launching, setLaunching] = useState(false);
  const instance = instances.find((item) => item.id === id);
  if (!instance) return <div className="page"><h1>Instance introuvable</h1><Button onClick={() => navigate("/instances")}>Retour à la bibliothèque</Button></div>;
  const isRunning = running.has(instance.id);

  async function launch() {
    if (!instance) return;
    setLaunching(true); await startGame(instance.id); setLaunching(false);
  }
  function saved(updated: Instance) {
    void refreshInstances(); setEditing(false); setActiveInstanceId(updated.id);
  }

  return <div className="page instance-details">
    <button className="back-link" onClick={() => navigate("/instances")}><ArrowLeft size={16} />Bibliothèque</button>
    <section className="instance-detail-hero"><InstanceArtwork instance={instance} compact /><div className="detail-actions">
      <Button variant="secondary" onClick={() => setEditing(true)}><Pencil size={17} />Modifier</Button>
      <Button variant="secondary" className="icon-button" title="Ouvrir le dossier" onClick={() => void perform(() => api.openFolder(instance.id))}><FolderOpen size={19} /></Button>
      {isRunning ? <Button variant="danger" onClick={() => void stopGame(instance.id)}><CircleStop size={19} />Arrêter</Button> : <Button onClick={() => void launch()} loading={launching}><Play size={19} fill="currentColor" />Jouer</Button>}
    </div></section>
    {progress?.operation === "launch" ? <ProgressBar label={progress.label} value={progress.total ? progress.current / progress.total * 100 : 0} /> : null}
    <nav className="detail-tabs" aria-label="Contenu de l’instance">{tabs.map((item) => <button key={item.id} className={tab === item.id ? "active" : ""} onClick={() => setTab(item.id)}><item.icon size={16} />{item.label}</button>)}</nav>
    {tab === "overview" ? <div className="overview-grid"><section className="panel"><span className="eyebrow">Configuration</span><h2>Prête pour l’aventure</h2><dl className="instance-specs"><div><dt>Minecraft</dt><dd>{instance.versionId}</dd></div><div><dt>Modloader</dt><dd>{instance.loader}{instance.loaderVersion ? ` · ${instance.loaderVersion}` : ""}</dd></div><div><dt>Mémoire</dt><dd>{instance.memoryMin} Mo — {instance.memoryMax} Mo</dd></div><div><dt>Résolution</dt><dd>{instance.fullscreen ? "Plein écran" : `${instance.resolutionWidth} × ${instance.resolutionHeight}`}</dd></div><div><dt>Temps de jeu</dt><dd>{formatDuration(instance.totalPlaySeconds)}</dd></div><div><dt>Serveur favori</dt><dd>{instance.serverAddress ?? "Aucun"}</dd></div></dl></section><aside className="panel"><Settings2 /><h3>Environnement isolé</h3><p>Les mods, mondes, captures et réglages de cette instance sont stockés séparément des autres installations.</p><Button variant="secondary" onClick={() => setTab("backups")}>Protéger mes mondes</Button></aside></div> : null}
    {["mod", "resourcepack", "shader", "world", "screenshot"].includes(tab) ? <ContentPanel instance={instance} kind={tab as "mod" | "resourcepack" | "shader" | "world" | "screenshot"} /> : null}
    {tab === "backups" ? <BackupPanel instanceId={instance.id} /> : null}
    {tab === "logs" ? <LogPanel instanceId={instance.id} /> : null}
    {editing ? <InstanceForm existing={instance} onClose={() => setEditing(false)} onSaved={saved} /> : null}
  </div>;
}
