import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowUpDown, Check, Download, ExternalLink, Heart, KeyRound, PackageSearch, RefreshCw, Search } from "lucide-react";
import { Button, EmptyState, Modal } from "@/components/UI";
import { api, errorMessage } from "@/lib/api";
import { formatDate, formatNumber } from "@/lib/format";
import { confirmCodeExecution, confirmHorizonModpack, confirmHorizonNative } from "@/lib/security";
import { useLauncher } from "@/state/LauncherContext";
import type { ContentEntry, ContentProvider, Instance, ModrinthVersion, ProjectHit, SearchIndex } from "@/types";

export type BrowserKind = "mod" | "native-mod" | "modpack" | "resourcepack" | "shader";

const PAGE_SIZE = 24;
const sortOptions: { value: SearchIndex; label: string }[] = [
  { value: "relevance", label: "Pertinence" },
  { value: "downloads", label: "Téléchargements" },
  { value: "follows", label: "Suivis" },
  { value: "newest", label: "Nouveautés" },
  { value: "updated", label: "Dernière mise à jour" },
];

function projectUrl(project: ProjectHit): string {
  const source = project.provider ?? "modrinth";
  if (source === "curseforge") {
    const segment = project.projectType === "mod"
      ? "mc-mods"
      : project.projectType === "resourcepack"
        ? "texture-packs"
        : project.projectType === "shader"
          ? "shaders"
          : "modpacks";
    return `https://www.curseforge.com/minecraft/${segment}/${project.slug}`;
  }
  if (source === "horizon") return `https://playhorizon.dev/mods/${project.slug}`;
  return `https://modrinth.com/${project.projectType}/${project.slug}`;
}

function providerLabelFor(project: ProjectHit, fallback: ContentProvider): string {
  const source = project.provider ?? fallback;
  return source === "curseforge" ? "CurseForge" : source === "horizon" ? "Horizon" : "Modrinth";
}

function matchesProject(item: ContentEntry, project: ProjectHit, fallback: ContentProvider): boolean {
  const itemProvider = item.provider ?? "modrinth";
  const projectProvider = project.provider ?? fallback;
  return item.projectId === project.projectId && itemProvider === projectProvider;
}

type Props = {
  kind: BrowserKind;
  instance?: Instance;
  installedContent?: ContentEntry[];
  onContentInstalled?: () => Promise<void>;
  onModpackInstalled?: (instance: Instance) => void;
};

export function ProjectBrowser({ kind, instance, installedContent = [], onContentInstalled, onModpackInstalled }: Props) {
  const { perform, notify } = useLauncher();
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<SearchIndex>(() => {
    const stored = localStorage.getItem("xal-sort");
    return sortOptions.some((option) => option.value === stored) ? stored as SearchIndex : "relevance";
  });
  const [selectedProvider, setSelectedProvider] = useState<"modrinth" | "curseforge" | "horizon">(() => {
    const stored = localStorage.getItem("xal-provider");
    if (stored === "curseforge") return "curseforge";
    if (stored === "horizon" && kind === "modpack") return "horizon";
    return "modrinth";
  });
  const [curseforgeReady, setCurseforgeReady] = useState<boolean>();
  const [curseforgeChecking, setCurseforgeChecking] = useState(false);
  const [results, setResults] = useState<ProjectHit[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const [installing, setInstalling] = useState<string>();
  const [selectedProject, setSelectedProject] = useState<ProjectHit>();
  const [versions, setVersions] = useState<ModrinthVersion[]>([]);
  const [selectedVersionId, setSelectedVersionId] = useState("");
  const [versionsLoading, setVersionsLoading] = useState(false);
  const sentinel = useRef<HTMLDivElement>(null);
  const request = useRef(0);
  const provider: ContentProvider = instance?.loader === "horizon" ? "horizon" : selectedProvider;
  const providerLabel = provider === "horizon" ? "Horizon" : provider === "curseforge" ? "CurseForge" : "Modrinth";
  const selectedVersion = versions.find((version) => version.id === selectedVersionId);
  const selectedVersionInstalled = Boolean(selectedProject && installedContent.some((item) => matchesProject(item, selectedProject, provider) && item.versionId === selectedVersionId));

  useEffect(() => {
    if (provider !== "curseforge") {
      setCurseforgeReady(undefined);
      setCurseforgeChecking(false);
      return;
    }
    let cancelled = false;
    setCurseforgeChecking(true);
    api.curseforgeKeyStatus()
      .then((status) => {
        if (!cancelled) {
          setCurseforgeReady(status.configured);
          setCurseforgeChecking(false);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setCurseforgeReady(false);
          setCurseforgeChecking(false);
        }
      });
    return () => { cancelled = true; };
  }, [provider]);

  function chooseProvider(next: "modrinth" | "curseforge" | "horizon") {
    if (next === selectedProvider) return;
    localStorage.setItem("xal-provider", next);
    setSelectedProvider(next);
    setResults([]);
    setHasMore(false);
    setSelectedProject(undefined);
    setLoading(true);
  }

  const loadPage = useCallback(async (offset: number, replace: boolean) => {
    const ticket = ++request.current;
    if (provider === "curseforge" && curseforgeReady !== true) {
      if (replace) setLoading(false);
      return;
    }
    if (replace) setLoading(true); else setLoadingMore(true);
    try {
      const page = provider === "horizon"
        ? await api.searchHorizon(query, kind, instance?.versionId ?? "26.3", instance?.loader ?? "horizon", offset, sort)
        : provider === "curseforge"
          ? await api.searchCurseforge(query, kind, instance?.versionId, instance?.loader, offset, sort)
          : await api.search(query, kind, instance?.versionId, instance?.loader, offset, sort);
      if (ticket !== request.current) return;
      setResults((items) => {
        if (replace) return page;
        const seen = new Set(items.map((item) => `${item.provider ?? provider}:${item.projectId}`));
        return [...items, ...page.filter((item) => !seen.has(`${item.provider ?? provider}:${item.projectId}`))];
      });
      setHasMore(page.length >= PAGE_SIZE);
    } catch (error) {
      if (ticket === request.current) notify(errorMessage(error), "error");
    } finally {
      if (ticket === request.current) { setLoading(false); setLoadingMore(false); }
    }
  }, [query, kind, instance?.versionId, instance?.loader, provider, sort, notify, curseforgeReady]);

  useEffect(() => {
    const timer = window.setTimeout(() => { void loadPage(0, true); }, query ? 350 : 0);
    return () => window.clearTimeout(timer);
  }, [loadPage, query]);

  useEffect(() => { localStorage.setItem("xal-sort", sort); }, [sort]);

  useEffect(() => {
    const node = sentinel.current;
    if (!node || loading || loadingMore || !hasMore) return;
    const observer = new IntersectionObserver((entries) => {
      if (entries[0]?.isIntersecting) void loadPage(results.length, false);
    }, { rootMargin: "320px" });
    observer.observe(node);
    return () => observer.disconnect();
  }, [hasMore, loading, loadingMore, results.length, loadPage]);

  async function openVersions(project: ProjectHit) {
    if (kind === "modpack") {
      if (provider === "horizon") {
        if (!(await confirmHorizonModpack(`« ${project.title} »`))) return;
        setInstalling(project.projectId);
        const created = await perform(() => api.installHorizonModpack(project.projectId), `« ${project.title} » installé`);
        setInstalling(undefined);
        if (created) onModpackInstalled?.(created);
        return;
      }
      if (!(await confirmCodeExecution("Ce modpack"))) return;
      setInstalling(project.projectId);
      const created = await perform(() => provider === "curseforge" ? api.installCurseforgeModpack(project.projectId) : api.installModpack(project.projectId), `« ${project.title} » installé`);
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
      const available = provider === "horizon"
        ? await api.horizonVersions(project.projectId, instance.id)
        : provider === "curseforge"
          ? await api.curseforgeVersions(project.projectId, instance.id)
          : await api.projectVersions(project.projectId, instance.id);
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
    if (provider === "horizon" && kind === "native-mod" && !(await confirmHorizonNative(`« ${selectedProject.title} »`))) return;
    setInstalling(selectedProject.projectId);
    const result = await perform(
      () => provider === "horizon"
        ? api.installHorizonProject(selectedProject.projectId, instance.id, selectedVersionId)
        : provider === "curseforge"
          ? api.installCurseforgeProject(selectedProject.projectId, instance.id, selectedVersionId)
          : api.installProject(selectedProject.projectId, instance.id, selectedVersionId),
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
    <div className="browser-toolbar">
      <label className="search-field explorer-search"><Search size={20} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={`Rechercher ${kind === "mod" ? "des mods" : kind === "native-mod" ? "des mods natifs" : kind === "modpack" ? "des modpacks" : kind === "shader" ? "des shaders" : "des resource packs"}…`} /><kbd>{providerLabel}</kbd></label>
      <label className="sort-field" title="Trier les résultats"><ArrowUpDown size={16} /><select aria-label="Trier les résultats" value={sort} onChange={(event) => setSort(event.target.value as SearchIndex)}>{sortOptions.map((option) => <option value={option.value} key={option.value}>{provider === "curseforge" && option.value === "follows" ? "Popularité" : option.label}</option>)}</select></label>
    </div>
    {instance?.loader !== "horizon" ? <div className="browser-providers"><div className="segmented"><button className={provider === "modrinth" ? "active" : ""} onClick={() => chooseProvider("modrinth")}>Modrinth</button><button className={provider === "curseforge" ? "active" : ""} onClick={() => chooseProvider("curseforge")}>CurseForge</button>{kind === "modpack" ? <button className={provider === "horizon" ? "active" : ""} onClick={() => chooseProvider("horizon")}>Horizon</button> : null}</div>{provider === "curseforge" ? <span className="provider-note">Données et téléchargements fournis par l’API CurseForge</span> : provider === "horizon" ? <span className="provider-note">Registre Horizon · modpacks (.hpack)</span> : null}</div> : null}
    <div className="browser-compatibility">{kind === "modpack" ? "Chaque modpack créera une nouvelle instance isolée." : provider === "horizon" ? `Contenus Horizon compatibles avec Minecraft ${instance?.versionId ?? "26.3"}` : `Contenus compatibles avec Minecraft ${instance?.versionId} · ${instance?.loader}`}</div>
    {provider === "curseforge" && curseforgeChecking ? <div className="panel-loading"><RefreshCw className="spin" />Vérification de la clé CurseForge…</div> : provider === "curseforge" && curseforgeReady === false ? <EmptyState icon={<KeyRound size={32} />} title="Clé API CurseForge requise" description="Ajoutez votre clé personnelle dans les paramètres. Elle reste dans le Coffre d’identification Windows et n’est jamais partagée." action={<Button onClick={() => navigate("/settings")}>Configurer CurseForge</Button>} /> : loading ? <div className="project-grid">{Array.from({ length: 6 }, (_, index) => <div className="project-card skeleton" key={index} />)}</div> : results.length ? <>
      <div className="project-grid">{results.map((project) => {
        const installed = installedContent.some((item) => matchesProject(item, project, provider));
        return <article className="project-card" key={`${project.provider ?? provider}-${project.projectId}`}>
          <button className="project-icon" onClick={() => void openUrl(projectUrl(project))} title={`Ouvrir sur ${providerLabelFor(project, provider)}`}>{project.iconUrl ? <img src={project.iconUrl} alt="" /> : <PackageSearch />}</button>
          <div className="project-info"><div className="project-title"><h3>{project.title}</h3>{installed ? <span className="provider-badge"><Check size={12} />Installé</span> : <span className="provider-badge">{providerLabelFor(project, provider)}</span>}<span>par {project.author}</span></div><p>{project.description}</p><div className="project-stats"><span><Download size={14} />{formatNumber(project.downloads)}</span><span><Heart size={14} />{formatNumber(project.follows)}</span><button onClick={() => void openUrl(projectUrl(project))}>Détails<ExternalLink size={13} /></button></div></div>
          <Button className="project-install" variant={installed ? "secondary" : "primary"} loading={installing === project.projectId && !selectedProject} disabled={kind !== "modpack" && !instance} onClick={() => void openVersions(project)}>{installed ? <Check size={16} /> : <Download size={16} />}{installed ? "Versions" : "Installer"}</Button>
        </article>;
      })}</div>
      <div ref={sentinel} className="browser-sentinel" aria-hidden="true" />
      {loadingMore ? <div className="panel-loading"><RefreshCw className="spin" />Chargement de la suite…</div> : null}
      {!loadingMore && hasMore ? <div className="browser-more"><Button variant="secondary" onClick={() => void loadPage(results.length, false)}>Charger plus</Button></div> : null}
    </> : <EmptyState icon={<PackageSearch size={32} />} title="Aucun contenu compatible" description={provider === "horizon" ? (kind === "shader" ? "Aucun shader Horizon compatible avec les lignes d’API servies par ce build. Modifiez votre recherche." : kind === "native-mod" ? "Aucun mod natif Horizon compatible avec ce build. Les versions marquées cassées sont masquées." : "Aucun projet Horizon compatible avec Minecraft 26.3. Modifiez votre recherche.") : "Modifiez votre recherche ou utilisez une instance avec une autre version de Minecraft."} />}
    {selectedProject ? <Modal title={`Installer ${selectedProject.title}`} description="Choisissez la publication compatible avec cette instance." onClose={closeVersions}>
      {versionsLoading ? <div className="panel-loading">Chargement des versions…</div> : versions.length ? <>
        <label className="field field-full"><span>Version du contenu</span><select autoFocus value={selectedVersionId} onChange={(event) => setSelectedVersionId(event.target.value)} disabled={Boolean(installing)}>{versions.map((version) => <option key={version.id} value={version.id}>{version.versionNumber} · {version.versionType}{installedContent.some((item) => matchesProject(item, selectedProject, provider) && item.versionId === version.id) ? " · installée" : ""}</option>)}</select></label>
        {selectedVersion ? <dl className="version-details"><div><dt>Nom</dt><dd>{selectedVersion.name}</dd></div><div><dt>Canal</dt><dd><span className={`version-channel ${selectedVersion.versionType}`}>{selectedVersion.versionType}</span></dd></div><div><dt>Publication</dt><dd>{formatDate(selectedVersion.datePublished)}</dd></div><div><dt>Minecraft</dt><dd>{selectedVersion.gameVersions.join(", ")}</dd></div><div><dt>Loaders</dt><dd>{selectedVersion.loaders.join(", ") || "Tous"}</dd></div></dl> : null}
        {provider === "horizon" && kind === "native-mod" ? <p className="provider-note">Ce bundle sera installé dans horizon-native-mods. Il devra être compilé par un build natif Horizon avant de se charger.</p> : null}
      </> : <EmptyState icon={<PackageSearch />} title="Aucune version compatible" description="Ce projet ne propose aucune publication pour cette instance." />}
      <footer className="modal-footer"><Button variant="secondary" disabled={Boolean(installing)} onClick={closeVersions}>Annuler</Button><Button loading={installing === selectedProject.projectId} disabled={!selectedVersionId || versionsLoading || selectedVersionInstalled} onClick={() => void installSelected()}>{selectedVersionInstalled ? <Check size={16} /> : <Download size={16} />}{selectedVersionInstalled ? "Version installée" : installedContent.some((item) => matchesProject(item, selectedProject, provider)) ? "Remplacer par cette version" : "Installer cette version"}</Button></footer>
    </Modal> : null}
  </div>;
}


