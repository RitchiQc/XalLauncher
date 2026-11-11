mod auth;
mod backup;
mod branding;
mod content;
mod crash;
mod download;
mod error;
mod http;
mod instances;
mod java;
mod launch;
mod loaders;
mod models;
mod modules;
mod modpacks;
mod options;
mod providers;
mod servers;
mod store;
mod versions;

use std::collections::HashMap;
use std::process::Child;
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub store: store::Store,
    pub client: reqwest::Client,
    pub processes: Arc<Mutex<HashMap<String, Arc<Mutex<Child>>>>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            store: store::Store::new().expect("Impossible d’initialiser le stockage XalLauncher"),
            client: http::client().expect("Impossible d’initialiser le client réseau"),
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter("xallauncher=info")
        .init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            store::get_settings,
            store::update_settings,
            auth::list_accounts,
            auth::begin_microsoft_login,
            auth::remove_account,
            auth::refresh_account,
            auth::set_active_cape,
            versions::list_versions,
            versions::list_loader_versions,
            instances::list_instances,
            instances::create_instance,
            instances::update_instance,
            instances::duplicate_instance,
            instances::delete_instance,
            instances::open_instance_folder,
            launch::launch_instance,
            launch::stop_game,
            launch::running_games,
            launch::read_instance_log,
            providers::modrinth::search_modrinth,
            providers::modrinth::list_modrinth_versions,
            providers::modrinth::install_modrinth_project,
            content::list_content,
            content::toggle_content,
            content::delete_content,
            content::import_content,
            content::content_file_path,
            servers::list_servers,
            servers::upsert_server,
            servers::delete_server,
            servers::query_server,
            backup::list_backups,
            backup::create_backup,
            backup::restore_backup,
            backup::delete_backup,
            crash::diagnose_crash,
            modpacks::import_modrinth_pack,
            modpacks::install_modrinth_modpack,
        ])
        .run(tauri::generate_context!())
        .expect("Erreur fatale de XalLauncher");
}
