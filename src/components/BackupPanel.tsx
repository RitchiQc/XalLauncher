import { useCallback, useEffect, useState } from "react";
import { ArchiveRestore, Plus, ShieldCheck, Trash2 } from "lucide-react";
import { api } from "@/lib/api";
import { confirmAction } from "@/lib/dialog";
import { formatBytes, formatDate } from "@/lib/format";
import { useLauncher } from "@/state/LauncherContext";
import type { BackupEntry } from "@/types";
import { Button, EmptyState } from "./UI";

export function BackupPanel({ instanceId }: { instanceId: string }) {
  const { perform } = useLauncher();
  const [backups, setBackups] = useState<BackupEntry[]>([]);
  const [busy, setBusy] = useState(false);
  const refresh = useCallback(async () => setBackups(await api.backups(instanceId)), [instanceId]);
  useEffect(() => { void refresh(); }, [refresh]);

  async function create() {
    setBusy(true); const result = await perform(() => api.createBackup(instanceId), "Sauvegarde des mondes terminée"); setBusy(false);
    if (result) await refresh();
  }
  async function restore(backup: BackupEntry) {
    if (!(await confirmAction(`Restaurer « ${backup.name} » ? Les fichiers portant le même nom seront remplacés.`, "Restaurer une sauvegarde"))) return;
    setBusy(true); const result = await perform(() => api.restoreBackup(backup.id), "Mondes restaurés"); setBusy(false);
    if (result !== undefined) await refresh();
  }
  async function remove(backup: BackupEntry) {
    if (!(await confirmAction("Supprimer cette sauvegarde ?", "Supprimer une sauvegarde"))) return;
    const result = await perform(() => api.deleteBackup(backup.id), "Sauvegarde supprimée");
    if (result !== undefined) await refresh();
  }

  return <section className="content-section"><div className="section-toolbar"><div><h3>Sauvegardes des mondes</h3><span className="muted">Archives ZIP locales de tous les mondes de cette instance.</span></div><Button onClick={() => void create()} loading={busy}><Plus size={17} />Nouvelle sauvegarde</Button></div>
    <div className="info-banner"><ShieldCheck size={19} /><span>Créez une sauvegarde avant de mettre à jour des mods ou de modifier un monde important.</span></div>
    {backups.length ? <div className="content-list">{backups.map((backup) => <article className="content-row" key={backup.id}><div className="content-icon"><ArchiveRestore /></div><div className="content-info"><strong>{backup.name}</strong><span>{formatDate(backup.createdAt)} · {formatBytes(backup.size)}</span></div><Button variant="secondary" onClick={() => void restore(backup)} disabled={busy}><ArchiveRestore size={16} />Restaurer</Button><Button variant="ghost" className="icon-button danger-icon" onClick={() => void remove(backup)}><Trash2 size={17} /></Button></article>)}</div> : <EmptyState icon={<ArchiveRestore size={30} />} title="Aucune sauvegarde" description="Vos mondes peuvent être archivés et restaurés ici en un clic." action={<Button onClick={() => void create()} loading={busy}>Créer une sauvegarde</Button>} />}
  </section>;
}
