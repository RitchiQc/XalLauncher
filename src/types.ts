export type Loader = "vanilla" | "fabric" | "quilt" | "forge" | "neoforge";
export type LoginMode = "device" | "browser";
export type ContentKind = "mod" | "resourcepack" | "shader" | "world";
export type Theme = "dark" | "light" | "space";
export type SearchIndex = "relevance" | "downloads" | "follows" | "newest" | "updated";

export interface ModuleState {
  performance: boolean;
  performanceJvm: boolean;
  performanceOptions: boolean;
  performancePriority: boolean;
  performanceEcoqos: boolean;
  performanceAffinity: boolean;
  autoBackup: boolean;
  branding: boolean;
  crashDiagnosis: boolean;
}

export const defaultModules: ModuleState = {
  performance: false,
  performanceJvm: false,
  performanceOptions: false,
  performancePriority: false,
  performanceEcoqos: false,
  performanceAffinity: false,
  autoBackup: false,
  branding: true,
  crashDiagnosis: true,
};

export interface CrashCause {
  id: string;
  title: string;
  detail: string;
  action: string;
}

export interface CrashReport {
  instanceId: string;
  summary: string;
  severity: "warning" | "error" | "fatal";
  causes: CrashCause[];
  mods: string[];
  crashFile?: string;
  logExcerpt: string;
  generatedAt: string;
}

export interface MinecraftVersion {
  id: string;
  kind: string;
  releaseTime: string;
}

export interface LoaderVersion {
  version: string;
  stable: boolean;
}

export interface Instance {
  id: string;
  name: string;
  versionId: string;
  loader: Loader;
  loaderVersion?: string;
  color: string;
  memoryMin: number;
  memoryMax: number;
  resolutionWidth: number;
  resolutionHeight: number;
  fullscreen: boolean;
  closeOnLaunch: boolean;
  javaPath?: string;
  jvmArguments: string;
  serverAddress?: string;
  iconUrl?: string;
  iconFile?: string;
  modules?: ModuleState;
  createdAt: string;
  lastPlayedAt?: string;
  totalPlaySeconds: number;
}

export interface InstanceInput {
  name: string;
  versionId: string;
  loader: Loader;
  loaderVersion?: string;
  color: string;
  memoryMin: number;
  memoryMax: number;
  resolutionWidth: number;
  resolutionHeight: number;
  fullscreen: boolean;
  closeOnLaunch: boolean;
  javaPath?: string;
  jvmArguments: string;
  serverAddress?: string;
  modules?: ModuleState;
}

export interface Cape {
  id: string;
  state: "ACTIVE" | "INACTIVE";
  url: string;
  alias: string;
}

export interface Account {
  id: string;
  name: string;
  skinUrl?: string;
  skinVariant?: string;
  capes: Cape[];
  expiresAt: string;
}

export interface AppSettings {
  language: "fr" | "en";
  theme: Theme;
  accent: string;
  reducedMotion: boolean;
  downloadConcurrency: number;
  microsoftClientId: string;
  discordEnabled: boolean;
  autoUpdate: boolean;
}

export interface StorageInfo {
  dataDirectory: string;
  installDirectory: string;
  installDataDirectory: string;
  defaultDirectory: string;
  configured: boolean;
  hasExistingData: boolean;
  instanceCount: number;
}

export interface ContentEntry {
  fileName: string;
  kind: ContentKind;
  enabled: boolean;
  size: number;
  modifiedAt: string;
  projectId?: string;
  projectTitle?: string;
  versionId?: string;
  iconUrl?: string;
}

export interface ImportFailure {
  fileName: string;
  message: string;
}

export interface ImportSummary {
  imported: ContentEntry[];
  skipped: string[];
  failed: ImportFailure[];
}

export interface ImportReport {
  instance: Instance;
  format: string;
  importedFiles: number;
  warnings: string[];
}

export interface ExportOptions {
  includeWorlds: boolean;
  includeScreenshots: boolean;
}

export interface ExportReport {
  path: string;
  files: number;
  bytes: number;
}

export interface ProjectHit {
  projectId: string;
  slug: string;
  title: string;
  description: string;
  author: string;
  iconUrl?: string;
  downloads: number;
  follows: number;
  projectType: string;
  versions: string[];
}

export interface ModrinthVersion {
  id: string;
  name: string;
  versionNumber: string;
  versionType: "release" | "beta" | "alpha";
  datePublished: string;
  gameVersions: string[];
  loaders: string[];
}

export interface ServerEntry {
  id: string;
  name: string;
  address: string;
  port: number;
  instanceId?: string;
}

export interface ServerStatus extends ServerEntry {
  online: boolean;
  latency?: number;
  onlinePlayers?: number;
  maxPlayers?: number;
  motd?: string;
  version?: string;
  icon?: string;
  error?: string;
}

export interface BackupEntry {
  id: string;
  instanceId: string;
  name: string;
  createdAt: string;
  size: number;
  path: string;
}

export interface LaunchResult {
  pid: number;
  startedAt: string;
}

export interface GameLog {
  instanceId: string;
  line: string;
  stream: "stdout" | "stderr";
}

export interface ProgressEvent {
  operation: string;
  label: string;
  current: number;
  total: number;
}

export interface DeviceCodeEvent {
  userCode: string;
  verificationUri: string;
  expiresIn: number;
  message: string;
}
