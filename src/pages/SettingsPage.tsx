import { useState } from "react";
import { Check, Download, KeyRound, MonitorCog, Moon, Palette, RefreshCw, Save, Sun } from "lucide-react";
import { Button, PageHeader, ProgressBar, Toggle } from "@/components/UI";
import { api } from "@/lib/api";
import { findUpdate } from "@/lib/updates";
import { useLauncher } from "@/state/LauncherContext";

const accents = ["#8b5cf6", "#6366f1", "#06b6d4", "#10b981", "#f59e0b", "#ef4444", "#ec4899"];

export function SettingsPage() {
  const { settings, setSettings, perform } = useLauncher();
  const [form, setForm] = useState(settings);
  const [saving, setSaving] = useState(false);
  const [updateStatus, setUpdateStatus] = useState("");
  const [updateProgress, setUpdateProgress] = useState<number>();
  const dirty = JSON.stringify(form) !== JSON.stringify(settings);
  function patch<K extends keyof typeof form>(key: K, value: (typeof form)[K]) { setForm((current) => ({ ...current, [key]: value })); }
  async function save() {
    setSaving(true); const result = await perform(() => api.saveSettings(form), "Paramètres enregistrés"); setSaving(false);
    if (result) setSettings(result);
  }
  async function checkUpdate() {
    setUpdateStatus("Recherche d’une mise à jour…"); setUpdateProgress(undefined);
    const update = await perform(() => findUpdate());
    if (!update) { setUpdateStatus("Vous utilisez la dernière version disponible."); return; }
    if (!window.confirm(`La version ${update.version} est disponible. L’installer maintenant ?`)) { setUpdateStatus(`Version ${update.version} disponible.`); return; }
    let downloaded = 0;
    let total = 0;
    setUpdateStatus(`Téléchargement de la version ${update.version}…`);
    const result = await perform(() => update.downloadAndInstall((event) => {
      if (event.event === "Started") total = event.data.contentLength ?? 0;
      if (event.event === "Progress") {
        downloaded += event.data.chunkLength;
        if (total > 0) setUpdateProgress(downloaded / total * 100);
      }
      if (event.event === "Finished") setUpdateProgress(100);
    }), "Mise à jour installée. XalLauncher va redémarrer.");
    if (result !== undefined) setUpdateStatus("Installation terminée.");
  }

  return <div className="page settings-page"><PageHeader eyebrow="Personnalisation" title="Paramètres" description="Réglez XalLauncher selon votre machine et vos préférences." actions={<Button onClick={() => void save()} loading={saving} disabled={!dirty}><Save size={17} />Enregistrer</Button>} />
    <section className="settings-section panel"><header><Palette /><div><h2>Apparence</h2><p>Un environnement qui vous ressemble, sans nuire à la lisibilité.</p></div></header><div className="settings-body">
      <div className="setting-row"><span><strong>Thème</strong><small>Choisissez l’ambiance de l’interface.</small></span><div className="segmented"><button className={form.theme === "dark" ? "active" : ""} onClick={() => patch("theme", "dark")}><Moon size={15} />Sombre</button><button className={form.theme === "light" ? "active" : ""} onClick={() => patch("theme", "light")}><Sun size={15} />Clair</button></div></div>
      <div className="setting-row"><span><strong>Couleur d’accent</strong><small>Boutons, sélections et indicateurs de progression.</small></span><div className="color-picker">{accents.map((color) => <button key={color} className={form.accent === color ? "selected" : ""} style={{ background: color }} onClick={() => patch("accent", color)}>{form.accent === color ? <Check size={15} /> : null}</button>)}</div></div>
      <Toggle checked={form.reducedMotion} onChange={(value) => patch("reducedMotion", value)} label="Réduire les animations et transitions" />
    </div></section>
    <section className="settings-section panel"><header><Download /><div><h2>Téléchargements</h2><p>Contrôlez la charge réseau pendant l’installation du jeu et des contenus.</p></div></header><div className="settings-body"><label className="setting-row"><span><strong>Tâches simultanées</strong><small>Entre 1 et 32 téléchargements parallèles.</small></span><div className="number-control"><input type="range" min={1} max={32} value={form.downloadConcurrency} onChange={(event) => patch("downloadConcurrency", Number(event.target.value))} /><strong>{form.downloadConcurrency}</strong></div></label></div></section>
    <section className="settings-section panel"><header><KeyRound /><div><h2>Authentification Microsoft</h2><p>Identifiant public de votre application Microsoft Entra.</p></div></header><div className="settings-body"><label className="field"><span>Application (client) ID</span><input value={form.microsoftClientId} onChange={(event) => patch("microsoftClientId", event.target.value)} placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx" /><small>Activez « Autoriser le flux de code d’appareil » pour cette application publique. Aucun secret client n’est stocké.</small></label></div></section>
    <section className="settings-section panel"><header><RefreshCw /><div><h2>Mises à jour du launcher</h2><p>Packages vérifiés par signature avant installation.</p></div></header><div className="settings-body"><Toggle checked={form.autoUpdate} onChange={(value) => patch("autoUpdate", value)} label="Rechercher les nouvelles versions au démarrage" /><div className="setting-row"><span><strong>Canal stable</strong><small>{updateStatus || "Vérifiez manuellement les Releases GitHub."}</small></span><Button variant="secondary" onClick={() => void checkUpdate()}><RefreshCw size={16} />Vérifier</Button></div>{updateProgress !== undefined ? <ProgressBar value={updateProgress} label="Mise à jour" /> : null}</div></section>
    <section className="settings-section panel about-section"><header><MonitorCog /><div><h2>À propos</h2><p>XalLauncher {import.meta.env.VITE_APP_VERSION ?? "0.1.0"} · Windows</p></div></header><div className="settings-body"><p>Développé par <strong>Xaltar Studio</strong> avec Tauri, React et Rust. Minecraft est une marque de Mojang Synergies AB. XalLauncher n’est pas affilié à Mojang ou Microsoft.</p></div></section>
  </div>;
}
