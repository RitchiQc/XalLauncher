import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Box, Download, ExternalLink, Heart, PackageSearch, Search } from "lucide-react";
import { Button, EmptyState, PageHeader } from "@/components/UI";
import { api, errorMessage } from "@/lib/api";
import { formatNumber } from "@/lib/format";
import { useLauncher } from "@/state/LauncherContext";
import type { ProjectHit } from "@/types";

const kinds = [{ id: "mod", label: "Mods" }, { id: "modpack", label: "Modpacks" }, { id: "resourcepack", label: "Resource packs" }, { id: "shader", label: "Shaders" }];

export function ExplorerPage() {
  const { instances, activeInstance, activeInstanceId, setActiveInstanceId, perform, notify, refreshInstances, setActiveInstanceId: selectInstance } = useLauncher();
  const [kind, setKind] = useState("mod");
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<ProjectHit[]>([]);
  const [loading, setLoading] = useState(true);
  const [installing, setInstalling] = useState<string>();
  const navigate = useNavigate();

  useEffect(() => {
    const timer = window.setTimeout(() => {
      setLoading(true);
      api.search(query, kind, activeInstance?.versionId, activeInstance?.loader)
        .then(setResults).catch((error) => notify(errorMessage(error), "error")).finally(() => setLoading(false));
    }, query ? 350 : 0);
    return () => window.clearTimeout(timer);
  }, [query, kind, activeInstance?.versionId, activeInstance?.loader, notify]);

  async function install(project: ProjectHit) {
    if (kind !== "modpack" && !activeInstance) { notify("Créez ou sélectionnez une instance avant l’installation", "error"); return; }
    setInstalling(project.projectId);
    if (kind === "modpack") {
      const instance = await perform(() => api.installModpack(project.projectId), `« ${project.title} » installé`);
      if (instance) {
        await refreshInstances(); selectInstance(instance.id); navigate(`/instances/${instance.id}`);
      }
    } else if (activeInstance) {
      await perform(() => api.installProject(project.projectId, activeInstance.id), `« ${project.title} » installé`);
    }
    setInstalling(undefined);
  }

  return <div className="page">
    <PageHeader eyebrow="Propulsé par Modrinth" title="Explorer" description="Découvrez et installez du contenu compatible avec vos instances." />
    <div className="explorer-controls"><div className="segmented">{kinds.map((item) => <button key={item.id} className={kind === item.id ? "active" : ""} onClick={() => setKind(item.id)}>{item.label}</button>)}</div>
      {kind !== "modpack" ? <label className="field compact-field"><span>Installer dans</span><select value={activeInstanceId} onChange={(event) => setActiveInstanceId(event.target.value)}><option value="">Choisir une instance</option>{instances.map((instance) => <option value={instance.id} key={instance.id}>{instance.name} · {instance.versionId}</option>)}</select></label> : <span className="info-pill"><Box size={15} />Une nouvelle instance sera créée</span>}
    </div>
    <label className="search-field explorer-search"><Search size={20} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={`Rechercher ${kinds.find((item) => item.id === kind)?.label.toLowerCase()}…`} /><kbd>Modrinth</kbd></label>
    {loading ? <div className="project-grid">{Array.from({ length: 8 }, (_, index) => <div className="project-card skeleton" key={index} />)}</div> : results.length ? <div className="project-grid">{results.map((project) => <article className="project-card" key={project.projectId}>
      <button className="project-icon" onClick={() => void openUrl(`https://modrinth.com/${project.projectType}/${project.slug}`)}>{project.iconUrl ? <img src={project.iconUrl} alt="" /> : <PackageSearch />}</button>
      <div className="project-info"><div className="project-title"><h3>{project.title}</h3><span>par {project.author}</span></div><p>{project.description}</p><div className="project-stats"><span><Download size={14} />{formatNumber(project.downloads)}</span><span><Heart size={14} />{formatNumber(project.follows)}</span><button onClick={() => void openUrl(`https://modrinth.com/${project.projectType}/${project.slug}`)}>Détails<ExternalLink size={13} /></button></div></div>
      <Button className="project-install" loading={installing === project.projectId} disabled={kind !== "modpack" && !activeInstance} onClick={() => void install(project)}><Download size={16} />Installer</Button>
    </article>)}</div> : <EmptyState icon={<PackageSearch size={32} />} title="Aucun contenu compatible" description="Modifiez votre recherche ou choisissez une instance avec une autre version de Minecraft." />}
  </div>;
}
