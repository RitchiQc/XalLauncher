import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { ArrowRight, ChevronDown, CircleStop, Clock3, CloudDownload, Library, Play, Plus, ShieldCheck, Sparkles, UserRound } from "lucide-react";
import { InstanceArtwork } from "@/components/InstanceArtwork";
import { Button, EmptyState, ProgressBar } from "@/components/UI";
import { avatarFallback, avatarUrl, formatDuration, handleAvatarError } from "@/lib/format";
import { useLauncher } from "@/state/LauncherContext";

export function HomePage() {
  const { instances, activeInstance, activeAccount, activeInstanceId, setActiveInstanceId, running, startGame, stopGame, progress } = useLauncher();
  const [launching, setLaunching] = useState(false);
  const navigate = useNavigate();
  const isRunning = activeInstance ? running.has(activeInstance.id) : false;

  async function launch() {
    if (!activeInstance) return;
    setLaunching(true); await startGame(activeInstance.id); setLaunching(false);
  }

  if (!activeInstance) return <div className="page"><EmptyState icon={<Sparkles size={32} />} title="Votre prochaine aventure commence ici" description="Créez une instance Vanilla ou moddée, choisissez votre version et laissez XalLauncher préparer tout le nécessaire." action={<Button onClick={() => navigate("/instances")}><Plus size={18} />Créer ma première instance</Button>} /></div>;

  return <div className="page home-page">
    <section className="hero">
      <InstanceArtwork instance={activeInstance} />
      <div className="hero-play">
        <div className="instance-select-wrap"><select aria-label="Instance active" value={activeInstanceId} onChange={(event) => setActiveInstanceId(event.target.value)}>{instances.map((instance) => <option key={instance.id} value={instance.id}>{instance.name}</option>)}</select><ChevronDown size={16} /></div>
        {isRunning ? <Button variant="danger" className="play-button" onClick={() => void stopGame(activeInstance.id)}><CircleStop size={22} />Arrêter le jeu</Button> : <Button className="play-button" onClick={() => void launch()} loading={launching} disabled={!activeAccount}><Play size={22} fill="currentColor" />Jouer</Button>}
        {progress?.operation === "launch" ? <ProgressBar value={progress.total ? progress.current / progress.total * 100 : 0} label={progress.label} /> : null}
        {!activeAccount ? <button className="account-warning" onClick={() => navigate("/accounts")}><UserRound size={16} />Connectez un compte Microsoft pour jouer<ArrowRight size={15} /></button> : <span className="playing-as"><img src={avatarUrl(activeAccount.id, 32)} data-fallback={avatarFallback(activeAccount.id, 32)} onError={handleAvatarError} alt="" />Vous jouez avec <strong>{activeAccount.name}</strong><ShieldCheck size={15} /></span>}
      </div>
    </section>

    <div className="home-grid">
      <section className="panel recent-panel"><header className="section-heading"><div><span className="eyebrow">Votre bibliothèque</span><h2>Reprendre une aventure</h2></div><Button variant="ghost" onClick={() => navigate("/instances")}>Tout afficher<ArrowRight size={16} /></Button></header>
        <div className="recent-list">{instances.slice(0, 3).map((instance) => <button key={instance.id} className={instance.id === activeInstance.id ? "recent-item active" : "recent-item"} onClick={() => setActiveInstanceId(instance.id)}>
          <span className="recent-color" style={{ background: instance.color }} /><span><strong>{instance.name}</strong><small>{instance.loader} · {instance.versionId}</small></span><span className="recent-time"><Clock3 size={14} />{formatDuration(instance.totalPlaySeconds)}</span><Play size={17} />
        </button>)}</div>
      </section>
      <aside className="panel quick-panel"><span className="eyebrow">Raccourcis</span><h2>À vous de jouer</h2>
        <button onClick={() => navigate("/explore")}><CloudDownload /><span><strong>Découvrir des mods</strong><small>Le catalogue Modrinth dans votre launcher</small></span><ArrowRight /></button>
        <button onClick={() => navigate("/instances")}><Library /><span><strong>Gérer les instances</strong><small>Contenu, mondes, sauvegardes et réglages</small></span><ArrowRight /></button>
      </aside>
    </div>
  </div>;
}
