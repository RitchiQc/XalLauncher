use crate::error::{AppError, Result};
use crate::models::{Instance, Loader, ModuleState, ProgressEvent};
use crate::store::Store;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};
use zip::write::SimpleFileOptions;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    #[serde(default)]
    pub include_worlds: bool,
    #[serde(default)]
    pub include_screenshots: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    pub path: String,
    pub files: u64,
    pub bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportedInstance<'a> {
    schema: u32,
    name: &'a str,
    color: &'a str,
    version_id: &'a str,
    loader: Loader,
    loader_version: Option<&'a str>,
    memory_min: u32,
    memory_max: u32,
    resolution_width: u32,
    resolution_height: u32,
    fullscreen: bool,
    close_on_launch: bool,
    jvm_arguments: &'a str,
    server_address: Option<&'a str>,
    modules: &'a ModuleState,
    exported_at: DateTime<Utc>,
}

#[tauri::command]
pub async fn export_instance(
    id: String,
    destination: PathBuf,
    options: ExportOptions,
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<ExportReport> {
    if state
        .processes
        .lock()
        .map_err(|_| AppError::new("Verrou du processus invalide"))?
        .contains_key(&id)
    {
        return Err(AppError::new(
            "Arrêtez le jeu avant d’exporter cette instance",
        ));
    }
    let store = state.store.clone();
    let instance = state.store.instance(&id)?;
    tokio::task::spawn_blocking(move || {
        export_sync(&store, &instance, &destination, &options, Some(&app))
    })
    .await
    .map_err(|error| AppError::new(error.to_string()))?
}

fn normalize_destination(path: &Path) -> Result<PathBuf> {
    if path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("mrpack"))
        .unwrap_or(false)
    {
        return Ok(path.to_path_buf());
    }
    if path.extension().is_none() {
        let mut path = path.to_path_buf();
        path.set_extension("mrpack");
        return Ok(path);
    }
    Err(AppError::new("L’export doit utiliser l’extension .mrpack"))
}

fn build_index(instance: &Instance) -> Value {
    let mut dependencies = Map::new();
    dependencies.insert(
        "minecraft".into(),
        Value::String(instance.version_id.clone()),
    );
    if let Some(loader_version) = instance.loader_version.as_deref() {
        let loader = match instance.loader {
            Loader::Fabric => Some(("fabric-loader", loader_version.to_string())),
            Loader::Quilt => Some(("quilt-loader", loader_version.to_string())),
            Loader::Forge => Some((
                "forge",
                loader_version
                    .strip_prefix(&format!("{}-", instance.version_id))
                    .unwrap_or(loader_version)
                    .to_string(),
            )),
            Loader::Neoforge => Some(("neoforge", loader_version.to_string())),
            Loader::Vanilla => None,
        };
        if let Some((key, value)) = loader {
            dependencies.insert(key.into(), Value::String(value));
        }
    }
    json!({
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": "1.0.0",
        "name": instance.name,
        "summary": "Instance exportée depuis XalLauncher",
        "files": [],
        "dependencies": dependencies,
    })
}

fn is_ignored_file(relative: &str) -> bool {
    let name = relative.rsplit('/').next().unwrap_or(relative);
    matches!(
        name,
        "usercache.json"
            | "launcher_profiles.json"
            | "launcher_accounts.json"
            | "launcher_log.txt"
            | ".xal-branding-version"
            | ".xal-performance-options"
    ) || name.ends_with(".xal-backup")
        || name.ends_with(".xal-tmp")
        || name.ends_with(".part")
        || relative == "resourcepacks/XalLauncher-Branding.zip"
}

fn collect_dir(root: &Path, current: &Path, options: &ExportOptions, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        if relative.is_empty() {
            continue;
        }
        if !options.include_worlds && (relative == "saves" || relative.starts_with("saves/")) {
            continue;
        }
        if !options.include_screenshots
            && (relative == "screenshots" || relative.starts_with("screenshots/"))
        {
            continue;
        }
        if matches!(
            relative.split('/').next().unwrap_or_default(),
            "logs" | "crash-reports"
        ) {
            continue;
        }
        if file_type.is_dir() {
            collect_dir(root, &path, options, files);
        } else if file_type.is_file() && !is_ignored_file(&relative) {
            files.push(path);
        }
    }
}

fn collect_files(game_dir: &Path, options: &ExportOptions) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_dir(game_dir, game_dir, options, &mut files);
    files.sort();
    files
}

fn emit_progress(app: Option<&AppHandle>, current: u64, total: u64) {
    if let Some(app) = app {
        let _ = app.emit(
            "download-progress",
            ProgressEvent {
                operation: "export",
                label: "Export de l’instance",
                current,
                total,
            },
        );
    }
}

pub(crate) fn export_sync(
    store: &Store,
    instance: &Instance,
    destination: &Path,
    options: &ExportOptions,
    app: Option<&AppHandle>,
) -> Result<ExportReport> {
    let destination = normalize_destination(destination)?;
    let parent = destination
        .parent()
        .ok_or_else(|| AppError::new("Dossier de destination invalide"))?;
    fs::create_dir_all(parent)?;
    let game_dir = store.game_dir(&instance.id);
    if !game_dir.is_dir() {
        return Err(AppError::new(
            "Le dossier de jeu de cette instance est introuvable",
        ));
    }
    let files = collect_files(&game_dir, options);
    let total = files.len() as u64;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    {
        let mut zip = zip::ZipWriter::new(temp.as_file_mut());
        zip.start_file(
            "modrinth.index.json",
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )?;
        zip.write_all(&serde_json::to_vec_pretty(&build_index(instance))?)?;
        let exported = ExportedInstance {
            schema: 1,
            name: &instance.name,
            color: &instance.color,
            version_id: &instance.version_id,
            loader: instance.loader,
            loader_version: instance.loader_version.as_deref(),
            memory_min: instance.memory_min,
            memory_max: instance.memory_max,
            resolution_width: instance.resolution_width,
            resolution_height: instance.resolution_height,
            fullscreen: instance.fullscreen,
            close_on_launch: instance.close_on_launch,
            jvm_arguments: &instance.jvm_arguments,
            server_address: instance.server_address.as_deref(),
            modules: &instance.modules,
            exported_at: Utc::now(),
        };
        zip.start_file(
            "xallauncher.json",
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )?;
        zip.write_all(&serde_json::to_vec_pretty(&exported)?)?;
        let mut written = 0_u64;
        for path in files {
            let relative = path
                .strip_prefix(&game_dir)
                .map_err(|_| AppError::new("Chemin d’export invalide"))?;
            let name = format!(
                "overrides/{}",
                relative.to_string_lossy().replace('\\', "/")
            );
            let size = fs::metadata(&path)?.len();
            zip.start_file(
                name,
                SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated)
                    .large_file(size >= 4 * 1024 * 1024 * 1024),
            )?;
            let mut file = fs::File::open(&path)?;
            std::io::copy(&mut file, &mut zip)?;
            written += 1;
            if written.is_multiple_of(10) || written == total {
                emit_progress(app, written, total);
            }
        }
        zip.finish()?;
    }
    let bytes = temp.as_file().metadata()?.len();
    temp.persist(&destination).map_err(|error| {
        AppError::new(format!(
            "Écriture de l’archive impossible : {}",
            error.error
        ))
    })?;
    Ok(ExportReport {
        path: destination.to_string_lossy().to_string(),
        files: total,
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use zip::ZipArchive;

    fn test_instance(id: &str, loader: Loader, loader_version: Option<&str>) -> Instance {
        Instance {
            id: id.into(),
            name: "Test".into(),
            version_id: "1.20.1".into(),
            loader,
            loader_version: loader_version.map(str::to_string),
            color: "#8b5cf6".into(),
            memory_min: 512,
            memory_max: 4096,
            resolution_width: 1280,
            resolution_height: 720,
            fullscreen: false,
            close_on_launch: false,
            java_path: None,
            jvm_arguments: String::new(),
            server_address: None,
            modules: ModuleState::default(),
            created_at: Utc::now(),
            last_played_at: None,
            total_play_seconds: 0,
        }
    }

    fn collect_names(path: &Path) -> Vec<String> {
        let file = fs::File::open(path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        (0..archive.len())
            .map(|index| archive.by_index(index).unwrap().name().unwrap().to_string())
            .collect()
    }

    #[test]
    fn exports_self_contained_mrpack() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::temporary(directory.path());
        let instance = test_instance("instance", Loader::Fabric, Some("0.15.11"));
        let game_dir = store.game_dir(&instance.id);
        fs::create_dir_all(game_dir.join("mods")).unwrap();
        fs::create_dir_all(game_dir.join("logs")).unwrap();
        fs::create_dir_all(game_dir.join("resourcepacks")).unwrap();
        fs::create_dir_all(game_dir.join("saves/world")).unwrap();
        fs::write(game_dir.join("mods/a.jar"), b"jar").unwrap();
        fs::write(game_dir.join("logs/latest.log"), b"log").unwrap();
        fs::write(
            game_dir.join("resourcepacks/XalLauncher-Branding.zip"),
            b"brand",
        )
        .unwrap();
        fs::write(game_dir.join("resourcepacks/Other.zip"), b"pack").unwrap();
        fs::write(game_dir.join("saves/world/level.dat"), b"level").unwrap();
        fs::write(game_dir.join("options.txt"), b"fov:0.5").unwrap();
        fs::write(game_dir.join(".xal-performance-options"), b"x").unwrap();

        let destination = directory.path().join("export.mrpack");
        let report = export_sync(
            &store,
            &instance,
            &destination,
            &ExportOptions::default(),
            None,
        )
        .unwrap();

        assert_eq!(report.files, 3);
        let names = collect_names(&destination);
        assert!(names.contains(&"modrinth.index.json".to_string()));
        assert!(names.contains(&"xallauncher.json".to_string()));
        assert!(names.contains(&"overrides/mods/a.jar".to_string()));
        assert!(names.contains(&"overrides/options.txt".to_string()));
        assert!(names.contains(&"overrides/resourcepacks/Other.zip".to_string()));
        assert!(!names.iter().any(|name| name.contains("logs")));
        assert!(!names
            .iter()
            .any(|name| name.contains("XalLauncher-Branding")));
        assert!(!names.iter().any(|name| name.contains("saves")));
        assert!(!names.iter().any(|name| name.contains("xal-performance")));

        let file = fs::File::open(&destination).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        let mut index = String::new();
        archive
            .by_name("modrinth.index.json")
            .unwrap()
            .read_to_string(&mut index)
            .unwrap();
        let index: Value = serde_json::from_str(&index).unwrap();
        assert_eq!(index["dependencies"]["minecraft"], "1.20.1");
        assert_eq!(index["dependencies"]["fabric-loader"], "0.15.11");
        assert_eq!(index["files"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn includes_worlds_and_screenshots_when_requested() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::temporary(directory.path());
        let instance = test_instance("instance", Loader::Vanilla, None);
        let game_dir = store.game_dir(&instance.id);
        fs::create_dir_all(game_dir.join("saves/world")).unwrap();
        fs::create_dir_all(game_dir.join("screenshots")).unwrap();
        fs::write(game_dir.join("saves/world/level.dat"), b"level").unwrap();
        fs::write(game_dir.join("screenshots/shot.png"), b"png").unwrap();

        let destination = directory.path().join("export.mrpack");
        let report = export_sync(
            &store,
            &instance,
            &destination,
            &ExportOptions {
                include_worlds: true,
                include_screenshots: true,
            },
            None,
        )
        .unwrap();

        assert_eq!(report.files, 2);
        let names = collect_names(&destination);
        assert!(names.contains(&"overrides/saves/world/level.dat".to_string()));
        assert!(names.contains(&"overrides/screenshots/shot.png".to_string()));
    }

    #[test]
    fn strips_forge_minecraft_prefix() {
        let instance = test_instance("instance", Loader::Forge, Some("1.20.1-47.2.0"));
        let index = build_index(&instance);
        assert_eq!(index["dependencies"]["forge"], "47.2.0");
    }

    #[test]
    fn validates_destination_extension() {
        assert!(normalize_destination(Path::new("pack.zip")).is_err());
        let path = normalize_destination(Path::new("pack")).unwrap();
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("mrpack")
        );
    }
}
