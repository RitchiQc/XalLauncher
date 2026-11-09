import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Check, Download, ExternalLink, Heart, PackageSearch, Search } from "lucide-react";
import { Button, EmptyState, Modal } from "@/components/UI";
import { api, errorMessage } from "@/lib/api";
import { formatDate, formatNumber } from "@/lib/format";
import { confirmCodeExecution } from "@/lib/security";
import { useLauncher } from "@/state/LauncherContext";
import type { ContentEntry, Instance, ModrinthVersion, ProjectHit } from "@/types";

export type BrowserKind = "mod" | "modpack" | "resourcepack" | "shader";

type Props = {
  kind: BrowserKind;
  instance?: Instance;
  installedContent?: ContentEntry[];
  onContentInstalled?: () => Promise<void>;
  onModpackInstalled?: (instance: Instance) => void;
};

export function ProjectBrowser({ kind, instance, installedContent = [], onContentInstalled, onModpackInstalled }: Props) {
  const { perform, notify } = useLauncher();
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<ProjectHit[]>([]);
  const [loading, setLoading] = useState(true);
  const [installing, setInstalling] = useState<string>();
  const [selectedProject, setSelectedProject] = useState<ProjectHit>();
  const [versions, setVersions] = useState<ModrinthVersion[]>([]);
  const [selectedVersionId, setSelectedVersionId] = useState("");
  const [versionsLoading, setVersionsLoading] = useState(false);
  const selectedVersion = versions.find((version) => version.id === selectedVersionId);
  const selectedVersionInstalled = Boolean(selectedProject && installedContent.some((item) => item.projectId === selectedProject.projectId && item.versionId === selectedVersionId));

  useEffect(() => {
    const timer = window.setTimeout(() => {
      setLoading(true);
      api.search(query, kind, instance?.versionId, instance?.loader)
        .then(setResults)
        .catch((error) => notify(errorMessage(error), "error"))
        .finally(() => setLoading(false));
    }, query ? 350 : 0);
    return () => window.clearTimeout(timer);
  }, [query, kind, instance?.versionId, instance?.loader, notify]);

  async function openVersions(project: ProjectHit) {
    if (kind === "modpack") {
      if (!confirmCodeExecution("Ce modpack")) return;
      setInstalling(project.projectId);
      const created = await perform(() => api.installModpack(project.projectId), `« ${project.title} » installé`);
      setInstalling(undefined);
      if (created) onModpackInstalled?.(created);
      return;
    }
    if (!instance) return;
    setSelectedProject(project);
    setVersions([]);
    setSelectedVersionId("");
    setVersionsLoading(true);
    try {
      const available = await api.projectVersions(project.projectId, instance.id);
      setVersions(available);
      setSelectedVersionId(available[0]?.id ?? "");
    } catch (error) {
      notify(errorMessage(error), "error");
      setSelectedProject(undefined);
    } finally {
      setVersionsLoading(false);
    }
  }

  async function installSelected() {
    if (!instance || !selectedProject || !selectedVersionId) return;
    setInstalling(selectedProject.projectId);
    const result = await perform(
      () => api.installProject(selectedProject.projectId, instance.id, selectedVersionId),
      `« ${selectedProject.title} » installé`,
    );
    setInstalling(undefined);
    if (result) {
      setSelectedProject(undefined);
      await onContentInstalled?.();
    }
  }

  function closeVersions() {
    if (!installing) setSelectedProject(undefined);
  }


  return <div className="project-browser">
    <label className="search-field explorer-search"><Search size={20} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={`Rechercher ${kind === "mod" ? "des mods" : kind === "modpack" ? "des modpacks" : kind === "shader" ? "des shaders" : "des resource packs"}…`} /><kbd>Modrinth</kbd></label>
    <div className="browser-compatibility">{kind === "modpack" ? "Chaque modpack créera une nouvelle instance isolée." : `Contenus compatibles avec Minecraft ${instance?.versionId} · ${instance?.loader}`}</div>
    {loading ? <div className="project-grid">{Array.from({ length: 6 }, (_, index) => <div className="project-card skeleton" key={index} />)}</div> : results.length ? <div className="project-grid">{results.map((project) => {
      const installed = installedContent.some((item) => item.projectId === project.projectId);
      return <article className="project-card" key={project.projectId}>
        <button className="project-icon" onClick={() => void openUrl(`https://modrinth.com/${project.projectType}/${project.slug}`)} title="Ouvrir sur Modrinth">{project.iconUrl ? <img src={project.iconUrl} alt="" /> : <PackageSearch />}</button>
        <div className="project-info"><div className="project-title"><h3>{project.title}</h3>{installed ? <span className="provider-badge"><Check size={12} />Installé</span> : null}<span>par {project.author}</span></div><p>{project.description}</p><div className="project-stats"><span><Download size={14} />{formatNumber(project.downloads)}</span><span><Heart size={14} />{formatNumber(project.follows)}</span><button onClick={() => void openUrl(`https://modrinth.com/${project.projectType}/${project.slug}`)}>Détails<ExternalLink size={13} /></button></div></div>
        <Button className="project-install" variant={installed ? "secondary" : "primary"} loading={installing === project.projectId && !selectedProject} disabled={kind !== "modpack" && !instance} onClick={() => void openVersions(project)}>{installed ? <Check size={16} /> : <Download size={16} />}{installed ? "Versions" : "Installer"}</Button>
      </article>;
    })}</div> : <EmptyState icon={<PackageSearch size={32} />} title="Aucun contenu compatible" description="Modifiez votre recherche ou utilisez une instance avec une autre version de Minecraft." />}
    {selectedProject ? <Modal title={`Installer ${selectedProject.title}`} description="Choisissez la publication compatible avec cette instance." onClose={closeVersions}>
      {versionsLoading ? <div className="panel-loading">Chargement des versions…</div> : versions.length ? <>
        <label className="field field-full"><span>Version du contenu</span><select autoFocus value={selectedVersionId} onChange={(event) => setSelectedVersionId(event.target.value)} disabled={Boolean(installing)}>{versions.map((version) => <option key={version.id} value={version.id}>{version.versionNumber} · {version.versionType}{installedContent.some((item) => item.projectId === selectedProject.projectId && item.versionId === version.id) ? " · installée" : ""}</option>)}</select></label>
        {selectedVersion ? <dl className="version-details"><div><dt>Nom</dt><dd>{selectedVersion.name}</dd></div><div><dt>Canal</dt><dd><span className={`version-channel ${selectedVersion.versionType}`}>{selectedVersion.versionType}</span></dd></div><div><dt>Publication</dt><dd>{formatDate(selectedVersion.datePublished)}</dd></div><div><dt>Minecraft</dt><dd>{selectedVersion.gameVersions.join(", ")}</dd></div><div><dt>Loaders</dt><dd>{selectedVersion.loaders.join(", ") || "Tous"}</dd></div></dl> : null}
      </> : <EmptyState icon={<PackageSearch />} title="Aucune version compatible" description="Ce projet ne propose aucune publication pour cette instance." />}
      <footer className="modal-footer"><Button variant="secondary" disabled={Boolean(installing)} onClick={closeVersions}>Annuler</Button><Button loading={installing === selectedProject.projectId} disabled={!selectedVersionId || versionsLoading || selectedVersionInstalled} onClick={() => void installSelected()}>{selectedVersionInstalled ? <Check size={16} /> : <Download size={16} />}{selectedVersionInstalled ? "Version installée" : installedContent.some((item) => item.projectId === selectedProject.projectId) ? "Remplacer par cette version" : "Installer cette version"}</Button></footer>
    </Modal> : null}
  </div>;
}
