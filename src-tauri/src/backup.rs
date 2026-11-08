use crate::error::{AppError, Result};
use crate::models::BackupEntry;
use crate::store::Store;
use chrono::Utc;
use std::fs;
use std::io::Write;
use std::path::Path;
use zip::write::SimpleFileOptions;

fn add_directory(zip: &mut zip::ZipWriter<fs::File>, source: &Path, prefix: &Path) -> Result<()> {
    if !source.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(source)?.flatten() {
        let path = entry.path();
        let relative = prefix.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            add_directory(zip, &path, &relative)?;
        } else {
            zip.start_file(
                relative.to_string_lossy().replace('\\', "/"),
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
            )?;
            let mut file = fs::File::open(&path)?;
            std::io::copy(&mut file, zip)?;
        }
    }
    Ok(())
}

fn create_zip(store: &Store, instance_id: &str, name: String) -> Result<BackupEntry> {
    let instance = store.instance(instance_id)?;
    let id = uuid::Uuid::new_v4().to_string();
    let directory = store.backups_dir().join(instance_id);
    fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{}-{id}.zip", Utc::now().format("%Y%m%d-%H%M%S")));
    let file = fs::File::create(&path)?;
    let mut zip = zip::ZipWriter::new(file);
    zip.start_file("instance.json", SimpleFileOptions::default())?;
    zip.write_all(&serde_json::to_vec_pretty(&instance)?)?;
    add_directory(
        &mut zip,
        &store.game_dir(instance_id).join("saves"),
        Path::new("worlds"),
    )?;
    zip.finish()?;
    let entry = BackupEntry {
        id,
        instance_id: instance_id.into(),
        name,
        created_at: Utc::now(),
        size: path.metadata()?.len(),
        path,
    };
    let mut backups = store.backups()?;
    backups.push(entry.clone());
    store.save_backups(&backups)?;
    Ok(entry)
}

#[tauri::command]
pub async fn create_backup(
    instance_id: String,
    name: Option<String>,
    state: tauri::State<'_, crate::AppState>,
) -> Result<BackupEntry> {
    let store = state.store.clone();
    tokio::task::spawn_blocking(move || {
        create_zip(
            &store,
            &instance_id,
            name.unwrap_or_else(|| {
                format!("Sauvegarde du {}", Utc::now().format("%d/%m/%Y %H:%M"))
            }),
        )
    })
    .await
    .map_err(|error| AppError::new(error.to_string()))?
}

#[tauri::command]
pub fn list_backups(
    instance_id: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Vec<BackupEntry>> {
    let mut backups: Vec<BackupEntry> = store_backups_filtered(&state.store, &instance_id)?;
    backups.sort_by_key(|backup| std::cmp::Reverse(backup.created_at));
    Ok(backups)
}

fn store_backups_filtered(store: &Store, instance_id: &str) -> Result<Vec<BackupEntry>> {
    Ok(store
        .backups()?
        .into_iter()
        .filter(|backup| backup.instance_id == instance_id)
        .collect())
}

#[tauri::command]
pub async fn restore_backup(
    backup_id: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<()> {
    let store = state.store.clone();
    tokio::task::spawn_blocking(move || {
        let backup = store
            .backups()?
            .into_iter()
            .find(|backup| backup.id == backup_id)
            .ok_or_else(|| AppError::new("Sauvegarde introuvable"))?;
        if !backup.path.is_file() {
            return Err(AppError::new("Le fichier de sauvegarde est introuvable"));
        }
        create_zip(
            &store,
            &backup.instance_id,
            format!("Avant restauration de {}", backup.name),
        )?;
        let file = fs::File::open(&backup.path)?;
        let mut zip = zip::ZipArchive::new(file)?;
        let saves = store.game_dir(&backup.instance_id).join("saves");
        fs::create_dir_all(&saves)?;
        for index in 0..zip.len() {
            let mut entry = zip.by_index(index)?;
            let Some(relative) = entry.enclosed_name() else {
                continue;
            };
            let Some(relative) = relative.strip_prefix("worlds").ok().map(Path::to_path_buf) else {
                continue;
            };
            if relative.as_os_str().is_empty() {
                continue;
            }
            let target = saves.join(relative);
            if entry.is_dir() {
                fs::create_dir_all(target)?;
                continue;
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut output = fs::File::create(target)?;
            std::io::copy(&mut entry, &mut output)?;
        }
        Ok(())
    })
    .await
    .map_err(|error| AppError::new(error.to_string()))?
}

#[tauri::command]
pub fn delete_backup(backup_id: String, state: tauri::State<'_, crate::AppState>) -> Result<()> {
    let mut backups = state.store.backups()?;
    let backup = backups
        .iter()
        .find(|backup| backup.id == backup_id)
        .ok_or_else(|| AppError::new("Sauvegarde introuvable"))?
        .clone();
    if backup.path.exists() {
        fs::remove_file(&backup.path)?;
    }
    backups.retain(|backup| backup.id != backup_id);
    state.store.save_backups(&backups)?;
    Ok(())
}
