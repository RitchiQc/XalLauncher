import { useCallback, useEffect, useState } from "react";
import { Globe2, Pencil, Play, Plus, RefreshCw, Server, Signal, Trash2, Users } from "lucide-react";
import { Button, EmptyState, Modal, PageHeader } from "@/components/UI";
import { api } from "@/lib/api";
import { confirmAction } from "@/lib/dialog";
import { useLauncher } from "@/state/LauncherContext";
import type { ServerEntry, ServerStatus } from "@/types";

export function ServersPage() {
  const { servers, instances, refreshServers, perform, startGame } = useLauncher();
  const [statuses, setStatuses] = useState<Record<string, ServerStatus>>({});
  const [editing, setEditing] = useState<ServerEntry>();
  const [formOpen, setFormOpen] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [form, setForm] = useState({ name: "", address: "", port: "25565", instanceId: "" });
  const port = Number(form.port);
  const validPort = Number.isInteger(port) && port >= 1 && port <= 65535;
  const target = (server: ServerEntry) => server.address.includes(":") ? server.address : `${server.address}:${server.port}`;

  const refreshStatuses = useCallback(async () => {
    setRefreshing(true);
    await Promise.all(servers.map(async (server) => {
      const status = await api.queryServer(server);
      setStatuses((current) => ({ ...current, [server.id]: status }));
    }));
    setRefreshing(false);
  }, [servers]);
  useEffect(() => { void refreshStatuses(); }, [refreshStatuses]);

  function openForm(server?: ServerEntry) {
    setEditing(server);
    setForm(server ? { name: server.name, address: server.address, port: String(server.port), instanceId: server.instanceId ?? "" } : { name: "", address: "", port: "25565", instanceId: "" });
    setFormOpen(true);
  }
  async function save() {
    if (!validPort) return;
    const server: ServerEntry = { id: editing?.id ?? "", name: form.name, address: form.address, port, instanceId: form.instanceId || undefined };
    const result = await perform(() => api.saveServer(server), "Serveur enregistré");
    if (result) { await refreshServers(); setFormOpen(false); }
  }
  async function remove(server: ServerEntry) {
    if (!(await confirmAction(`Retirer ${server.name} des favoris ?`, "Retirer un serveur"))) return;
    const result = await perform(() => api.deleteServer(server.id), "Serveur supprimé");
    if (result) await refreshServers();
  }
  async function play(server: ServerEntry) {
    if (server.instanceId) await startGame(server.instanceId, target(server));
  }

  return <div className="page"><PageHeader eyebrow="Multijoueur" title="Serveurs" description="Vos mondes partagés et communautés préférées, à portée de clic." actions={<><Button variant="secondary" onClick={() => void refreshStatuses()} loading={refreshing}><RefreshCw size={17} />Actualiser</Button><Button onClick={() => openForm()}><Plus size={18} />Ajouter un serveur</Button></>} />
    {servers.length ? <div className="server-grid">{servers.map((server) => {
      const status = statuses[server.id];
      return <article className="server-card" key={server.id}><div className="server-icon">{status?.icon ? <img src={status.icon} alt="" /> : <Server />}</div><div className="server-main"><div className="server-title"><h3>{server.name}</h3><span className={status?.online ? "status-dot online" : "status-dot"}>{!status ? "Vérification…" : status.online ? "En ligne" : "Hors ligne"}</span></div><span className="server-address">{target(server)}</span><p>{status?.motd || status?.error || "Interrogation du serveur…"}</p><div className="server-meta">{status?.online ? <><span><Users size={14} />{status.onlinePlayers ?? 0} / {status.maxPlayers ?? 0}</span><span><Signal size={14} />{status.latency ?? 0} ms</span><span>{status.version}</span></> : null}</div></div><div className="server-actions">{server.instanceId ? <Button onClick={() => void play(server)}><Play size={16} />Rejoindre</Button> : <span className="muted">Aucune instance liée</span>}<Button variant="ghost" className="icon-button" onClick={() => openForm(server)}><Pencil size={16} /></Button><Button variant="ghost" className="icon-button danger-icon" onClick={() => void remove(server)}><Trash2 size={16} /></Button></div></article>;
    })}</div> : <EmptyState icon={<Globe2 size={32} />} title="Aucun serveur favori" description="Ajoutez une adresse pour consulter son statut et rejoindre rapidement avec l’instance de votre choix." action={<Button onClick={() => openForm()}><Plus size={17} />Ajouter un serveur</Button>} />}
    {formOpen ? <Modal title={editing ? "Modifier le serveur" : "Ajouter un serveur"} onClose={() => setFormOpen(false)}><div className="form-grid">
      <label className="field field-full"><span>Nom</span><input autoFocus value={form.name} onChange={(event) => setForm({ ...form, name: event.target.value })} placeholder="Mon serveur préféré" /></label>
      <label className="field"><span>Adresse</span><input value={form.address} onChange={(event) => setForm({ ...form, address: event.target.value })} placeholder="play.exemple.fr" /></label>
      <label className="field"><span>Port</span><input type="number" min={1} max={65535} value={form.port} onChange={(event) => setForm({ ...form, port: event.target.value })} /></label>
      <label className="field field-full"><span>Instance utilisée pour rejoindre</span><select value={form.instanceId} onChange={(event) => setForm({ ...form, instanceId: event.target.value })}><option value="">Ne pas lancer automatiquement</option>{instances.map((instance) => <option value={instance.id} key={instance.id}>{instance.name} · {instance.versionId}</option>)}</select></label>
    </div><footer className="modal-footer"><Button variant="secondary" onClick={() => setFormOpen(false)}>Annuler</Button><Button onClick={() => void save()} disabled={!form.name || !form.address || !validPort}>Enregistrer</Button></footer></Modal> : null}
  </div>;
}
