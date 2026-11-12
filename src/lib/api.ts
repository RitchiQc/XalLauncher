import { invoke } from "@tauri-apps/api/core";
import type { Account, AppSettings, BackupEntry, ContentEntry, CrashReport, ExportOptions, ExportReport, ImportReport, ImportSummary, Instance, InstanceInput, LaunchResult, Loader, LoaderVersion, MinecraftVersion, ModrinthVersion, ProjectHit, SearchIndex, ServerEntry, ServerStatus, StorageInfo } from "@/types";

export const api = {
  settings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => invoke<AppSettings>("update_settings", { settings }),
  storageInfo: () => invoke<StorageInfo>("storage_info"),
  changeStorageDirectory: (path: string, migrate: boolean) => invoke<StorageInfo>("change_storage_directory", { path, migrate }),
  openStorageFolder: () => invoke<void>("open_storage_folder"),
  restartApp: () => invoke<void>("restart_app"),
  accounts: () => invoke<Account[]>("list_accounts"),
  login: (clientId: string) => invoke<Account>("begin_microsoft_login", { clientId }),
  removeAccount: (accountId: string) => invoke<Account[]>("remove_account", { accountId }),
  refreshAccount: (accountId: string) => invoke<Account>("refresh_account", { accountId }),
  setCape: (accountId: string, capeId: string | null) => invoke<Account>("set_active_cape", { accountId, capeId }),
  versions: () => invoke<MinecraftVersion[]>("list_versions"),
  loaderVersions: (loader: Loader, versionId: string) => invoke<LoaderVersion[]>("list_loader_versions", { loader, versionId }),
  instances: () => invoke<Instance[]>("list_instances"),
  createInstance: (input: InstanceInput) => invoke<Instance>("create_instance", { input }),
  updateInstance: (id: string, input: InstanceInput) => invoke<Instance>("update_instance", { id, input }),
  duplicateInstance: (id: string) => invoke<Instance>("duplicate_instance", { id }),
  deleteInstance: (id: string, deleteFiles: boolean) => invoke<Instance[]>("delete_instance", { id, deleteFiles }),
  openFolder: (id: string) => invoke<void>("open_instance_folder", { id }),
  setInstanceIcon: (id: string, sourcePath: string | null) => invoke<Instance>("set_instance_icon", { id, sourcePath }),
  instanceIconPath: (id: string) => invoke<string | null>("instance_icon_path", { id }),
  launch: (id: string, accountId: string, serverOverride?: string) => invoke<LaunchResult>("launch_instance", { id, accountId, serverOverride }),
  stop: (id: string) => invoke<void>("stop_game", { id }),
  running: () => invoke<string[]>("running_games"),
  log: (id: string) => invoke<string>("read_instance_log", { id }),
  search: (query: string, projectType: string, versionId?: string, loader?: Loader, offset = 0, index: SearchIndex = "relevance") => invoke<ProjectHit[]>("search_modrinth", { query, projectType, versionId, loader, offset, index }),
  diagnoseCrash: (instanceId: string) => invoke<CrashReport>("diagnose_crash", { instanceId }),
  projectVersions: (projectId: string, instanceId: string) => invoke<ModrinthVersion[]>("list_modrinth_versions", { projectId, instanceId }),
  installProject: (projectId: string, instanceId: string, versionId: string) => invoke<ContentEntry>("install_modrinth_project", { projectId, instanceId, versionId }),
  installModpack: (projectId: string) => invoke<Instance>("install_modrinth_modpack", { projectId }),
  importModpack: (path: string, name?: string) => invoke<Instance>("import_modrinth_pack", { path, name }),
  importInstance: (path: string, name?: string) => invoke<ImportReport>("import_instance", { path, name }),
  importInstanceFolder: (path: string, name?: string) => invoke<ImportReport>("import_instance_folder", { path, name }),
  exportInstance: (id: string, destination: string, options: ExportOptions) => invoke<ExportReport>("export_instance", { id, destination, options }),
  content: (instanceId: string, kind: string) => invoke<ContentEntry[]>("list_content", { instanceId, kind }),
  toggleContent: (instanceId: string, kind: string, fileName: string) => invoke<ContentEntry>("toggle_content", { instanceId, kind, fileName }),
  deleteContent: (instanceId: string, kind: string, fileName: string) => invoke<void>("delete_content", { instanceId, kind, fileName }),
  importContent: (instanceId: string, kind: string, sourcePath: string) => invoke<ContentEntry>("import_content", { instanceId, kind, sourcePath }),
  importContents: (instanceId: string, kind: string, sourcePaths: string[]) => invoke<ImportSummary>("import_contents", { instanceId, kind, sourcePaths }),
  contentPath: (instanceId: string, kind: string, fileName: string) => invoke<string>("content_file_path", { instanceId, kind, fileName }),
  servers: () => invoke<ServerEntry[]>("list_servers"),
  saveServer: (server: ServerEntry) => invoke<ServerEntry>("upsert_server", { server }),
  deleteServer: (id: string) => invoke<ServerEntry[]>("delete_server", { id }),
  queryServer: (server: ServerEntry) => invoke<ServerStatus>("query_server", { server }),
  backups: (instanceId: string) => invoke<BackupEntry[]>("list_backups", { instanceId }),
  createBackup: (instanceId: string, name?: string) => invoke<BackupEntry>("create_backup", { instanceId, name }),
  restoreBackup: (backupId: string) => invoke<void>("restore_backup", { backupId }),
  deleteBackup: (backupId: string) => invoke<void>("delete_backup", { backupId }),
};

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Une erreur inattendue est survenue";
}
