use crate::error::{AppError, Result};
use crate::models::BackupEntry;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const STORAGE_FILE: &str = "storage.json";
const DATA_ENTRIES: &[&str] = &[
    "instances",
    "versions",
    "libraries",
    "assets",
    "runtimes",
    "cache",
    "logs",
    "backups",
    "settings.json",
    "accounts.json",
    "instances.json",
    "servers.json",
    "backups.json",
];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct StorageConfig {
    #[serde(default)]
    data_directory: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageInfo {
    pub data_directory: PathBuf,
    pub install_directory: PathBuf,
    pub install_data_directory: PathBuf,
    pub default_directory: PathBuf,
    pub configured: bool,
    pub has_existing_data: bool,
    pub instance_count: usize,
}

fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("studio", "Xaltar", "XalLauncher")
        .ok_or_else(|| AppError::new("Impossible de déterminer le dossier des données utilisateur"))
}

fn config_path() -> Result<PathBuf> {
    Ok(project_dirs()?.config_dir().join(STORAGE_FILE))
}

fn read_config() -> Result<Option<StorageConfig>> {
    let path = config_path()?;
    if !path.is_file() {
        return Ok(None);
    }
    let content = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&content).ok())
}

fn save_config(path: &Path) -> Result<()> {
    let config_path = config_path()?;
    let parent = config_path
        .parent()
        .ok_or_else(|| AppError::new("Dossier de configuration invalide"))?;
    fs::create_dir_all(parent)?;
    let content = serde_json::to_vec_pretty(&StorageConfig {
        data_directory: Some(path.to_path_buf()),
    })?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(&content)?;
    temp.persist(&config_path).map_err(|error| {
        AppError::new(format!(
            "Enregistrement de {} : {}",
            config_path.display(),
            error.error
        ))
    })?;
    Ok(())
}

pub fn legacy_root() -> Result<PathBuf> {
    Ok(project_dirs()?.data_local_dir().to_path_buf())
}

pub fn install_directory() -> PathBuf {
    clean_path(
        std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from(".")),
    )
}

fn install_data_directory() -> PathBuf {
    if cfg!(debug_assertions) {
        if let Ok(root) = legacy_root() {
            return root;
        }
    }
    install_directory().join("data")
}

fn has_data(root: &Path) -> bool {
    [
        "instances.json",
        "settings.json",
        "accounts.json",
        "servers.json",
        "backups.json",
    ]
    .iter()
    .any(|name| root.join(name).is_file())
        || fs::read_dir(root.join("instances"))
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false)
}

fn instance_count(root: &Path) -> usize {
    fs::read_to_string(root.join("instances.json"))
        .ok()
        .and_then(|content| serde_json::from_str::<Vec<serde_json::Value>>(&content).ok())
        .map(|instances| instances.len())
        .unwrap_or(0)
}

fn is_subpath(parent: &Path, child: &Path) -> bool {
    child.starts_with(parent)
}

#[cfg(windows)]
fn clean_path(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy().into_owned();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    path
}

#[cfg(not(windows))]
fn clean_path(path: PathBuf) -> PathBuf {
    path
}

pub fn preferred_root() -> Result<PathBuf> {
    let legacy = legacy_root()?;
    if has_data(&legacy) {
        return Ok(legacy);
    }
    if !cfg!(debug_assertions) {
        let install = install_data_directory();
        if fs::create_dir_all(&install).is_ok() {
            return Ok(install);
        }
    }
    Ok(legacy)
}

pub fn resolve_data_root() -> Result<PathBuf> {
    if let Ok(Some(config)) = read_config() {
        if let Some(path) = config.data_directory {
            if path.is_absolute() && fs::create_dir_all(&path).is_ok() {
                return Ok(path);
            }
        }
    }
    preferred_root()
}

fn info_for(data_directory: &Path) -> Result<StorageInfo> {
    Ok(StorageInfo {
        data_directory: data_directory.to_path_buf(),
        install_directory: install_directory(),
        install_data_directory: install_data_directory(),
        default_directory: preferred_root()?,
        configured: read_config()?
            .and_then(|config| config.data_directory)
            .is_some(),
        has_existing_data: has_data(data_directory),
        instance_count: instance_count(data_directory),
    })
}

enum Transfer {
    Renamed,
    Copied,
}

fn copy_directory(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn migrate_entries_inner(
    source: &Path,
    target: &Path,
    done: &mut Vec<(PathBuf, PathBuf, Transfer)>,
) -> Result<()> {
    for name in DATA_ENTRIES {
        let from = source.join(name);
        let to = target.join(name);
        if !from.exists() {
            continue;
        }
        if to.exists() {
            return Err(AppError::new(format!(
                "Le dossier de destination contient déjà {name}"
            )));
        }
        match fs::rename(&from, &to) {
            Ok(()) => done.push((from, to, Transfer::Renamed)),
            Err(_) => {
                done.push((from.clone(), to.clone(), Transfer::Copied));
                if from.is_dir() {
                    copy_directory(&from, &to)?;
                } else {
                    fs::copy(&from, &to)?;
                }
            }
        }
    }
    Ok(())
}

fn migrate_entries(source: &Path, target: &Path) -> Result<()> {
    let mut done = Vec::new();
    let result = migrate_entries_inner(source, target, &mut done);
    if result.is_err() {
        for (from, to, transfer) in done.iter().rev() {
            match transfer {
                Transfer::Renamed => {
                    let _ = fs::rename(to, from);
                }
                Transfer::Copied => {
                    let _ = if to.is_dir() {
                        fs::remove_dir_all(to)
                    } else {
                        fs::remove_file(to)
                    };
                }
            }
        }
    }
    result
}

fn rewrite_backups(target: &Path, source: &Path) -> Result<()> {
    let path = target.join("backups.json");
    if !path.is_file() {
        return Ok(());
    }
    let mut backups: Vec<BackupEntry> = serde_json::from_str(&fs::read_to_string(&path)?)?;
    for backup in &mut backups {
        if let Ok(relative) = backup.path.strip_prefix(source) {
            backup.path = target.join(relative);
        }
    }
    fs::write(&path, serde_json::to_vec_pretty(&backups)?)?;
    Ok(())
}

#[tauri::command]
pub fn storage_info(state: tauri::State<'_, crate::AppState>) -> Result<StorageInfo> {
    info_for(state.store.root())
}

#[tauri::command]
pub async fn change_storage_directory(
    path: String,
    migrate: bool,
    state: tauri::State<'_, crate::AppState>,
) -> Result<StorageInfo> {
    if !state
        .processes
        .lock()
        .map_err(|_| AppError::new("Verrou du processus invalide"))?
        .is_empty()
    {
        return Err(AppError::new(
            "Arrêtez le jeu avant de changer l'emplacement des données",
        ));
    }
    let target_input = PathBuf::from(path.trim());
    if !target_input.is_absolute() {
        return Err(AppError::new(
            "Le dossier de données doit être un chemin absolu",
        ));
    }
    fs::create_dir_all(&target_input)?;
    let target_canonical = target_input.canonicalize()?;
    let current_canonical = state.store.root().canonicalize()?;
    if target_canonical == current_canonical {
        let target = clean_path(target_canonical);
        save_config(&target)?;
        return info_for(&target);
    }
    if is_subpath(&current_canonical, &target_canonical)
        || is_subpath(&target_canonical, &current_canonical)
    {
        return Err(AppError::new(
            "Le nouveau dossier ne peut pas être identique, parent ou enfant du dossier actuel",
        ));
    }
    let target = clean_path(target_canonical);
    let mut has_entries = false;
    for entry in fs::read_dir(&target)? {
        let entry = entry?;
        let name = entry.file_name();
        let known = DATA_ENTRIES
            .iter()
            .any(|known| name.as_os_str() == std::ffi::OsStr::new(known));
        if !known {
            return Err(AppError::new(
                "Le dossier choisi contient déjà des fichiers qui ne viennent pas de XalLauncher",
            ));
        }
        has_entries = true;
    }
    if migrate && has_entries {
        return Err(AppError::new(
            "Le dossier de destination contient déjà des données. Choisissez un dossier vide ou désactivez le déplacement",
        ));
    }
    let source = clean_path(current_canonical);
    let destination = target.clone();
    tokio::task::spawn_blocking(move || {
        if migrate {
            migrate_entries(&source, &destination)?;
            rewrite_backups(&destination, &source)?;
        }
        Ok::<(), AppError>(())
    })
    .await
    .map_err(|error| AppError::new(error.to_string()))??;
    save_config(&target)?;
    info_for(&target)
}

#[tauri::command]
pub fn open_storage_folder(state: tauri::State<'_, crate::AppState>) -> Result<()> {
    let path = state.store.root().to_path_buf();
    fs::create_dir_all(&path)?;
    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer").arg(&path).spawn()?;
    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg(&path).spawn()?;
    #[cfg(all(unix, not(target_os = "macos")))]
    std::process::Command::new("xdg-open").arg(&path).spawn()?;
    Ok(())
}

#[tauri::command]
pub fn restart_app(app: tauri::AppHandle) {
    app.request_restart();
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::clean_path;
    use super::{has_data, is_subpath, migrate_entries, rewrite_backups};
    use std::fs;
    use std::path::Path;
    #[cfg(windows)]
    use std::path::PathBuf;

    #[test]
    fn detects_existing_data() {
        let directory = tempfile::tempdir().unwrap();
        assert!(!has_data(directory.path()));
        fs::write(directory.path().join("instances.json"), "[]").unwrap();
        assert!(has_data(directory.path()));
    }

    #[cfg(windows)]
    #[test]
    fn strips_verbatim_path_prefix() {
        assert_eq!(
            clean_path(PathBuf::from(r"\\?\C:\data")),
            PathBuf::from(r"C:\data")
        );
        assert_eq!(
            clean_path(PathBuf::from(r"\\?\UNC\server\share")),
            PathBuf::from(r"\\server\share")
        );
    }

    #[test]
    fn rejects_nested_storage_targets() {
        assert!(is_subpath(
            Path::new("C:/data"),
            Path::new("C:/data/instances")
        ));
        assert!(!is_subpath(Path::new("C:/data"), Path::new("D:/data")));
    }

    #[test]
    fn migrates_entries_and_rewrites_backups() {
        let source = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        fs::create_dir_all(source.path().join("instances")).unwrap();
        fs::write(source.path().join("instances/abc.txt"), "data").unwrap();
        let backup_directory = source.path().join("backups").join("instance");
        fs::create_dir_all(&backup_directory).unwrap();
        let backup_path = backup_directory.join("save.zip");
        fs::write(&backup_path, "zip").unwrap();
        fs::write(source.path().join("instances.json"), "[]").unwrap();
        fs::write(
            source.path().join("backups.json"),
            format!(
                "[{{\"id\":\"1\",\"instanceId\":\"i\",\"name\":\"n\",\"createdAt\":\"2026-01-01T00:00:00Z\",\"size\":3,\"path\":{}}}]",
                serde_json::to_string(&backup_path).unwrap()
            ),
        )
        .unwrap();

        migrate_entries(source.path(), target.path()).unwrap();
        rewrite_backups(target.path(), source.path()).unwrap();

        assert!(target.path().join("instances/abc.txt").is_file());
        assert!(target.path().join("backups/instance/save.zip").is_file());
        let backups: Vec<serde_json::Value> =
            serde_json::from_str(&fs::read_to_string(target.path().join("backups.json")).unwrap())
                .unwrap();
        let rewritten = Path::new(backups[0]["path"].as_str().unwrap());
        assert!(rewritten.is_file());
        assert!(rewritten.starts_with(target.path()));
    }
}
