export type Loader = "vanilla" | "fabric" | "quilt" | "forge" | "neoforge";
export type ContentKind = "mod" | "resourcepack" | "shader" | "world";

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
  capes: Cape[];
  expiresAt: string;
}

export interface AppSettings {
  language: "fr" | "en";
  theme: "dark" | "light";
  accent: string;
  reducedMotion: boolean;
  downloadConcurrency: number;
  microsoftClientId: string;
  discordEnabled: boolean;
  autoUpdate: boolean;
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
