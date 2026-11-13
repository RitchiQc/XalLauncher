import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowLeft, Check, Copy, ExternalLink, Plus, RefreshCw, ShieldCheck, Trash2, UserRound } from "lucide-react";
import { CapeThumbnail } from "@/components/CapeThumbnail";
import { PlayerHead } from "@/components/PlayerHead";
import { SkinPreview } from "@/components/SkinPreview";
import { Button, EmptyState, Modal, PageHeader } from "@/components/UI";
import { api } from "@/lib/api";
import { confirmAction } from "@/lib/dialog";
import { useLauncher } from "@/state/LauncherContext";
import type { LoginMode } from "@/types";

export function AccountsPage() {
  const { accounts, activeAccount, activeAccountId, setActiveAccountId, settings, setSettings, refreshAccounts, perform, deviceCode, clearDeviceCode } = useLauncher();
  const [clientId, setClientId] = useState(settings.microsoftClientId);
  const [adding, setAdding] = useState(false);
  const [mode, setMode] = useState<LoginMode>("device");
  const [busy, setBusy] = useState(false);
  const [capeBusy, setCapeBusy] = useState(false);

  function openLogin() {
    setClientId(settings.microsoftClientId);
    setMode("device");
    clearDeviceCode();
    setAdding(true);
  }
  function changeMode(next: LoginMode) {
    clearDeviceCode();
    setMode(next);
  }
  async function login() {
    setBusy(true);
    let saved = settings;
    if (clientId !== settings.microsoftClientId) {
      const result = await perform(() => api.saveSettings({ ...settings, microsoftClientId: clientId }));
      if (result) { saved = result; setSettings(result); } else { setBusy(false); return; }
    }
    const account = await perform(() => api.login(saved.microsoftClientId, mode), "Compte Microsoft connecté");
    clearDeviceCode(); setBusy(false);
    if (account) { await refreshAccounts(); setActiveAccountId(account.id); setAdding(false); }
  }
  async function cancelLogin() {
    clearDeviceCode();
    setBusy(false);
    await api.cancelLogin().catch(() => undefined);
  }
  async function remove(id: string) {
    if (!(await confirmAction("Déconnecter ce compte de XalLauncher ?", "Déconnexion"))) return;
    const result = await perform(() => api.removeAccount(id), "Compte déconnecté");
    if (result) await refreshAccounts();
  }
  async function refresh(id: string) {
    const result = await perform(() => api.refreshAccount(id), "Profil actualisé");
    if (result) await refreshAccounts();
  }
  async function selectCape(capeId: string | null) {
    if (!activeAccount) return;
    setCapeBusy(true); const result = await perform(() => api.setCape(activeAccount.id, capeId), "Cape officielle mise à jour"); setCapeBusy(false);
    if (result) await refreshAccounts();
  }

  return <div className="page"><PageHeader eyebrow="Identité Minecraft" title="Comptes & capes" description="Connexion Microsoft officielle et gestion des capes que vous possédez." actions={<Button onClick={openLogin}><Plus size={18} />Ajouter un compte</Button>} />
    {accounts.length ? <><div className="account-grid">{accounts.map((account) => <div key={account.id} role="button" tabIndex={0} className={account.id === activeAccountId ? "account-card selected" : "account-card"} onClick={() => setActiveAccountId(account.id)}><PlayerHead skinUrl={account.skinUrl} uuid={account.id} name={account.name} /><span><strong>{account.name}</strong><small><ShieldCheck size={13} />Compte Microsoft vérifié</small></span>{account.id === activeAccountId ? <Check className="account-check" size={18} /> : null}<span className="account-card-actions"><Button variant="ghost" className="icon-button" onClick={(event) => { event.stopPropagation(); void refresh(account.id); }}><RefreshCw size={16} /></Button><Button variant="ghost" className="icon-button danger-icon" onClick={(event) => { event.stopPropagation(); void remove(account.id); }}><Trash2 size={16} /></Button></span></div>)}</div>
      {activeAccount ? <section className="cape-section panel"><div className="cape-preview"><SkinPreview skinUrl={activeAccount.skinUrl} capeUrl={activeAccount.capes.find((cape) => cape.state === "ACTIVE")?.url} model={activeAccount.skinVariant === "slim" ? "slim" : undefined} /><div><span className="eyebrow">Garde-robe officielle</span><h2>{activeAccount.name}</h2><p>Seules les capes rattachées à votre compte Minecraft peuvent être activées. Les changements sont appliqués à votre profil officiel.</p><Button variant="secondary" disabled={capeBusy || !activeAccount.capes.some((cape) => cape.state === "ACTIVE")} onClick={() => void selectCape(null)}>Masquer la cape actuelle</Button></div></div><div className="cape-list">{activeAccount.capes.length ? activeAccount.capes.map((cape) => <button className={cape.state === "ACTIVE" ? "cape-card active" : "cape-card"} key={cape.id} disabled={capeBusy} onClick={() => void selectCape(cape.id)}><div><CapeThumbnail url={cape.url} alias={cape.alias} /></div><span><strong>{cape.alias || "Cape Minecraft"}</strong><small>{cape.state === "ACTIVE" ? "Activée" : "Possédée"}</small></span>{cape.state === "ACTIVE" ? <Check size={18} /> : null}</button>) : <EmptyState icon={<UserRound />} title="Aucune cape sur ce compte" description="Les capes officielles obtenues sur ce compte apparaîtront ici." />}</div></section> : null}
    </> : <EmptyState icon={<UserRound size={32} />} title="Aucun compte connecté" description="Utilisez votre compte Microsoft possédant Minecraft: Java Edition. Vos jetons sont conservés dans le Coffre d’identification Windows." action={<Button onClick={openLogin}><Plus size={17} />Connexion Microsoft</Button>} />}
    {adding ? <Modal
      title="Connexion Microsoft"
      description={mode === "browser" ? "Le navigateur va s’ouvrir sur la page officielle Microsoft pour choisir le compte qui possède Minecraft." : "XalLauncher utilise le flux de code d’appareil : aucun mot de passe n’est demandé dans l’application."}
      onClose={() => { if (!busy) { setAdding(false); setMode("device"); clearDeviceCode(); } }}
    >
      <label className="field field-full"><span>Identifiant d’application Microsoft Entra</span><input value={clientId} onChange={(event) => setClientId(event.target.value)} placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx" disabled={busy} /><small>Créez une application publique dans Microsoft Entra et activez le flux de code d’appareil. Le mode navigateur nécessite aussi l’URI <code>http://localhost</code>. Cet identifiant n’est pas un secret.</small></label>
      {mode === "device" ? <>
        {deviceCode ? <div className="device-code">
          <span>Sur votre appareil, ouvrez le lien puis saisissez :</span>
          <strong>{deviceCode.userCode}</strong>
          <div>
            <Button variant="secondary" onClick={() => void navigator.clipboard.writeText(deviceCode.userCode)}><Copy size={16} />Copier le code</Button>
            <Button onClick={() => void openUrl(deviceCode.verificationUri)}><ExternalLink size={16} />Ouvrir Microsoft</Button>
            <Button variant="secondary" onClick={() => void api.openPrivate(deviceCode.verificationUri).catch(() => undefined)}><ExternalLink size={16} />Navigation privée</Button>
          </div>
          <small>En attente de votre confirmation… Si le navigateur utilise le mauvais compte, ouvrez le lien en navigation privée ou choisissez « Utiliser un autre compte Microsoft » ci-dessous.</small>
        </div> : null}
        <div className="auth-alternative">
          <span>Vous devez utiliser un autre compte Microsoft ?</span>
          <Button variant="secondary" disabled={busy} onClick={() => changeMode("browser")}><UserRound size={16} />Utiliser un autre compte Microsoft</Button>
        </div>
      </> : <div className="auth-alternative auth-browser">
        <p>La page de connexion Microsoft s’ouvre dans votre navigateur. Choisissez « Utiliser un autre compte » pour saisir l’email qui possède Minecraft.</p>
        <small>Aucun email ni mot de passe n’est demandé par XalLauncher. Si la page affiche une erreur AADSTS50011, l’URI http://localhost n’est pas encore autorisée dans l’application Entra : revenez au code d’appareil.</small>
        <Button variant="ghost" disabled={busy} onClick={() => changeMode("device")}><ArrowLeft size={16} />Revenir au code d’appareil</Button>
      </div>}
      <footer className="modal-footer">
        {busy ? <Button variant="secondary" onClick={() => void cancelLogin()}>Annuler</Button> : <Button variant="secondary" onClick={() => { setAdding(false); setMode("device"); clearDeviceCode(); }}>Fermer</Button>}
        {mode === "browser"
          ? <Button onClick={() => void login()} loading={busy} disabled={!clientId.trim()}>{busy ? "En attente du navigateur…" : "Ouvrir la page Microsoft"}</Button>
          : <Button onClick={() => void login()} loading={busy} disabled={!clientId.trim()}>{deviceCode ? "Connexion en cours…" : "Continuer avec Microsoft"}</Button>}
      </footer>
    </Modal> : null}
  </div>;
}

