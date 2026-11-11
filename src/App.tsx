import { NavLink, Navigate, Route, Routes, useLocation, useNavigate } from "react-router-dom";
import { Box, ChevronDown, CloudDownload, Home, Library, Server, Settings, UserRound } from "lucide-react";
import { PlayerHead } from "@/components/PlayerHead";
import { LoadingScreen, ProgressBar, Toasts } from "@/components/UI";
import { loaderLabels } from "@/lib/format";
import { AccountsPage } from "@/pages/AccountsPage";
import { HomePage } from "@/pages/HomePage";
import { InstanceDetailsPage } from "@/pages/InstanceDetailsPage";
import { InstancesPage } from "@/pages/InstancesPage";
import { ServersPage } from "@/pages/ServersPage";
import { SettingsPage } from "@/pages/SettingsPage";
import { useLauncher } from "@/state/LauncherContext";

const navigation = [
  { to: "/", label: "Accueil", icon: Home, end: true },
  { to: "/instances", label: "Instances", icon: Library },
  { to: "/servers", label: "Serveurs", icon: Server },
  { to: "/accounts", label: "Comptes & capes", icon: UserRound },
];

const titles: Record<string, string> = { "/": "Vue d’ensemble", "/instances": "Bibliothèque", "/servers": "Serveurs multijoueurs", "/accounts": "Profils Minecraft", "/settings": "Préférences" };

export default function App() {
  const { ready, accounts, activeAccount, activeAccountId, setActiveAccountId, activeInstance, instances, activeInstanceId, setActiveInstanceId, progress } = useLauncher();
  const location = useLocation();
  const navigate = useNavigate();
  if (!ready) return <LoadingScreen />;
  const title = location.pathname.startsWith("/instances/") ? "Détail de l’instance" : titles[location.pathname] ?? "XalLauncher";

  return <div className="app-shell">
    <aside className="sidebar">
      <button className="brand" onClick={() => navigate("/")}><span className="brand-mark">X</span><span><strong>XalLauncher</strong><small>XALTAR STUDIO</small></span></button>
      <nav className="main-nav">{navigation.map((item) => <NavLink key={item.to} to={item.to} end={item.end} className={({ isActive }) => isActive ? "active" : ""}><item.icon size={19} /><span>{item.label}</span></NavLink>)}</nav>
      <div className="sidebar-bottom">
        {progress ? <div className="download-widget"><CloudDownload size={17} /><div><strong>{progress.label}</strong><ProgressBar value={progress.total ? progress.current / progress.total * 100 : 0} /></div></div> : null}
        {activeInstance ? <button className="sidebar-instance" onClick={() => navigate(`/instances/${activeInstance.id}`)}><span className="instance-dot" style={{ background: activeInstance.color }}><Box size={17} /></span><span><strong>{activeInstance.name}</strong><small>{loaderLabels[activeInstance.loader]} · {activeInstance.versionId}</small></span><ChevronDown size={15} /></button> : null}
        <NavLink to="/settings" className={({ isActive }) => `settings-link ${isActive ? "active" : ""}`}><Settings size={19} /><span>Paramètres</span></NavLink>
        <div className="sidebar-account">{accounts.length ? <>{activeAccount ? <PlayerHead skinUrl={activeAccount.skinUrl} uuid={activeAccount.id} name={activeAccount.name} size={31} /> : null}<select aria-label="Compte actif" value={activeAccountId} onChange={(event) => setActiveAccountId(event.target.value)}>{accounts.map((account) => <option key={account.id} value={account.id}>{account.name}</option>)}</select><ChevronDown size={14} /></> : <button onClick={() => navigate("/accounts")}><UserRound size={18} /><span>Se connecter</span></button>}</div>
      </div>
    </aside>
    <div className="app-main"><header className="topbar"><div><span>XalLauncher</span><strong>/</strong><span>{title}</span></div><div className="topbar-status"><span className="secure-status"><span />Services locaux prêts</span>{activeInstance ? <select value={activeInstanceId} onChange={(event) => setActiveInstanceId(event.target.value)}>{instances.map((instance) => <option value={instance.id} key={instance.id}>{instance.name}</option>)}</select> : null}</div></header>
      <main className="content"><Routes><Route path="/" element={<HomePage />} /><Route path="/instances" element={<InstancesPage />} /><Route path="/instances/:id" element={<InstanceDetailsPage />} /><Route path="/explore" element={<Navigate to="/instances" replace />} /><Route path="/servers" element={<ServersPage />} /><Route path="/accounts" element={<AccountsPage />} /><Route path="/settings" element={<SettingsPage />} /><Route path="*" element={<Navigate to="/" replace />} /></Routes></main>
    </div>
    <Toasts />
  </div>;
}
