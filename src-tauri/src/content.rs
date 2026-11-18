use crate::error::{AppError, Result};
use crate::models::{ContentEntry, Loader};
use crate::providers::horizon::{mod_enabled, mod_id_from_file, set_mod_enabled};
use chrono::{DateTime, Utc};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub(crate) fn folder_for_kind(loader: Loader, kind: &str) -> Result<&'static str> {
    match kind {
        "mod" => Ok(if loader == Loader::Horizon {
            "horizon-mods"
        } else {
            "mods"
        }),
        "native-mod" if loader == Loader::Horizon => Ok("horizon-native-mods"),
        "native-mod" => Err(AppError::new(
            "Les mods natifs nécessitent une instance Horizon",
        )),
        "resourcepack" => Ok("resourcepacks"),
        "shader" => Ok("shaderpacks"),
        "world" => Ok("saves"),
        "screenshot" => Ok("screenshots"),
        _ => Err(AppError::new("Type de contenu inconnu")),
    }
}

fn safe_name(name: &str) -> Result<&str> {
    let mut components = Path::new(name).components();
    if components.next() != Some(Component::Normal(std::ffi::OsStr::new(name)))
        || components.next().is_some()
        || name.is_empty()
    {
        return Err(AppError::new("Nom de fichier invalide"));
    }
    Ok(name)
}

fn directory_size(path: &Path) -> u64 {
    fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| {
                    if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                        directory_size(&entry.path())
                    } else {
                        entry.metadata().map(|meta| meta.len()).unwrap_or(0)
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}

#[tauri::command]
pub fn list_content(
    instance_id: String,
    kind: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Vec<ContentEntry>> {
    list_entries(&state.store, &instance_id, &kind)
}

pub(crate) fn list_entries(
    store: &crate::store::Store,
    instance_id: &str,
    kind: &str,
) -> Result<Vec<ContentEntry>> {
    let loader = store.instance(instance_id)?.loader;
    let instance_dir = store.instance_dir(instance_id);
    let game_dir = store.game_dir(instance_id);
    let folder_name = folder_for_kind(loader, kind)?;
    let folder = game_dir.join(folder_name);
    if !folder.exists() {
        return Ok(Vec::new());
    }
    crate::content_meta::migrate_folder(&instance_dir, folder_name);
    let mut entries = Vec::new();
    for item in fs::read_dir(folder)?.flatten() {
        let name = item.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if loader == Loader::Horizon
            && kind == "mod"
            && !(name.ends_with(".hmod") || name.ends_with(".hmod.disabled"))
        {
            continue;
        }
        let path = item.path();
        let is_dir = item.file_type()?.is_dir();
        if (kind == "world" || kind == "native-mod") != is_dir {
            continue;
        }
        let file_meta = item.metadata()?;
        let modified = file_meta
            .modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let metadata = crate::content_meta::read(&instance_dir, folder_name, &name);
        entries.push(ContentEntry {
            enabled: if loader == Loader::Horizon && kind == "native-mod" {
                mod_enabled(&game_dir, &name)
            } else if loader == Loader::Horizon && kind == "mod" {
                !name.ends_with(".disabled") && mod_enabled(&game_dir, mod_id_from_file(&name))
            } else {
                !name.ends_with(".disabled")
            },
            file_name: name,
            kind: kind.to_string(),
            size: if is_dir {
                directory_size(&path)
            } else {
                file_meta.len()
            },
            modified_at: DateTime::<Utc>::from(modified),
            project_id: metadata.as_ref().map(|meta| meta.project_id.clone()),
            project_title: metadata.as_ref().map(|meta| meta.project_title.clone()),
            version_id: metadata.as_ref().map(|meta| meta.version_id.clone()),
            icon_url: metadata.as_ref().and_then(|meta| meta.icon_url.clone()),
            provider: metadata.and_then(|meta| meta.provider),
        });
    }
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.modified_at));
    Ok(entries)
}

#[tauri::command]
pub fn toggle_content(
    instance_id: String,
    kind: String,
    file_name: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<ContentEntry> {
    if kind == "world" || kind == "screenshot" {
        return Err(AppError::new("Ce contenu ne peut pas être désactivé"));
    }
    let name = safe_name(&file_name)?;
    let loader = state.store.instance(&instance_id)?.loader;
    let game_dir = state.store.game_dir(&instance_id);
    let path = game_dir.join(folder_for_kind(loader, &kind)?).join(name);
    if loader == Loader::Horizon && kind == "native-mod" {
        if !path.is_dir() {
            return Err(AppError::new("Mod natif Horizon introuvable"));
        }
        let enabled = mod_enabled(&game_dir, name);
        set_mod_enabled(&game_dir, name, !enabled)?;
        return list_entries(&state.store, &instance_id, &kind)?
            .into_iter()
            .find(|entry| entry.file_name == name)
            .ok_or_else(|| AppError::new("Mod natif Horizon introuvable"));
    }
    if !path.is_file() {
        return Err(AppError::new("Fichier introuvable"));
    }
    if loader == Loader::Horizon && kind == "mod" {
        let active_name = name.strip_suffix(".disabled").unwrap_or(name);
        let active_path = path.with_file_name(active_name);
        let enabled =
            !name.ends_with(".disabled") && mod_enabled(&game_dir, mod_id_from_file(active_name));
        if !enabled && active_path != path {
            fs::rename(&path, &active_path)?;
        }
        set_mod_enabled(&game_dir, mod_id_from_file(active_name), !enabled)?;
        return list_entries(&state.store, &instance_id, &kind)?
            .into_iter()
            .find(|entry| entry.file_name == active_name)
            .ok_or_else(|| AppError::new("Contenu Horizon introuvable"));
    }
    let new_name = name
        .strip_suffix(".disabled")
        .map(str::to_string)
        .unwrap_or_else(|| format!("{name}.disabled"));
    let destination = path.with_file_name(new_name);
    fs::rename(path, &destination)?;
    list_entries(&state.store, &instance_id, &kind)?
        .into_iter()
        .find(|entry| {
            entry.file_name
                == destination
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
        })
        .ok_or_else(|| AppError::new("Contenu renommé mais introuvable"))
}

#[tauri::command]
pub fn delete_content(
    instance_id: String,
    kind: String,
    file_name: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<()> {
    let name = safe_name(&file_name)?;
    let loader = state.store.instance(&instance_id)?.loader;
    let game_dir = state.store.game_dir(&instance_id);
    let folder = folder_for_kind(loader, &kind)?;
    let path = game_dir.join(folder).join(name);
    if path.is_dir() {
        fs::remove_dir_all(&path)?;
    } else if path.is_file() {
        fs::remove_file(&path)?;
    }
    if loader == Loader::Horizon && (kind == "mod" || kind == "native-mod") {
        let _ = set_mod_enabled(&game_dir, mod_id_from_file(name), false);
    }
    crate::content_meta::remove(&state.store.instance_dir(&instance_id), folder, name);
    Ok(())
}

#[tauri::command]
pub fn content_file_path(
    instance_id: String,
    kind: String,
    file_name: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<String> {
    let name = safe_name(&file_name)?;
    let loader = state.store.instance(&instance_id)?.loader;
    let path = state
        .store
        .game_dir(&instance_id)
        .join(folder_for_kind(loader, &kind)?)
        .join(name);
    if !path.exists() {
        return Err(AppError::new("Fichier introuvable"));
    }
    Ok(path.to_string_lossy().to_string())
}

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub imported: Vec<ContentEntry>,
    pub skipped: Vec<String>,
    pub failed: Vec<ImportFailure>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFailure {
    pub file_name: String,
    pub message: String,
}

fn allowed_extensions(loader: Loader, kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "mod" => Some(if loader == Loader::Horizon {
            &["hmod"]
        } else {
            &["jar"]
        }),
        "resourcepack" => Some(&["zip"]),
        "shader" => Some(if loader == Loader::Horizon {
            &["zip", "hshader"]
        } else {
            &["zip"]
        }),
        _ => None,
    }
}

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            extensions
                .iter()
                .any(|allowed| extension.eq_ignore_ascii_case(allowed))
        })
        .unwrap_or(false)
}

fn copy_content(
    store: &crate::store::Store,
    instance_id: &str,
    kind: &str,
    source_path: &Path,
) -> Result<ContentEntry> {
    if kind == "native-mod" {
        return Err(AppError::new(
            "Les mods natifs Horizon s’installent depuis le marché Horizon",
        ));
    }
    if kind == "world" || kind == "screenshot" {
        return Err(AppError::new(
            "Utilisez le dossier du jeu pour ce type de contenu",
        ));
    }
    let loader = store
        .instance(instance_id)
        .map(|instance| instance.loader)
        .unwrap_or(Loader::Vanilla);
    let extensions =
        allowed_extensions(loader, kind).ok_or_else(|| AppError::new("Type de contenu inconnu"))?;
    if !source_path.is_file() {
        return Err(AppError::new("Le fichier à importer est introuvable"));
    }
    if !has_extension(source_path, extensions) {
        return Err(AppError::new(format!(
            "Format non pris en charge : .{} attendu",
            extensions.join(" ou .")
        )));
    }
    let name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::new("Nom de fichier invalide"))?;
    safe_name(name)?;
    let folder_name = folder_for_kind(loader, kind)?;
    let folder = store.game_dir(instance_id).join(folder_name);
    fs::create_dir_all(&folder)?;
    let destination = folder.join(name);
    if destination.exists() {
        return Err(AppError::new(format!("« {name} » est déjà présent")));
    }
    fs::copy(source_path, &destination)?;
    let file_meta = destination.metadata()?;
    let modified = file_meta
        .modified()
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let metadata =
        crate::content_meta::read(&store.instance_dir(instance_id), folder_name, name);
    Ok(ContentEntry {
        enabled: true,
        file_name: name.to_string(),
        kind: kind.to_string(),
        size: file_meta.len(),
        modified_at: DateTime::<Utc>::from(modified),
        project_id: metadata.as_ref().map(|meta| meta.project_id.clone()),
        project_title: metadata.as_ref().map(|meta| meta.project_title.clone()),
        version_id: metadata.as_ref().map(|meta| meta.version_id.clone()),
        icon_url: metadata.as_ref().and_then(|meta| meta.icon_url.clone()),
        provider: metadata.and_then(|meta| meta.provider),
    })
}

#[tauri::command]
pub fn import_content(
    instance_id: String,
    kind: String,
    source_path: PathBuf,
    state: tauri::State<'_, crate::AppState>,
) -> Result<ContentEntry> {
    copy_content(&state.store, &instance_id, &kind, &source_path)
}

pub(crate) fn import_many(
    store: &crate::store::Store,
    instance_id: &str,
    kind: &str,
    source_paths: &[PathBuf],
) -> Result<ImportSummary> {
    if kind == "native-mod" {
        return Err(AppError::new(
            "Les mods natifs Horizon s’installent depuis le marché Horizon",
        ));
    }
    if kind == "world" || kind == "screenshot" {
        return Err(AppError::new(
            "Utilisez le dossier du jeu pour ce type de contenu",
        ));
    }
    let loader = store
        .instance(instance_id)
        .map(|instance| instance.loader)
        .unwrap_or(Loader::Vanilla);
    let folder = store
        .game_dir(instance_id)
        .join(folder_for_kind(loader, kind)?);
    let mut summary = ImportSummary::default();
    for source_path in source_paths {
        let file_name = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("fichier")
            .to_string();
        if folder.join(&file_name).exists() {
            summary.skipped.push(file_name);
            continue;
        }
        match copy_content(store, instance_id, kind, source_path) {
            Ok(entry) => summary.imported.push(entry),
            Err(error) => summary.failed.push(ImportFailure {
                file_name,
                message: error.0,
            }),
        }
    }
    Ok(summary)
}

#[tauri::command]
pub async fn import_contents(
    instance_id: String,
    kind: String,
    source_paths: Vec<PathBuf>,
    state: tauri::State<'_, crate::AppState>,
) -> Result<ImportSummary> {
    let store = state.store.clone();
    tokio::task::spawn_blocking(move || import_many(&store, &instance_id, &kind, &source_paths))
        .await
        .map_err(|error| AppError::new(error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::{folder_for_kind, import_many, list_entries};
    use crate::models::{Instance, Loader, ModuleState};
    use crate::content_meta::ContentMetadata;
    use crate::store::Store;

    #[test]
    fn imports_multiple_files_and_skips_duplicates() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::temporary(directory.path());
        let source = directory.path().join("source");
        std::fs::create_dir_all(&source).unwrap();
        let first = source.join("a.jar");
        let second = source.join("b.jar");
        std::fs::write(&first, b"a").unwrap();
        std::fs::write(&second, b"b").unwrap();

        let summary = import_many(
            &store,
            "instance",
            "mod",
            &[first.clone(), second.clone(), first],
        )
        .unwrap();

        assert_eq!(summary.imported.len(), 2);
        assert_eq!(summary.skipped.len(), 1);
        assert!(summary.failed.is_empty());
        assert!(store.game_dir("instance").join("mods/a.jar").is_file());
        assert!(store.game_dir("instance").join("mods/b.jar").is_file());
    }

    #[test]
    fn horizon_uses_its_own_mods_folder() {
        assert_eq!(
            folder_for_kind(Loader::Horizon, "mod").unwrap(),
            "horizon-mods"
        );
        assert_eq!(folder_for_kind(Loader::Vanilla, "mod").unwrap(), "mods");
        assert_eq!(
            folder_for_kind(Loader::Horizon, "native-mod").unwrap(),
            "horizon-native-mods"
        );
        assert!(folder_for_kind(Loader::Fabric, "native-mod").is_err());
    }

    #[test]
    fn lists_installed_native_mod_bundles() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::temporary(directory.path());
        store
            .save_instances(&[Instance {
                id: "instance".into(),
                name: "Horizon".into(),
                version_id: "26.3".into(),
                loader: Loader::Horizon,
                loader_version: None,
                color: "#8b5cf6".into(),
                memory_min: 512,
                memory_max: 4096,
                resolution_width: 1280,
                resolution_height: 720,
                fullscreen: false,
                close_on_launch: false,
                java_path: None,
                java_major: None,
                jvm_arguments: String::new(),
                server_address: None,
                icon_url: None,
                icon_file: None,
                modules: ModuleState::default(),
                created_at: chrono::Utc::now(),
                last_played_at: None,
                total_play_seconds: 0,
            }])
            .unwrap();
        let game_dir = store.game_dir("instance");
        let native = game_dir.join("horizon-native-mods");
        std::fs::create_dir_all(native.join("zoom")).unwrap();
        std::fs::write(native.join("stray.txt"), b"ignored").unwrap();
        let metadata = ContentMetadata {
            project_id: "zoom".into(),
            project_title: "Zoom".into(),
            version_id: "2.0.0".into(),
            icon_url: None,
            provider: Some("horizon".into()),
        };
        std::fs::write(
            native.join(".zoom.xal.json"),
            serde_json::to_vec_pretty(&metadata).unwrap(),
        )
        .unwrap();
        std::fs::write(game_dir.join("horizon-mods.txt"), b"mod.zoom:true\n").unwrap();

        let entries = list_entries(&store, "instance", "native-mod").unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].file_name, "zoom");
        assert!(entries[0].enabled);
        assert_eq!(entries[0].project_id.as_deref(), Some("zoom"));
        assert_eq!(entries[0].version_id.as_deref(), Some("2.0.0"));
        assert_eq!(entries[0].provider.as_deref(), Some("horizon"));
        assert!(!native.join(".zoom.xal.json").exists());
        assert!(store
            .instance_dir("instance")
            .join("metadata/horizon-native-mods/zoom.xal.json")
            .is_file());
    }

    #[test]
    fn rejects_unsupported_extensions() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::temporary(directory.path());
        let source = directory.path().join("mod.zip");
        std::fs::write(&source, b"x").unwrap();

        let summary = import_many(&store, "instance", "mod", &[source]).unwrap();

        assert!(summary.imported.is_empty());
        assert_eq!(summary.failed.len(), 1);
        assert!(summary.failed[0].message.contains(".jar"));
    }
}
