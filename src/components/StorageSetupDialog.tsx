import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, HardDrive } from "lucide-react";
import { Button, Modal, Toggle } from "@/components/UI";
import { api, errorMessage } from "@/lib/api";
import type { StorageInfo } from "@/types";

export function StorageSetupDialog({ info, onClose }: { info: StorageInfo; onClose: () => void }) {
  const suggestion = info.hasExistingData && info.installDataDirectory !== info.dataDirectory ? info.installDataDirectory : info.defaultDirectory;
  const [selected, setSelected] = useState(suggestion !== info.dataDirectory ? suggestion : info.dataDirectory);
  const [migrate, setMigrate] = useState(info.hasExistingData);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const changes = selected !== info.dataDirectory;

  async function choose() {
    const path = await open({ directory: true, multiple: false });
    if (path && !Array.isArray(path)) setSelected(path);
  }

  async function confirm() {
    setBusy(true); setError("");
    try {
      await api.changeStorageDirectory(selected, changes && migrate);
      if (changes) await api.restartApp();
      else onClose();
    } catch (exception) {
      setError(errorMessage(exception));
      setBusy(false);
    }
  }

  async function later() {
    setBusy(true); setError("");
    try {
      await api.changeStorageDirectory(info.dataDirectory, false);
      onClose();
    } catch (exception) {
      setError(errorMessage(exception));
      setBusy(false);
    }
  }

  return <Modal title="Dossier de données" description="Choisissez où XalLauncher stocke les instances, téléchargements et sauvegardes." onClose={busy ? () => undefined : onClose}>
    <div className="storage-setup">
      <div className="storage-current"><HardDrive size={18} /><span><strong>Dossier proposé</strong><small>{selected}</small></span></div>
      {info.hasExistingData ? <div className="storage-note">Vos données actuelles se trouvent dans <code>{info.dataDirectory}</code> ({info.instanceCount} instance{info.instanceCount > 1 ? "s" : ""}).</div> : null}
      <div className="storage-actions">
        <Button variant="secondary" onClick={() => void choose()} disabled={busy}><FolderOpen size={16} />Choisir un dossier</Button>
        {info.installDataDirectory !== info.dataDirectory ? <Button variant="ghost" onClick={() => setSelected(info.installDataDirectory)} disabled={busy}>Utiliser le dossier d'installation</Button> : null}
      </div>
      {changes && info.hasExistingData ? <Toggle checked={migrate} onChange={setMigrate} label="Reprendre mes instances et téléchargements existants" /> : null}
      {changes ? <div className="storage-note">XalLauncher redémarrera après la validation. {migrate ? "Les données existantes seront reprises ; l'ancien dossier est conservé par sécurité." : "Le nouveau dossier utilisera ses propres données ; l'ancien sera conservé."}</div> : null}
      {error ? <div className="form-error" role="alert">{error}</div> : null}
    </div>
    <footer className="modal-footer"><Button variant="secondary" onClick={() => void later()} disabled={busy}>Plus tard</Button><Button onClick={() => void confirm()} loading={busy} disabled={!selected}>Continuer</Button></footer>
  </Modal>;
}
