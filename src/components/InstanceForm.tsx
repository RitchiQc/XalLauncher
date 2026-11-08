import { useEffect, useState } from "react";
import { LoaderCircle } from "lucide-react";
import { api, errorMessage } from "@/lib/api";
import { loaderLabels } from "@/lib/format";
import { useLauncher } from "@/state/LauncherContext";
import type { Instance, InstanceInput, Loader, LoaderVersion, MinecraftVersion } from "@/types";
import { Button, Modal, Toggle } from "./UI";

const loaders: Loader[] = ["vanilla", "fabric", "forge", "neoforge", "quilt"];
const colors = ["#8b5cf6", "#06b6d4", "#22c55e", "#f59e0b", "#ef4444", "#ec4899"];

function inputFrom(instance?: Instance): InstanceInput {
  return instance ? {
    name: instance.name, versionId: instance.versionId, loader: instance.loader, loaderVersion: instance.loaderVersion,
    color: instance.color, memoryMin: instance.memoryMin, memoryMax: instance.memoryMax,
    resolutionWidth: instance.resolutionWidth, resolutionHeight: instance.resolutionHeight,
    fullscreen: instance.fullscreen, closeOnLaunch: instance.closeOnLaunch, javaPath: instance.javaPath,
    jvmArguments: instance.jvmArguments, serverAddress: instance.serverAddress,
  } : {
    name: "", versionId: "", loader: "vanilla", color: colors[0], memoryMin: 512, memoryMax: 4096,
    resolutionWidth: 1280, resolutionHeight: 720, fullscreen: false, closeOnLaunch: false, jvmArguments: "",
  };
}

export function InstanceForm({ existing, onClose, onSaved }: { existing?: Instance; onClose: () => void; onSaved: (instance: Instance) => void }) {
  const { perform } = useLauncher();
  const [input, setInput] = useState(() => inputFrom(existing));
  const [versions, setVersions] = useState<MinecraftVersion[]>([]);
  const [loaderVersions, setLoaderVersions] = useState<LoaderVersion[]>([]);
  const [snapshots, setSnapshots] = useState(false);
  const [loading, setLoading] = useState(true);
  const [versionError, setVersionError] = useState("");
  const [versionsAttempt, setVersionsAttempt] = useState(0);
  const [loaderLoading, setLoaderLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const patch = <K extends keyof InstanceInput>(key: K, value: InstanceInput[K]) => setInput((current) => ({ ...current, [key]: value }));

  useEffect(() => {
    let active = true;
    setLoading(true);
    setVersionError("");
    void api.versions().then((items) => {
      if (!active) return;
      setVersions(items);
      setInput((current) => current.versionId ? current : { ...current, versionId: items.find((item) => item.kind === "release")?.id ?? items[0]?.id ?? "" });
    }).catch((error) => {
      if (!active) return;
      setVersions([]);
      setVersionError(errorMessage(error));
    }).finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [versionsAttempt]);

  useEffect(() => {
    if (!input.versionId || input.loader === "vanilla") { setLoaderVersions([]); return; }
    setLoaderLoading(true);
    void api.loaderVersions(input.loader, input.versionId)
      .then((items) => {
        setLoaderVersions(items);
        if (!items.some((item) => item.version === input.loaderVersion)) patch("loaderVersion", items.find((item) => item.stable)?.version ?? items[0]?.version);
      })
      .catch(() => setLoaderVersions([]))
      .finally(() => setLoaderLoading(false));
  }, [input.loader, input.versionId]);

  async function submit() {
    setSaving(true);
    const result = await perform(() => existing ? api.updateInstance(existing.id, input) : api.createInstance(input), existing ? "Instance mise à jour" : "Instance créée");
    setSaving(false);
    if (result) onSaved(result);
  }

  return <Modal title={existing ? "Modifier l’instance" : "Nouvelle instance"} description="Chaque installation conserve ses propres mods, mondes et réglages." onClose={onClose} wide>
    <div className="form-grid">
      <label className="field field-full"><span>Nom de l’instance</span><input autoFocus value={input.name} onChange={(event) => patch("name", event.target.value)} placeholder="Mon aventure" maxLength={48} /></label>
      <label className="field"><span>Minecraft</span><div className="select-with-icon"><select value={input.versionId} onChange={(event) => patch("versionId", event.target.value)} disabled={loading || !versions.length}>
        {loading ? <option value="">Chargement des versions…</option> : null}
        {!loading && !versions.length ? <option value="">Aucune version disponible</option> : null}
        {versions.filter((version) => snapshots || version.kind === "release").map((version) => <option key={version.id} value={version.id}>{version.id}{version.kind === "snapshot" ? " · Snapshot" : ""}</option>)}
      </select>{loading ? <LoaderCircle className="spin" size={16} /> : null}</div></label>
      <label className="field"><span>Modloader</span><select value={input.loader} onChange={(event) => patch("loader", event.target.value as Loader)}>{loaders.map((loader) => <option value={loader} key={loader}>{loaderLabels[loader]}</option>)}</select></label>
      {versionError ? <div className="form-error field-full" role="alert"><span>Impossible de charger les versions Minecraft. {versionError}</span><Button type="button" variant="secondary" onClick={() => setVersionsAttempt((current) => current + 1)}>Réessayer</Button></div> : null}
      {input.loader !== "vanilla" ? <label className="field field-full"><span>Version de {loaderLabels[input.loader]}</span><div className="select-with-icon"><select value={input.loaderVersion ?? ""} onChange={(event) => patch("loaderVersion", event.target.value)} disabled={loaderLoading || !loaderVersions.length}>
        <option value="">{loaderLoading ? "Chargement des versions…" : loaderVersions.length ? "Sélectionner" : "Aucune version compatible"}</option>
        {loaderVersions.map((version) => <option value={version.version} key={version.version}>{version.version}{version.stable ? " · Stable" : " · Expérimentale"}</option>)}
      </select>{loaderLoading ? <LoaderCircle className="spin" size={16} /> : null}</div></label> : null}
      <div className="field field-full"><span>Couleur de l’instance</span><div className="color-picker">{colors.map((color) => <button type="button" key={color} className={input.color === color ? "selected" : ""} style={{ background: color }} onClick={() => patch("color", color)} aria-label={`Couleur ${color}`} />)}</div></div>
      <label className="field"><span>Mémoire maximale · {(input.memoryMax / 1024).toFixed(1)} Go</span><input type="range" min={1024} max={16384} step={512} value={input.memoryMax} onChange={(event) => patch("memoryMax", Number(event.target.value))} /></label>
      <label className="field"><span>Serveur au lancement (facultatif)</span><input value={input.serverAddress ?? ""} onChange={(event) => patch("serverAddress", event.target.value || undefined)} placeholder="play.exemple.fr:25565" /></label>
      <div className="field-full inline-options"><Toggle checked={snapshots} onChange={setSnapshots} label="Afficher les snapshots" /><Toggle checked={input.fullscreen} onChange={(value) => patch("fullscreen", value)} label="Plein écran" /><Toggle checked={input.closeOnLaunch} onChange={(value) => patch("closeOnLaunch", value)} label="Masquer le launcher en jeu" /></div>
      <details className="advanced field-full"><summary>Réglages avancés</summary><div className="form-grid">
        <label className="field"><span>Résolution largeur</span><input type="number" min={640} value={input.resolutionWidth} onChange={(event) => patch("resolutionWidth", Number(event.target.value))} /></label>
        <label className="field"><span>Résolution hauteur</span><input type="number" min={360} value={input.resolutionHeight} onChange={(event) => patch("resolutionHeight", Number(event.target.value))} /></label>
        <label className="field field-full"><span>Arguments JVM supplémentaires</span><input value={input.jvmArguments} onChange={(event) => patch("jvmArguments", event.target.value)} placeholder="-XX:+UseG1GC" /></label>
      </div></details>
    </div>
    <footer className="modal-footer"><Button variant="secondary" onClick={onClose}>Annuler</Button><Button onClick={() => void submit()} loading={saving} disabled={!input.name.trim() || !input.versionId || (input.loader !== "vanilla" && !input.loaderVersion)}>{existing ? "Enregistrer" : "Créer l’instance"}</Button></footer>
  </Modal>;
}
