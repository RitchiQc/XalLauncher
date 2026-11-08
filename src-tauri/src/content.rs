use crate::error::{AppError, Result};
use crate::models::ContentEntry;
use crate::providers::modrinth::ContentMetadata;
use chrono::{DateTime, Utc};
use std::fs;
use std::path::{Component, Path, PathBuf};

fn folder_for_kind(kind: &str) -> Result<&'static str> {
    match kind {
        "mod" => Ok("mods"),
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

fn sidecar(path: &Path) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!(".{name}.xal.json"))
}

fn metadata(path: &Path) -> Option<ContentMetadata> {
    let direct = sidecar(path);
    let alternate = path
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(".disabled"))
        .map(|name| path.with_file_name(format!(".{name}.xal.json")));
    fs::read(&direct)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .or_else(|| {
            alternate
                .and_then(|path| fs::read(path).ok())
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        })
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
    let folder = state
        .store
        .game_dir(&instance_id)
        .join(folder_for_kind(&kind)?);
    if !folder.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for item in fs::read_dir(folder)?.flatten() {
        let name = item.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let path = item.path();
        let is_dir = item.file_type()?.is_dir();
        if (kind == "world") != is_dir {
            continue;
        }
        let file_meta = item.metadata()?;
        let modified = file_meta
            .modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let metadata = metadata(&path);
        entries.push(ContentEntry {
            enabled: !name.ends_with(".disabled"),
            file_name: name,
            kind: kind.clone(),
            size: if is_dir {
                directory_size(&path)
            } else {
                file_meta.len()
            },
            modified_at: DateTime::<Utc>::from(modified),
            project_id: metadata.as_ref().map(|meta| meta.project_id.clone()),
            project_title: metadata.as_ref().map(|meta| meta.project_title.clone()),
            version_id: metadata.as_ref().map(|meta| meta.version_id.clone()),
            icon_url: metadata.and_then(|meta| meta.icon_url),
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
    let path = state
        .store
        .game_dir(&instance_id)
        .join(folder_for_kind(&kind)?)
        .join(name);
    if !path.is_file() {
        return Err(AppError::new("Fichier introuvable"));
    }
    let new_name = name
        .strip_suffix(".disabled")
        .map(str::to_string)
        .unwrap_or_else(|| format!("{name}.disabled"));
    let destination = path.with_file_name(new_name);
    fs::rename(path, &destination)?;
    list_content(instance_id, kind, state)?
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
    let path = state
        .store
        .game_dir(&instance_id)
        .join(folder_for_kind(&kind)?)
        .join(name);
    if path.is_dir() {
        fs::remove_dir_all(&path)?;
    } else if path.is_file() {
        fs::remove_file(&path)?;
    }
    let _ = fs::remove_file(sidecar(&path));
    if let Some(base) = name.strip_suffix(".disabled") {
        let _ = fs::remove_file(path.with_file_name(format!(".{base}.xal.json")));
    }
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
    let path = state
        .store
        .game_dir(&instance_id)
        .join(folder_for_kind(&kind)?)
        .join(name);
    if !path.exists() {
        return Err(AppError::new("Fichier introuvable"));
    }
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn import_content(
    instance_id: String,
    kind: String,
    source_path: PathBuf,
    state: tauri::State<'_, crate::AppState>,
) -> Result<ContentEntry> {
    if kind == "world" || kind == "screenshot" {
        return Err(AppError::new(
            "Utilisez le dossier du jeu pour ce type de contenu",
        ));
    }
    if !source_path.is_file() {
        return Err(AppError::new("Le fichier à importer est introuvable"));
    }
    let name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::new("Nom de fichier invalide"))?;
    safe_name(name)?;
    let folder = state
        .store
        .game_dir(&instance_id)
        .join(folder_for_kind(&kind)?);
    fs::create_dir_all(&folder)?;
    let destination = folder.join(name);
    fs::copy(&source_path, &destination)?;
    list_content(instance_id, kind, state)?
        .into_iter()
        .find(|entry| entry.file_name == name)
        .ok_or_else(|| AppError::new("Import terminé mais contenu introuvable"))
}
