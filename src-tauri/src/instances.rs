use crate::error::{AppError, Result};
use crate::models::{Instance, InstanceInput, Loader};
use chrono::Utc;
use std::fs;

fn valid_version(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|char| char.is_ascii_alphanumeric() || matches!(char, '.' | '_' | '-'))
}

fn validate(input: &InstanceInput) -> Result<()> {
    if input.name.trim().len() < 2 || input.name.len() > 48 {
        return Err(AppError::new(
            "Le nom de l’instance doit contenir entre 2 et 48 caractères",
        ));
    }
    if !valid_version(input.version_id.trim()) {
        return Err(AppError::new("Version de Minecraft invalide"));
    }
    if input.loader != Loader::Vanilla
        && !valid_version(input.loader_version.as_deref().unwrap_or(""))
    {
        return Err(AppError::new("Version du modloader invalide"));
    }
    if input.memory_min < 256 || input.memory_max < input.memory_min || input.memory_max > 65536 {
        return Err(AppError::new("Allocation mémoire invalide"));
    }
    if input.resolution_width < 640 || input.resolution_height < 360 {
        return Err(AppError::new("La résolution minimale est de 640 × 360"));
    }
    Ok(())
}

#[tauri::command]
pub fn list_instances(state: tauri::State<'_, crate::AppState>) -> Result<Vec<Instance>> {
    state.store.instances()
}

#[tauri::command]
pub fn create_instance(
    input: InstanceInput,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Instance> {
    validate(&input)?;
    let instance = Instance {
        id: uuid::Uuid::new_v4().to_string(),
        name: input.name.trim().into(),
        version_id: input.version_id,
        loader: input.loader,
        loader_version: input.loader_version,
        color: input.color,
        memory_min: input.memory_min,
        memory_max: input.memory_max,
        resolution_width: input.resolution_width,
        resolution_height: input.resolution_height,
        fullscreen: input.fullscreen,
        close_on_launch: input.close_on_launch,
        java_path: input.java_path,
        jvm_arguments: input.jvm_arguments,
        server_address: input.server_address,
        modules: input.modules,
        created_at: Utc::now(),
        last_played_at: None,
        total_play_seconds: 0,
    };
    fs::create_dir_all(state.store.game_dir(&instance.id))?;
    let mut instances = state.store.instances()?;
    instances.push(instance.clone());
    state.store.save_instances(&instances)?;
    Ok(instance)
}

#[tauri::command]
pub fn update_instance(
    id: String,
    input: InstanceInput,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Instance> {
    validate(&input)?;
    let mut instances = state.store.instances()?;
    let existing = instances
        .iter_mut()
        .find(|instance| instance.id == id)
        .ok_or_else(|| AppError::new("Instance introuvable"))?;
    let (created_at, last_played_at, total_play_seconds) = (
        existing.created_at,
        existing.last_played_at,
        existing.total_play_seconds,
    );
    *existing = Instance {
        id,
        name: input.name.trim().into(),
        version_id: input.version_id,
        loader: input.loader,
        loader_version: input.loader_version,
        color: input.color,
        memory_min: input.memory_min,
        memory_max: input.memory_max,
        resolution_width: input.resolution_width,
        resolution_height: input.resolution_height,
        fullscreen: input.fullscreen,
        close_on_launch: input.close_on_launch,
        java_path: input.java_path,
        jvm_arguments: input.jvm_arguments,
        server_address: input.server_address,
        modules: input.modules,
        created_at,
        last_played_at,
        total_play_seconds,
    };
    let result = existing.clone();
    state.store.save_instances(&instances)?;
    Ok(result)
}

#[tauri::command]
pub fn duplicate_instance(
    id: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Instance> {
    let source = state.store.instance(&id)?;
    let destination_id = uuid::Uuid::new_v4().to_string();
    let destination = Instance {
        id: destination_id.clone(),
        name: format!("{} — copie", source.name),
        created_at: Utc::now(),
        last_played_at: None,
        total_play_seconds: 0,
        ..source
    };
    copy_directory(
        &state.store.game_dir(&id),
        &state.store.game_dir(&destination_id),
    )?;
    let mut instances = state.store.instances()?;
    instances.push(destination.clone());
    state.store.save_instances(&instances)?;
    Ok(destination)
}

fn copy_directory(source: &std::path::Path, destination: &std::path::Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    if !source.exists() {
        return Ok(());
    }
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

#[tauri::command]
pub fn delete_instance(
    id: String,
    delete_files: bool,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Vec<Instance>> {
    if state
        .processes
        .lock()
        .map_err(|_| AppError::new("Verrou du processus invalide"))?
        .contains_key(&id)
    {
        return Err(AppError::new(
            "Arrêtez le jeu avant de supprimer cette instance",
        ));
    }
    let mut instances = state.store.instances()?;
    if !instances.iter().any(|instance| instance.id == id) {
        return Err(AppError::new("Instance introuvable"));
    }
    instances.retain(|instance| instance.id != id);
    state.store.save_instances(&instances)?;
    if delete_files && state.store.instance_dir(&id).exists() {
        fs::remove_dir_all(state.store.instance_dir(&id))?;
    }
    Ok(instances)
}

#[tauri::command]
pub fn open_instance_folder(id: String, state: tauri::State<'_, crate::AppState>) -> Result<()> {
    let path = state.store.game_dir(&id);
    fs::create_dir_all(&path)?;
    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer").arg(path).spawn()?;
    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg(path).spawn()?;
    #[cfg(all(unix, not(target_os = "macos")))]
    std::process::Command::new("xdg-open").arg(path).spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::valid_version;

    #[test]
    fn validates_version_components() {
        assert!(valid_version("1.21.1"));
        assert!(valid_version("1.21.1-52.0.1"));
        assert!(valid_version("0.16.9-beta.1"));
        assert!(!valid_version("../outside"));
        assert!(!valid_version("1.21.1/evil"));
        assert!(!valid_version("1.21.1?url=evil"));
        assert!(!valid_version(""));
    }
}
