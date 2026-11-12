import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Box, Compass, Copy, MoreHorizontal, Plus, Search, Trash2, Upload } from "lucide-react";
import { ImportInstanceDialog } from "@/components/ImportInstanceDialog";
import { InstanceArtwork } from "@/components/InstanceArtwork";
import { InstanceForm } from "@/components/InstanceForm";
import { ProjectBrowser } from "@/components/ProjectBrowser";
import { Button, EmptyState, Modal, PageHeader } from "@/components/UI";
import { api } from "@/lib/api";
import { useLauncher } from "@/state/LauncherContext";
import type { Instance } from "@/types";

export function InstancesPage() {
  const { instances, refreshInstances, setActiveInstanceId, perform } = useLauncher();
  const [creating, setCreating] = useState(false);
  const [importing, setImporting] = useState(false);
  const [discovering, setDiscovering] = useState(false);
  const [query, setQuery] = useState("");
  const [deleting, setDeleting] = useState<Instance>();
  const [deleteFiles, setDeleteFiles] = useState(true);
  const navigate = useNavigate();
  const filtered = useMemo(() => instances.filter((instance) => `${instance.name} ${instance.versionId} ${instance.loader}`.toLowerCase().includes(query.toLowerCase())), [instances, query]);

  function saved(instance: Instance) {
    void refreshInstances(); setCreating(false); setDiscovering(false); setImporting(false); setActiveInstanceId(instance.id); navigate(`/instances/${instance.id}`);
  }
  async function duplicate(id: string) {
    const result = await perform(() => api.duplicateInstance(id), "Instance dupliquée");
    if (result) await refreshInstances();
  }
  async function remove() {
    if (!deleting) return;
    const result = await perform(() => api.deleteInstance(deleting.id, deleteFiles), "Instance supprimée");
    if (result) { await refreshInstances(); setDeleting(undefined); }
  }

  return <div className="page">
    <PageHeader eyebrow="Vos installations" title="Instances" description="Des environnements isolés pour toutes vos façons de jouer." actions={<><Button variant="secondary" onClick={() => setDiscovering(true)}><Compass size={17} />Découvrir des modpacks</Button><Button variant="secondary" onClick={() => setImporting(true)} title="Formats : .mrpack, .zip Prism/MultiMC"><Upload size={17} />Importer</Button><Button onClick={() => setCreating(true)}><Plus size={18} />Nouvelle instance</Button></>} />
    <div className="toolbar"><label className="search-field"><Search size={18} /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Rechercher une instance…" /></label><span className="muted">{instances.length} instance{instances.length > 1 ? "s" : ""}</span></div>
    {filtered.length ? <div className="instance-grid">{filtered.map((instance) => <article className="instance-card" key={instance.id}>
      <button className="instance-card-main" onClick={() => { setActiveInstanceId(instance.id); navigate(`/instances/${instance.id}`); }}><InstanceArtwork instance={instance} compact /></button>
      <footer><span><strong>{instance.name}</strong><small>{instance.totalPlaySeconds ? `${Math.floor(instance.totalPlaySeconds / 3600)} h de jeu` : "Prête à être lancée"}</small></span><div className="card-actions"><Button variant="ghost" className="icon-button" title="Dupliquer" onClick={() => void duplicate(instance.id)}><Copy size={17} /></Button><Button variant="ghost" className="icon-button" title="Supprimer" onClick={() => setDeleting(instance)}><Trash2 size={17} /></Button><Button variant="ghost" className="icon-button" title="Ouvrir" onClick={() => navigate(`/instances/${instance.id}`)}><MoreHorizontal size={18} /></Button></div></footer>
    </article>)}</div> : <EmptyState icon={<Box size={30} />} title={query ? "Aucun résultat" : "Aucune instance"} description={query ? "Essayez un autre nom, une autre version ou un autre modloader." : "Créez une instance ou importez une archive .mrpack / Prism-MultiMC pour commencer."} action={!query ? <Button onClick={() => setCreating(true)}><Plus size={17} />Créer une instance</Button> : undefined} />}
    {creating ? <InstanceForm onClose={() => setCreating(false)} onSaved={saved} /> : null}
    {importing ? <ImportInstanceDialog onClose={() => { setImporting(false); void refreshInstances(); }} onImported={(instance) => { setImporting(false); saved(instance); }} /> : null}
    {discovering ? <Modal title="Découvrir des modpacks" description="L’installation créera automatiquement une nouvelle instance isolée." onClose={() => setDiscovering(false)} wide><ProjectBrowser kind="modpack" onModpackInstalled={saved} /></Modal> : null}
    {deleting ? <Modal title={`Supprimer ${deleting.name} ?`} description="Cette action est irréversible." onClose={() => setDeleting(undefined)}>
      <label className="check-row"><input type="checkbox" checked={deleteFiles} onChange={(event) => setDeleteFiles(event.target.checked)} /><span><strong>Supprimer aussi les fichiers du jeu</strong><small>Mods, mondes, captures et configurations locaux seront effacés.</small></span></label>
      <footer className="modal-footer"><Button variant="secondary" onClick={() => setDeleting(undefined)}>Annuler</Button><Button variant="danger" onClick={() => void remove()}><Trash2 size={17} />Supprimer définitivement</Button></footer>
    </Modal> : null}
  </div>;
}
