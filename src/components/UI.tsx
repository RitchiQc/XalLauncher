import type { ButtonHTMLAttributes, ReactNode } from "react";
import { AlertCircle, Check, Info, LoaderCircle, X } from "lucide-react";
import { BrandMark } from "@/components/BrandMark";
import { useLauncher } from "@/state/LauncherContext";

export function Button({ children, variant = "primary", loading, className = "", disabled, ...props }: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  loading?: boolean;
}) {
  return <button className={`button button-${variant} ${className}`} disabled={disabled || loading} {...props}>
    {loading ? <LoaderCircle className="spin" size={17} /> : null}{children}
  </button>;
}

export function PageHeader({ eyebrow, title, description, actions }: { eyebrow?: string; title: string; description?: string; actions?: ReactNode }) {
  return <header className="page-header">
    <div>{eyebrow ? <span className="eyebrow">{eyebrow}</span> : null}<h1>{title}</h1>{description ? <p>{description}</p> : null}</div>
    {actions ? <div className="page-actions">{actions}</div> : null}
  </header>;
}

export function EmptyState({ icon, title, description, action }: { icon: ReactNode; title: string; description: string; action?: ReactNode }) {
  return <div className="empty-state"><div className="empty-icon">{icon}</div><h3>{title}</h3><p>{description}</p>{action}</div>;
}

export function Modal({ title, description, children, onClose, wide = false }: { title: string; description?: string; children: ReactNode; onClose: () => void; wide?: boolean }) {
  return <div className="modal-backdrop" role="presentation" onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
    <section className={`modal ${wide ? "modal-wide" : ""}`} role="dialog" aria-modal="true" aria-label={title}>
      <header><div><h2>{title}</h2>{description ? <p>{description}</p> : null}</div><Button variant="ghost" className="icon-button" onClick={onClose} aria-label="Fermer"><X size={20} /></Button></header>
      {children}
    </section>
  </div>;
}

export function ProgressBar({ value, label }: { value: number; label?: string }) {
  const percent = value <= 0 ? 8 : Math.min(100, Math.max(0, value));
  return <div className="progress-wrap">{label ? <div className="progress-label">{label}<span>{Math.round(value)}%</span></div> : null}<div className="progress-track"><div style={{ width: `${percent}%` }} /></div></div>;
}

export function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (checked: boolean) => void; label: string }) {
  return <label className="toggle-row"><span>{label}</span><input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} /><span className="toggle" aria-hidden="true"><span /></span></label>;
}

export function Toasts() {
  const { toasts, dismissToast } = useLauncher();
  return <div className="toasts" aria-live="polite">{toasts.map((toast) => <div className={`toast toast-${toast.kind}`} key={toast.id}>
    {toast.kind === "success" ? <Check size={18} /> : toast.kind === "error" ? <AlertCircle size={18} /> : <Info size={18} />}
    <span>{toast.message}</span><button onClick={() => dismissToast(toast.id)} aria-label="Fermer"><X size={15} /></button>
  </div>)}</div>;
}

export function LoadingScreen() {
  return <div className="loading-screen"><BrandMark large /><LoaderCircle className="spin" size={26} /><span>Préparation de XalLauncher…</span></div>;
}
