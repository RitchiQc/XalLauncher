import { useEffect, useState, type ReactNode } from "react";
import { Gauge, Save, ShieldAlert, Sparkles } from "lucide-react";
import { Toggle } from "@/components/UI";
import { api } from "@/lib/api";
import { useLauncher } from "@/state/LauncherContext";
import { defaultModules } from "@/types";
import type { Instance, InstanceInput, ModuleState } from "@/types";

const performanceOptions: { key: keyof ModuleState; label: string }[] = [
  { key: "performanceJvm", label: "Arguments JVM optimisés (Aikar)" },
  { key: "performanceOptions", label: "Presets graphiques dans options.txt" },
  { key: "performancePriority", label: "Priorité Windows élevée" },
  { key: "performanceEcoqos", label: "Désactiver EcoQoS / throttling" },
  { key: "performanceAffinity", label: "Affinité P-cores (CPU hybrides)" },
];

function inputFrom(instance: Instance, modules: ModuleState): InstanceInput {
  return {
    name: instance.name, versionId: instance.versionId, loader: instance.loader, loaderVersion: instance.loaderVersion,
    color: instance.color, memoryMin: instance.memoryMin, memoryMax: instance.memoryMax,
    resolutionWidth: instance.resolutionWidth, resolutionHeight: instance.resolutionHeight,
    fullscreen: instance.fullscreen, closeOnLaunch: instance.closeOnLaunch, javaPath: instance.javaPath,
    jvmArguments: instance.jvmArguments, serverAddress: instance.serverAddress, modules,
  };
}

function ModuleCard({ icon, title, description, enabled, onToggle, children }: {
  icon: ReactNode; title: string; description: string; enabled: boolean;
  onToggle: (value: boolean) => void; children?: ReactNode;
}) {
  return <section className={enabled ? "module-card enabled" : "module-card"}>
    <header>{icon}<div><h3>{title}</h3><p>{description}</p></div><Toggle checked={enabled} onChange={onToggle} label="Activer" /></header>
    {children}
  </section>;
}

export function ModulesPanel({ instance, onSaved }: { instance: Instance; onSaved: () => void }) {
  const { perform } = useLauncher();
  const [modules, setModules] = useState<ModuleState>(instance.modules ?? defaultModules);

  useEffect(() => { setModules(instance.modules ?? defaultModules); }, [instance.modules]);

  async function update(patch: Partial<ModuleState>) {
    const previous = modules;
    const next = { ...modules, ...patch };
    setModules(next);
    const result = await perform(() => api.updateInstance(instance.id, inputFrom(instance, next)), "Modules mis à jour");
    if (result) onSaved(); else setModules(previous);
  }

  return <section className="content-section">
    <div className="section-toolbar"><div><h3>Modules de l’instance</h3><span className="muted">Appliqués au prochain lancement, aucun mod n’est installé automatiquement.</span></div><Save size={17} /></div>
    <div className="modules-grid">
      <ModuleCard icon={<Gauge />} title="Performance" description="Optimise Java et les réglages du jeu sans modifier les fichiers du modloader." enabled={modules.performance} onToggle={(value) => void update(value ? { performance: true, performanceJvm: true, performanceOptions: true } : { performance: false })}>
        <div className={modules.performance ? "module-options" : "module-options off"}>
          {performanceOptions.map((option) => <Toggle key={option.key} checked={Boolean(modules[option.key])} onChange={(value) => void update({ [option.key]: value })} label={option.label} />)}
        </div>
        <span className="module-note">options.txt est sauvegardé puis restauré automatiquement quand le module est désactivé.</span>
      </ModuleCard>

      <ModuleCard icon={<Sparkles />} title="Branding XalLauncher" description="Panorama nébuleuse HD pré-calculé, splashs, icône de pack et titre de fenêtre XalLauncher." enabled={modules.branding} onToggle={(value) => void update({ branding: value })}>
        <span className="module-note">Activé par défaut. Désactivable à tout moment ; le pack est retiré des options du jeu.</span>
      </ModuleCard>

      <ModuleCard icon={<ShieldAlert />} title="Diagnostic de crash" description="Analyse automatiquement le journal après un arrêt anormal et propose des pistes." enabled={modules.crashDiagnosis} onToggle={(value) => void update({ crashDiagnosis: value })}>
        <span className="module-note">Passif : ne modifie aucun fichier du jeu.</span>
      </ModuleCard>

      <ModuleCard icon={<Save />} title="Sauvegarde automatique" description="Crée une sauvegarde ZIP des mondes avant chaque lancement." enabled={modules.autoBackup} onToggle={(value) => void update({ autoBackup: value })}>
        <span className="module-note">Les sauvegardes restent disponibles dans l’onglet Sauvegardes.</span>
      </ModuleCard>
    </div>
  </section>;
}
