import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, errorMessage } from "@/lib/api";
import { findUpdate } from "@/lib/updates";
import type { Account, AppSettings, DeviceCodeEvent, GameLog, Instance, ProgressEvent, ServerEntry } from "@/types";

const defaultSettings: AppSettings = {
  language: "fr", theme: "dark", accent: "#8b5cf6", reducedMotion: false,
  downloadConcurrency: 8, microsoftClientId: "", discordEnabled: false, autoUpdate: true,
};

type Toast = { id: number; message: string; kind: "success" | "error" | "info" };
type LauncherContextValue = {
  ready: boolean;
  settings: AppSettings;
  accounts: Account[];
  instances: Instance[];
  servers: ServerEntry[];
  activeAccount?: Account;
  activeInstance?: Instance;
  activeAccountId: string;
  activeInstanceId: string;
  running: Set<string>;
  progress?: ProgressEvent;
  deviceCode?: DeviceCodeEvent;
  logs: GameLog[];
  toasts: Toast[];
  setActiveAccountId: (id: string) => void;
  setActiveInstanceId: (id: string) => void;
  setSettings: (settings: AppSettings) => void;
  refreshAccounts: () => Promise<void>;
  refreshInstances: () => Promise<void>;
  refreshServers: () => Promise<void>;
  startGame: (instanceId: string, serverAddress?: string) => Promise<boolean>;
  stopGame: (instanceId: string) => Promise<void>;
  perform: <T>(action: () => Promise<T>, success?: string) => Promise<T | undefined>;
  notify: (message: string, kind?: Toast["kind"]) => void;
  dismissToast: (id: number) => void;
  clearDeviceCode: () => void;
};

const LauncherContext = createContext<LauncherContextValue | null>(null);

export function LauncherProvider({ children }: { children: ReactNode }) {
  const [ready, setReady] = useState(false);
  const [settings, setSettings] = useState(defaultSettings);
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [instances, setInstances] = useState<Instance[]>([]);
  const [servers, setServers] = useState<ServerEntry[]>([]);
  const [activeAccountId, setActiveAccount] = useState(() => localStorage.getItem("xal-account") ?? "");
  const [activeInstanceId, setActiveInstance] = useState(() => localStorage.getItem("xal-instance") ?? "");
  const [running, setRunning] = useState(new Set<string>());
  const [progress, setProgress] = useState<ProgressEvent>();
  const [deviceCode, setDeviceCode] = useState<DeviceCodeEvent>();
  const [logs, setLogs] = useState<GameLog[]>([]);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const updateChecked = useRef(false);

  const dismissToast = useCallback((id: number) => setToasts((items) => items.filter((item) => item.id !== id)), []);
  const notify = useCallback((message: string, kind: Toast["kind"] = "info") => {
    const id = Date.now() + Math.random();
    setToasts((items) => [...items.slice(-3), { id, message, kind }]);
    window.setTimeout(() => dismissToast(id), kind === "error" ? 9000 : 4500);
  }, [dismissToast]);

  const refreshAccounts = useCallback(async () => setAccounts(await api.accounts()), []);
  const refreshInstances = useCallback(async () => setInstances(await api.instances()), []);
  const refreshServers = useCallback(async () => setServers(await api.servers()), []);

  useEffect(() => {
    Promise.all([api.settings(), api.accounts(), api.instances(), api.servers(), api.running()])
      .then(([loadedSettings, loadedAccounts, loadedInstances, loadedServers, runningIds]) => {
        setSettings(loadedSettings); setAccounts(loadedAccounts); setInstances(loadedInstances);
        setServers(loadedServers); setRunning(new Set(runningIds));
        if (loadedAccounts[0]) setActiveAccount((current) => loadedAccounts.some((account) => account.id === current) ? current : loadedAccounts[0].id);
        if (loadedInstances[0]) setActiveInstance((current) => loadedInstances.some((instance) => instance.id === current) ? current : loadedInstances[0].id);
      })
      .catch((error) => notify(errorMessage(error), "error"))
      .finally(() => setReady(true));
  }, [notify]);

  useEffect(() => {
    const listeners = [
      listen<ProgressEvent>("download-progress", (event) => {
        setProgress(event.payload);
        if (event.payload.total > 0 && event.payload.current >= event.payload.total) {
          window.setTimeout(() => setProgress(undefined), 1400);
        }
      }),
      listen<DeviceCodeEvent>("auth-device-code", (event) => setDeviceCode(event.payload)),
      listen<GameLog>("game-log", (event) => setLogs((items) => [...items.slice(-1499), event.payload])),
      listen<{ instanceId: string }>("game-exit", (event) => {
        setRunning((items) => { const next = new Set(items); next.delete(event.payload.instanceId); return next; });
        void refreshInstances();
      }),
    ];
    return () => { listeners.forEach((listener) => void listener.then((unlisten) => unlisten())); };
  }, [refreshInstances]);

  useEffect(() => {
    document.documentElement.dataset.theme = settings.theme;
    document.documentElement.style.setProperty("--accent", settings.accent);
    document.documentElement.classList.toggle("reduce-motion", settings.reducedMotion);
  }, [settings]);

  useEffect(() => {
    if (!ready || !settings.autoUpdate || updateChecked.current) return;
    updateChecked.current = true;
    void findUpdate().then((update) => {
      if (update) notify(`XalLauncher ${update.version} est disponible dans les paramètres`, "info");
    }).catch(() => undefined);
  }, [ready, settings.autoUpdate, notify]);

  const setActiveAccountId = useCallback((id: string) => { localStorage.setItem("xal-account", id); setActiveAccount(id); }, []);
  const setActiveInstanceId = useCallback((id: string) => { localStorage.setItem("xal-instance", id); setActiveInstance(id); }, []);
  const perform = useCallback(async <T,>(action: () => Promise<T>, success?: string): Promise<T | undefined> => {
    try {
      const result = await action();
      if (success) notify(success, "success");
      return result;
    } catch (error) { notify(errorMessage(error), "error"); return undefined; }
  }, [notify]);

  const startGame = useCallback(async (instanceId: string, serverAddress?: string): Promise<boolean> => {
    if (!activeAccountId) { notify("Connectez un compte Microsoft avant de jouer", "error"); return false; }
    try {
      await api.launch(instanceId, activeAccountId, serverAddress);
      setRunning((items) => new Set(items).add(instanceId));
      return true;
    } catch (error) { notify(errorMessage(error), "error"); return false; }
  }, [activeAccountId, notify]);
  const stopGame = useCallback(async (instanceId: string) => {
    try { await api.stop(instanceId); } catch (error) { notify(errorMessage(error), "error"); }
  }, [notify]);

  const value = useMemo<LauncherContextValue>(() => ({
    ready, settings, accounts, instances, servers,
    activeAccount: accounts.find((account) => account.id === activeAccountId),
    activeInstance: instances.find((instance) => instance.id === activeInstanceId),
    activeAccountId, activeInstanceId, running, progress, deviceCode, logs, toasts,
    setActiveAccountId, setActiveInstanceId, setSettings, refreshAccounts, refreshInstances, refreshServers, startGame, stopGame,
    perform, notify, dismissToast, clearDeviceCode: () => setDeviceCode(undefined),
  }), [ready, settings, accounts, instances, servers, activeAccountId, activeInstanceId, running, progress, deviceCode, logs, toasts,
    setActiveAccountId, setActiveInstanceId, refreshAccounts, refreshInstances, refreshServers, startGame, stopGame, perform, notify, dismissToast]);

  return <LauncherContext.Provider value={value}>{children}</LauncherContext.Provider>;
}

export function useLauncher(): LauncherContextValue {
  const context = useContext(LauncherContext);
  if (!context) throw new Error("useLauncher doit être utilisé dans LauncherProvider");
  return context;
}
