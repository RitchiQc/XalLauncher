use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub version_id: String,
    pub loader: Loader,
    #[serde(default)]
    pub loader_version: Option<String>,
    pub color: String,
    pub memory_min: u32,
    pub memory_max: u32,
    pub resolution_width: u32,
    pub resolution_height: u32,
    pub fullscreen: bool,
    pub close_on_launch: bool,
    #[serde(default)]
    pub java_path: Option<PathBuf>,
    #[serde(default)]
    pub jvm_arguments: String,
    #[serde(default)]
    pub server_address: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub last_played_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub total_play_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceInput {
    pub name: String,
    pub version_id: String,
    pub loader: Loader,
    #[serde(default)]
    pub loader_version: Option<String>,
    #[serde(default = "default_color")]
    pub color: String,
    #[serde(default = "default_min_memory")]
    pub memory_min: u32,
    #[serde(default = "default_memory")]
    pub memory_max: u32,
    #[serde(default = "default_width")]
    pub resolution_width: u32,
    #[serde(default = "default_height")]
    pub resolution_height: u32,
    #[serde(default)]
    pub fullscreen: bool,
    #[serde(default)]
    pub close_on_launch: bool,
    #[serde(default)]
    pub java_path: Option<PathBuf>,
    #[serde(default)]
    pub jvm_arguments: String,
    #[serde(default)]
    pub server_address: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    #[default]
    Vanilla,
    Fabric,
    Quilt,
    Forge,
    Neoforge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinecraftVersion {
    pub id: String,
    pub kind: String,
    pub release_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderVersion {
    pub version: String,
    pub stable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cape {
    pub id: String,
    pub state: String,
    pub url: String,
    #[serde(default)]
    pub alias: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub skin_url: Option<String>,
    #[serde(default)]
    pub capes: Vec<Cape>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub language: String,
    pub theme: String,
    pub accent: String,
    pub reduced_motion: bool,
    pub download_concurrency: usize,
    pub microsoft_client_id: String,
    pub discord_enabled: bool,
    pub auto_update: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            language: "fr".into(),
            theme: "dark".into(),
            accent: "#8b5cf6".into(),
            reduced_motion: false,
            download_concurrency: 8,
            microsoft_client_id: match option_env!("XAL_MICROSOFT_CLIENT_ID") {
                Some(value) if !value.is_empty() => value.into(),
                _ => "2a1271ce-d488-4351-9f70-e184334f4c73".into(),
            },
            discord_enabled: false,
            auto_update: true,
        }
    }
}

fn default_color() -> String {
    "#8b5cf6".into()
}
fn default_min_memory() -> u32 {
    512
}
fn default_memory() -> u32 {
    4096
}
fn default_width() -> u32 {
    1280
}
fn default_height() -> u32 {
    720
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentEntry {
    pub file_name: String,
    pub kind: String,
    pub enabled: bool,
    pub size: u64,
    pub modified_at: DateTime<Utc>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub project_title: Option<String>,
    #[serde(default)]
    pub version_id: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub author: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub follows: u64,
    pub project_type: String,
    pub versions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerEntry {
    pub id: String,
    pub name: String,
    pub address: String,
    pub port: u16,
    #[serde(default)]
    pub instance_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    #[serde(flatten)]
    pub server: ServerEntry,
    pub online: bool,
    #[serde(default)]
    pub latency: Option<u64>,
    #[serde(default)]
    pub online_players: Option<u64>,
    #[serde(default)]
    pub max_players: Option<u64>,
    #[serde(default)]
    pub motd: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupEntry {
    pub id: String,
    pub instance_id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub size: u64,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResult {
    pub pid: u32,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent<'a> {
    pub operation: &'a str,
    pub label: &'a str,
    pub current: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameLog<'a> {
    pub instance_id: &'a str,
    pub line: &'a str,
    pub stream: &'a str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodeEvent {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub message: String,
}
