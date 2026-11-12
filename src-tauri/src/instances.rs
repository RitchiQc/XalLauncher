use crate::error::{AppError, Result};
use crate::models::{Instance, InstanceInput, Loader};
use chrono::Utc;
use std::fs;

const ICON_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp"];
const MAX_ICON_BYTES: u64 = 8 * 1024 * 1024;

pub(crate) fn valid_version(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|char| char.is_ascii_alphanumeric() || matches!(char, '.' | '_' | '-'))
}

fn valid_icon_name(value: &str) -> bool {
    !value.is_empty()
        && !value.contains(['/', '\\'])
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
        && value
            .rsplit_once('.')
            .map(|(_, extension)| {
                ICON_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
            })
            .unwrap_or(false)
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
        icon_url: None,
        icon_file: None,
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
    let (created_at, last_played_at, total_play_seconds, icon_url, icon_file) = (
        existing.created_at,
        existing.last_played_at,
        existing.total_play_seconds,
        existing.icon_url.clone(),
        existing.icon_file.clone(),
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
        icon_url,
        icon_file,
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
    fs::create_dir_all(state.store.instance_dir(&destination_id))?;
    if let Some(icon) = destination
        .icon_file
        .as_deref()
        .filter(|name| valid_icon_name(name))
    {
        let source_icon = state.store.instance_dir(&id).join(icon);
        if source_icon.is_file() {
            fs::copy(
                source_icon,
                state.store.instance_dir(&destination_id).join(icon),
            )?;
        }
    }
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

#[tauri::command]
pub fn set_instance_icon(
    id: String,
    source_path: Option<String>,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Instance> {
    let mut instances = state.store.instances()?;
    let index = instances
        .iter()
        .position(|instance| instance.id == id)
        .ok_or_else(|| AppError::new("Instance introuvable"))?;
    let directory = state.store.instance_dir(&id);
    fs::create_dir_all(&directory)?;
    let icon_file = match source_path {
        Some(path) => {
            let source = std::path::PathBuf::from(path);
            let extension = source
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.to_ascii_lowercase())
                .unwrap_or_default();
            if !ICON_EXTENSIONS.contains(&extension.as_str()) {
                return Err(AppError::new(
                    "Format d'image non pris en charge (PNG, JPEG ou WebP)",
                ));
            }
            let metadata = fs::metadata(&source)
                .map_err(|_| AppError::new("Image de l'instance introuvable"))?;
            if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_ICON_BYTES {
                return Err(AppError::new(
                    "L'image de l'instance doit faire moins de 8 Mo",
                ));
            }
            let target_name = format!("icon-{}.{extension}", uuid::Uuid::new_v4());
            fs::copy(&source, directory.join(&target_name))?;
            Some(target_name)
        }
        None => None,
    };
    if let Some(previous) = instances[index]
        .icon_file
        .as_deref()
        .filter(|name| valid_icon_name(name))
    {
        let _ = fs::remove_file(directory.join(previous));
    }
    instances[index].icon_file = icon_file;
    instances[index].icon_url = None;
    let result = instances[index].clone();
    state.store.save_instances(&instances)?;
    Ok(result)
}

#[tauri::command]
pub fn instance_icon_path(
    id: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Option<String>> {
    let instance = state.store.instance(&id)?;
    let Some(file) = instance
        .icon_file
        .as_deref()
        .filter(|name| valid_icon_name(name))
    else {
        return Ok(None);
    };
    let path = state.store.instance_dir(&id).join(file);
    Ok(path.is_file().then(|| path.to_string_lossy().to_string()))
}

#[cfg(test)]
mod tests {
    use super::{valid_icon_name, valid_version};

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

    #[test]
    fn validates_instance_icon_names() {
        assert!(valid_icon_name("icon-abc.png"));
        assert!(valid_icon_name("cover.webp"));
        assert!(!valid_icon_name("../outside.png"));
        assert!(!valid_icon_name("icon.exe"));
        assert!(!valid_icon_name(""));
    }
}
