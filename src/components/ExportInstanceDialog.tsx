import { useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { Archive, Download } from "lucide-react";
import { Button, Modal, Toggle } from "@/components/UI";
import { api, errorMessage } from "@/lib/api";
import { formatExportReport } from "@/lib/format";
import { useLauncher } from "@/state/LauncherContext";
import type { Instance } from "@/types";

function fileStem(name: string): string {
  return name.replace(/[\\/:*?"<>|]/g, "-").trim() || "instance";
}

export function ExportInstanceDialog({ instance, onClose }: { instance: Instance; onClose: () => void }) {
  const { notify, running } = useLauncher();
  const [includeWorlds, setIncludeWorlds] = useState(false);
  const [includeScreenshots, setIncludeScreenshots] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const isRunning = running.has(instance.id);

  async function exportInstance() {
    const destination = await save({ defaultPath: `${fileStem(instance.name)}.mrpack`, filters: [{ name: "Modpack Modrinth", extensions: ["mrpack"] }] });
    if (!destination) return;
    setError("");
    setBusy(true);
    try {
      const report = await api.exportInstance(instance.id, destination, { includeWorlds, includeScreenshots });
      notify(`Export terminé · ${formatExportReport(report)}`, "success");
      onClose();
    } catch (exception) {
      setError(errorMessage(exception));
    } finally {
      setBusy(false);
    }
  }

  return <Modal title="Exporter l’instance" description="Créez une archive .mrpack autonome, réimportable dans XalLauncher et les autres launchers." onClose={() => { if (!busy) onClose(); }}>
    <div className="format-highlight"><Archive size={24} /><div><strong>Modrinth (.mrpack)</strong><small>Compatible XalLauncher, Prism Launcher, MultiMC, ATLauncher et Modrinth App. Tous les fichiers sont inclus : aucun téléchargement n’est nécessaire à l’import.</small></div></div>
    <Toggle checked={includeWorlds} onChange={setIncludeWorlds} label="Inclure les mondes" />
    <Toggle checked={includeScreenshots} onChange={setIncludeScreenshots} label="Inclure les captures d’écran" />
    <p className="export-note">Les journaux, rapports de crash, caches et le pack de branding XalLauncher sont toujours exclus. Les réglages de l’instance (RAM, résolution, JVM, modules) sont enregistrés dans <code>xallauncher.json</code> et restaurés à l’import dans XalLauncher.</p>
    {isRunning ? <div className="form-error field-full" role="alert"><span>Arrêtez le jeu avant d’exporter cette instance.</span></div> : null}
    {error ? <div className="form-error field-full" role="alert"><span>{error}</span></div> : null}
    <footer className="modal-footer"><Button variant="secondary" onClick={onClose} disabled={busy}>Annuler</Button><Button onClick={() => void exportInstance()} loading={busy} disabled={isRunning}><Download size={17} />Exporter</Button></footer>
  </Modal>;
}