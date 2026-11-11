use crate::download::{download_file_with_checksum, valid_checksum, Checksum};
use crate::error::{AppError, Result};
use crate::http::{ensure_success, get_json};
use crate::models::{Instance, Loader, ProgressEvent};
use crate::store::Store;
use chrono::Utc;
use futures_util::{stream, StreamExt, TryStreamExt};
use serde_json::Value;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, Emitter};

fn safe_relative(path: &str) -> Result<PathBuf> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(AppError::new("Chemin dangereux dans le modpack"));
    }
    Ok(path.to_path_buf())
}

fn loader_from_dependencies(dependencies: &Value, minecraft: &str) -> (Loader, Option<String>) {
    for (key, loader) in [
        ("fabric-loader", Loader::Fabric),
        ("quilt-loader", Loader::Quilt),
        ("neoforge", Loader::Neoforge),
        ("forge", Loader::Forge),
    ] {
        if let Some(version) = dependencies[key].as_str() {
            let version =
                if loader == Loader::Forge && !version.starts_with(&format!("{minecraft}-")) {
                    format!("{minecraft}-{version}")
                } else {
                    version.to_string()
                };
            return (loader, Some(version));
        }
    }
    (Loader::Vanilla, None)
}

async fn import_pack(
    client: reqwest::Client,
    store: Store,
    app: AppHandle,
    archive_path: PathBuf,
    custom_name: Option<String>,
) -> Result<Instance> {
    let file = fs::File::open(&archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let index = {
        let mut file = archive
            .by_name("modrinth.index.json")
            .map_err(|_| AppError::new("Cette archive n’est pas un modpack Modrinth valide"))?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        serde_json::from_str::<Value>(&content)?
    };
    let minecraft = index["dependencies"]["minecraft"]
        .as_str()
        .ok_or_else(|| AppError::new("Version Minecraft absente du modpack"))?
        .to_string();
    let (loader, loader_version) = loader_from_dependencies(&index["dependencies"], &minecraft);
    let id = uuid::Uuid::new_v4().to_string();
    let game_dir = store.game_dir(&id);
    fs::create_dir_all(&game_dir)?;
    let result = install_pack_files(&client, &store, &app, &game_dir, &mut archive, &index).await;
    if let Err(error) = result {
        let _ = fs::remove_dir_all(store.instance_dir(&id));
        return Err(error);
    }
    let name = custom_name
        .filter(|name| !name.trim().is_empty())
        .or_else(|| index["name"].as_str().map(str::to_string))
        .unwrap_or_else(|| "Modpack importé".into());
    let instance = Instance {
        id,
        name,
        version_id: minecraft,
        loader,
        loader_version,
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
        modules: Default::default(),
        created_at: Utc::now(),
        last_played_at: None,
        total_play_seconds: 0,
    };
    fs::write(
        store.instance_dir(&instance.id).join("modpack.json"),
        serde_json::to_vec_pretty(&index)?,
    )?;
    let mut instances = store.instances()?;
    instances.push(instance.clone());
    store.save_instances(&instances)?;
    Ok(instance)
}

async fn install_pack_files(
    client: &reqwest::Client,
    store: &Store,
    app: &AppHandle,
    game_dir: &Path,
    archive: &mut zip::ZipArchive<fs::File>,
    index: &Value,
) -> Result<()> {
    for number in 0..archive.len() {
        let mut entry = archive.by_index(number)?;
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        let path_string = path.to_string_lossy().replace('\\', "/");
        let prefix = ["overrides/", "client-overrides/"]
            .into_iter()
            .find(|prefix| path_string.starts_with(prefix));
        let Some(prefix) = prefix else {
            continue;
        };
        let relative = &path_string[prefix.len()..];
        if relative.is_empty() {
            continue;
        }
        let target = game_dir.join(safe_relative(relative)?);
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
    let mut jobs = Vec::new();
    for file in index["files"].as_array().into_iter().flatten() {
        if file["env"]["client"] == "unsupported" {
            continue;
        }
        let path = file["path"]
            .as_str()
            .ok_or_else(|| AppError::new("Fichier du modpack sans chemin"))?
            .to_string();
        let url = file["downloads"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .next()
            .ok_or_else(|| AppError::new("Fichier du modpack sans URL"))?
            .to_string();
        let checksum = file["hashes"]["sha512"]
            .as_str()
            .map(|hash| Checksum::Sha512(hash.to_string()))
            .or_else(|| {
                file["hashes"]["sha1"]
                    .as_str()
                    .map(|hash| Checksum::Sha1(hash.to_string()))
            })
            .ok_or_else(|| AppError::new(format!("Fichier du modpack sans checksum : {path}")))?;
        jobs.push((path, url, checksum));
    }
    for (path, _, _) in &jobs {
        safe_relative(path)?;
    }
    let total = jobs.len();
    let completed = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let concurrency = store.settings()?.download_concurrency.clamp(1, 32);
    stream::iter(jobs.into_iter().map(|(path, url, checksum)| {
        let client = client.clone();
        let app = app.clone();
        let completed = completed.clone();
        let game_dir = game_dir.to_path_buf();
        async move {
            let destination = game_dir.join(safe_relative(&path)?);
            download_file_with_checksum(
                &client,
                &url,
                &destination,
                Some(&checksum),
                None,
                "modpack",
                &path,
            )
            .await?;
            let current = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            let _ = app.emit(
                "download-progress",
                ProgressEvent {
                    operation: "modpack",
                    label: "Fichiers du modpack",
                    current: current as u64,
                    total: total as u64,
                },
            );
            Ok::<(), AppError>(())
        }
    }))
    .buffer_unordered(concurrency)
    .try_collect::<Vec<()>>()
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn import_modrinth_pack(
    path: PathBuf,
    name: Option<String>,
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Instance> {
    import_pack(state.client.clone(), state.store.clone(), app, path, name).await
}

#[tauri::command]
pub async fn install_modrinth_modpack(
    project_id: String,
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Instance> {
    let project: Value = get_json(
        &state.client,
        &format!("https://api.modrinth.com/v2/project/{project_id}"),
    )
    .await?;
    if project["project_type"] != "modpack" {
        return Err(AppError::new("Ce projet n’est pas un modpack"));
    }
    let versions: Vec<Value> = get_json(
        &state.client,
        &format!("https://api.modrinth.com/v2/project/{project_id}/version"),
    )
    .await?;
    let version = versions
        .first()
        .ok_or_else(|| AppError::new("Ce modpack n’a aucune publication"))?;
    let files = version["files"]
        .as_array()
        .ok_or_else(|| AppError::new("Publication sans archive"))?;
    let file = files
        .iter()
        .find(|file| file["primary"] == true)
        .or_else(|| files.first())
        .ok_or_else(|| AppError::new("Publication sans archive"))?;
    let name = file["filename"]
        .as_str()
        .ok_or_else(|| AppError::new("Archive sans nom"))?;
    if name.contains(['/', '\\']) {
        return Err(AppError::new("Nom d’archive distant dangereux"));
    }
    let archive = state.store.root().join("cache").join(name);
    let response = state
        .client
        .get(file["url"].as_str().unwrap_or(""))
        .send()
        .await?;
    let response = ensure_success(response, "Téléchargement du modpack").await?;
    let bytes = response.bytes().await?;
    if let Some(parent) = archive.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&archive, bytes)?;
    let checksum = file["hashes"]["sha512"]
        .as_str()
        .map(|hash| Checksum::Sha512(hash.to_string()))
        .or_else(|| {
            file["hashes"]["sha1"]
                .as_str()
                .map(|hash| Checksum::Sha1(hash.to_string()))
        })
        .ok_or_else(|| AppError::new("Archive du modpack sans checksum"))?;
    if !valid_checksum(&archive, Some(&checksum)) {
        let _ = fs::remove_file(&archive);
        return Err(AppError::new(format!(
            "Somme {} du modpack incorrecte",
            checksum.name()
        )));
    }
    import_pack(
        state.client.clone(),
        state.store.clone(),
        app,
        archive,
        None,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::safe_relative;

    #[test]
    fn rejects_archive_traversal() {
        assert!(safe_relative("mods/example.jar").is_ok());
        assert!(safe_relative("../options.txt").is_err());
        assert!(safe_relative("/absolute/file.jar").is_err());
        assert!(safe_relative("mods/../options.txt").is_err());
    }
}
