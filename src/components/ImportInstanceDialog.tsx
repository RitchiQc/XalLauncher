import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertCircle, CheckCircle2, FileArchive, FolderOpen, LoaderCircle, Upload } from "lucide-react";
import { Button, Modal } from "@/components/UI";
import { api, errorMessage } from "@/lib/api";
import { confirmCodeExecution } from "@/lib/security";
import { useLauncher } from "@/state/LauncherContext";
import type { ImportReport, Instance } from "@/types";

export function ImportInstanceDialog({ onClose, onImported }: { onClose: () => void; onImported: (instance: Instance) => void }) {
  const { notify } = useLauncher();
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [busyLabel, setBusyLabel] = useState("");
  const [report, setReport] = useState<ImportReport>();
  const [error, setError] = useState("");

  async function run(action: () => Promise<ImportReport>, label: string) {
    setError("");
    setReport(undefined);
    setBusyLabel(label);
    setBusy(true);
    try {
      const result = await action();
      setReport(result);
      notify("Instance importée", "success");
    } catch (exception) {
      setError(errorMessage(exception));
    } finally {
      setBusy(false);
      setBusyLabel("");
    }
  }

  async function importArchive() {
    if (!(await confirmCodeExecution("Ce modpack"))) return;
    const path = await open({ multiple: false, filters: [{ name: "Instances", extensions: ["mrpack", "zip"] }] });
    if (!path || Array.isArray(path)) return;
    const fileName = path.split(/[\\/]/).pop() ?? "archive";
    await run(() => api.importInstance(path, name.trim() || undefined), `Import de ${fileName}…`);
  }

  async function importFolder() {
    if (!(await confirmCodeExecution("Cette instance"))) return;
    const path = await open({ directory: true, multiple: false });
    if (!path || Array.isArray(path)) return;
    const folderName = path.split(/[\\/]/).filter(Boolean).pop() ?? "dossier";
    await run(() => api.importInstanceFolder(path, name.trim() || undefined), `Import de ${folderName}…`);
  }

  return <Modal title="Importer une instance" description="Reprenez une instance existante sans réinstaller vos mods." onClose={() => { if (!busy) onClose(); }} wide>
    <label className="field field-full"><span>Nom de l’instance (facultatif)</span><input value={name} onChange={(event) => setName(event.target.value)} placeholder="Le nom du pack est utilisé par défaut" disabled={busy} /></label>
    <div className="format-list">
      <div className="format-row"><span className="format-badge">.mrpack</span><div><strong>Modrinth</strong><small>Mods, configs, resource packs et shaders. Un pack exporté par XalLauncher restaure aussi ses réglages (RAM, résolution, JVM, modules).</small></div></div>
      <div className="format-row"><span className="format-badge">.zip</span><div><strong>Prism Launcher / MultiMC</strong><small>Archive d’instance ou dossier contenant <code>mmc-pack.json</code> : mods, configs, mondes et réglages.</small></div></div>
    </div>
    <details className="format-help"><summary>Formats non pris en charge</summary>
      <p><strong>CurseForge (.zip)</strong> : non pris en charge, car le téléchargement de ses mods exige une clé API CurseForge. Importez d’abord le pack dans Prism Launcher, puis importez le dossier de l’instance Prism ici.</p>
      <p>ATLauncher, FTB, Technic et un dossier <code>.minecraft</code> brut ne sont pas reconnus.</p>
    </details>
    {busy ? <div className="panel-loading"><LoaderCircle className="spin" />{busyLabel} Les téléchargements peuvent prendre plusieurs minutes.</div> : null}
    {error ? <div className="form-error field-full" role="alert"><AlertCircle size={17} /><span>{error}</span></div> : null}
    {report ? <div className="import-report">
      <div className="import-report-head"><CheckCircle2 size={22} /><div><strong>{report.instance.name}</strong><small>{report.format} · {report.importedFiles} fichier{report.importedFiles > 1 ? "s" : ""}</small></div></div>
      {report.warnings.length ? <ul className="import-warnings">{report.warnings.map((warning) => <li key={warning}>{warning}</li>)}</ul> : null}
      <footer className="modal-footer"><Button variant="secondary" onClick={onClose}>Fermer</Button><Button onClick={() => onImported(report.instance)}><FileArchive size={17} />Ouvrir l’instance</Button></footer>
    </div> : <footer className="modal-footer"><Button variant="secondary" onClick={onClose} disabled={busy}>Annuler</Button><Button variant="secondary" onClick={() => void importFolder()} disabled={busy}><FolderOpen size={17} />Choisir un dossier</Button><Button onClick={() => void importArchive()} disabled={busy}><Upload size={17} />Choisir une archive</Button></footer>}
  </Modal>;
}