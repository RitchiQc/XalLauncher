use super::{
    api_key, cf_json, cf_post_json, cf_query, install_file, logo_url, numeric_id,
    project_type_from_class, CfFile, CfMod, DownloadUrlResponse, FileResponse, FilesResponse,
    ModResponse, API,
};
use crate::download::{download_file_with_checksum, Checksum};
use crate::error::{AppError, Result};
use crate::models::{Instance, Loader, ProgressEvent};
use crate::modpacks::{pack_relative, safe_relative};
use crate::store::Store;
use crate::transfer::{register_instance, sanitize_name, MAX_ENTRIES, MAX_TOTAL_BYTES};
use chrono::Utc;
use futures_util::{stream, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use zip::ZipArchive;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    #[serde(default)]
    manifest_type: String,
    name: Option<String>,
    minecraft: ManifestMinecraft,
    #[serde(default)]
    files: Vec<ManifestFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestMinecraft {
    version: String,
    #[serde(default)]
    mod_loaders: Vec<ManifestLoader>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestLoader {
    id: String,
    #[serde(default)]
    primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestFile {
    #[serde(rename = "projectID")]
    project_id: u64,
    #[serde(rename = "fileID")]
    file_id: u64,
    #[serde(default)]
    required: bool,
}

#[derive(Deserialize)]
struct ModsResponse {
    data: Vec<CfMod>,
}

fn loader_from_manifest(minecraft: &ManifestMinecraft) -> Result<(Loader, Option<String>)> {
    let Some(loader) = minecraft
        .mod_loaders
        .iter()
        .find(|loader| loader.primary)
        .or_else(|| minecraft.mod_loaders.first())
    else {
        return Ok((Loader::Vanilla, None));
    };
    let normalized = loader.id.to_ascii_lowercase();
    let (kind, raw) = if let Some(rest) = normalized.strip_prefix("fabric-loader-") {
        ("fabric", rest)
    } else if let Some(rest) = normalized.strip_prefix("quilt-loader-") {
        ("quilt", rest)
    } else {
        loader.id.split_once('-').ok_or_else(|| {
            AppError::new(format!("Modloader CurseForge invalide : {}", loader.id))
        })?
    };
    let raw = raw.to_string();
    let (kind, version) = if kind.eq_ignore_ascii_case("forge") {
        if raw.starts_with(&format!("{}-", minecraft.version)) {
            (Loader::Forge, raw)
        } else {
            (Loader::Forge, format!("{}-{raw}", minecraft.version))
        }
    } else if kind.eq_ignore_ascii_case("neoforge") {
        (Loader::Neoforge, raw)
    } else if kind.eq_ignore_ascii_case("fabric") {
        (Loader::Fabric, raw)
    } else if kind.eq_ignore_ascii_case("quilt") {
        (Loader::Quilt, raw)
    } else {
        return Err(AppError::new(format!(
            "Modloader CurseForge non pris en charge : {}",
            loader.id
        )));
    };
    Ok((kind, Some(version)))
}

fn read_manifest(archive: &mut ZipArchive<fs::File>, prefix: &str) -> Result<(Manifest, String)> {
    let direct = format!("{prefix}manifest.json");
    let name = if archive.by_name(&direct).is_ok() {
        direct
    } else {
        let mut names = Vec::new();
        for index in 0..archive.len() {
            names.push(archive.by_index(index)?.name()?.to_string());
        }
        names
            .into_iter()
            .filter(|name| name.ends_with("manifest.json"))
            .min_by_key(|name| name.matches('/').count())
            .ok_or_else(|| AppError::new("manifest.json CurseForge introuvable"))?
    };
    let prefix = name
        .strip_suffix("manifest.json")
        .unwrap_or_default()
        .to_string();
    let mut entry = archive
        .by_name(&name)
        .map_err(|_| AppError::new("manifest.json CurseForge introuvable"))?;
    let mut content = String::new();
    entry.read_to_string(&mut content)?;
    let manifest: Manifest = serde_json::from_str(&content)
        .map_err(|_| AppError::new("manifest.json CurseForge invalide"))?;
    if !manifest.manifest_type.is_empty() && manifest.manifest_type != "minecraftModpack" {
        return Err(AppError::new("Ce fichier n’est pas un modpack CurseForge"));
    }
    Ok((manifest, prefix))
}

fn extract_overrides(
    archive: &mut ZipArchive<fs::File>,
    prefix: &str,
    game_dir: &std::path::Path,
) -> Result<()> {
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        let path_string = path.to_string_lossy().replace('\\', "/");
        let Some(relative) = pack_relative(&path_string, prefix) else {
            continue;
        };
        let target = game_dir.join(safe_relative(relative)?);
        if entry.is_dir() {
            fs::create_dir_all(target)?;
            continue;
        }
        total = total.saturating_add(entry.size());
        if total > MAX_TOTAL_BYTES {
            return Err(AppError::new("Overrides CurseForge trop volumineux"));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::File::create(target)?;
        std::io::copy(&mut entry, &mut output)?;
    }
    Ok(())
}

pub async fn import_archive(
    client: &reqwest::Client,
    store: &Store,
    app: Option<&AppHandle>,
    archive_path: PathBuf,
    custom_name: Option<String>,
    archive_prefix: &str,
) -> Result<Instance> {
    let file = fs::File::open(&archive_path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|_| AppError::new("Archive CurseForge invalide ou corrompue"))?;
    if archive.len() > MAX_ENTRIES {
        return Err(AppError::new("Cette archive contient trop de fichiers"));
    }
    let (manifest, prefix) = read_manifest(&mut archive, archive_prefix)?;
    let (loader, loader_version) = loader_from_manifest(&manifest.minecraft)?;
    let id = uuid::Uuid::new_v4().to_string();
    let game_dir = store.game_dir(&id);
    fs::create_dir_all(&game_dir)?;
    let result = async {
        extract_overrides(&mut archive, &prefix, &game_dir)?;
        install_pack_files(client, store, app, &manifest, &game_dir).await
    }
    .await;
    if let Err(error) = result {
        let _ = fs::remove_dir_all(store.instance_dir(&id));
        return Err(error);
    }
    let name = custom_name
        .filter(|name| !name.trim().is_empty())
        .or_else(|| manifest.name.clone())
        .map(|name| sanitize_name(&name, "Modpack CurseForge"))
        .unwrap_or_else(|| "Modpack CurseForge".into());
    let instance = Instance {
        id,
        name,
        version_id: manifest.minecraft.version.clone(),
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
        java_major: None,
        jvm_arguments: String::new(),
        server_address: None,
        icon_url: None,
        icon_file: None,
        modules: Default::default(),
        created_at: Utc::now(),
        last_played_at: None,
        total_play_seconds: 0,
    };
    fs::write(
        store
            .instance_dir(&instance.id)
            .join("curseforge-manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    register_instance(store, instance)
}

async fn bulk_mods(
    client: &reqwest::Client,
    key: &str,
    ids: &[u64],
) -> Result<HashMap<u64, CfMod>> {
    let mut map = HashMap::new();
    for chunk in ids.chunks(50) {
        let response: ModsResponse = cf_post_json(
            client,
            key,
            "/mods",
            &json!({ "modIds": chunk }),
            "Mods du modpack CurseForge",
        )
        .await?;
        for project in response.data {
            map.insert(project.id, project);
        }
    }
    Ok(map)
}

async fn bulk_files(
    client: &reqwest::Client,
    key: &str,
    ids: &[u64],
) -> Result<HashMap<u64, CfFile>> {
    let mut map = HashMap::new();
    for chunk in ids.chunks(50) {
        let response: FilesResponse = cf_post_json(
            client,
            key,
            "/mods/files",
            &json!({ "fileIds": chunk }),
            "Fichiers du modpack CurseForge",
        )
        .await?;
        for file in response.data {
            map.insert(file.id, file);
        }
    }
    Ok(map)
}

async fn install_pack_files(
    client: &reqwest::Client,
    store: &Store,
    app: Option<&AppHandle>,
    manifest: &Manifest,
    game_dir: &std::path::Path,
) -> Result<()> {
    let required: Vec<&ManifestFile> = manifest.files.iter().filter(|file| file.required).collect();
    if required.is_empty() {
        return Ok(());
    }
    let key = api_key()?;
    let mut mod_ids = Vec::new();
    let mut file_ids = Vec::new();
    let mut seen_mods = std::collections::HashSet::new();
    let mut seen_files = std::collections::HashSet::new();
    for entry in &required {
        if seen_mods.insert(entry.project_id) {
            mod_ids.push(entry.project_id);
        }
        if seen_files.insert(entry.file_id) {
            file_ids.push(entry.file_id);
        }
    }
    let mods = bulk_mods(client, &key, &mod_ids).await?;
    let files = bulk_files(client, &key, &file_ids).await?;
    let mut jobs = Vec::new();
    let mut installed = std::collections::HashSet::new();
    for entry in required {
        if !installed.insert((entry.project_id, entry.file_id)) {
            continue;
        }
        let project = mods.get(&entry.project_id).ok_or_else(|| {
            AppError::new(format!(
                "Mod CurseForge {} introuvable dans l’API",
                entry.project_id
            ))
        })?;
        let file = files.get(&entry.file_id).ok_or_else(|| {
            AppError::new(format!(
                "Fichier CurseForge {} introuvable dans l’API",
                entry.file_id
            ))
        })?;
        if !file.is_available {
            return Err(AppError::new(format!(
                "Le fichier de {} n’est pas disponible",
                project.name
            )));
        }
        let project_type = project_type_from_class(project.class_id, "mod");
        jobs.push((project.clone(), file.clone(), project_type));
    }
    let total = jobs.len();
    let concurrency = store.settings()?.download_concurrency.clamp(1, 32);
    let completed = Arc::new(AtomicUsize::new(0));
    let _ = app.map(|app| {
        app.emit(
            "download-progress",
            ProgressEvent {
                operation: "modpack",
                label: "Fichiers du modpack CurseForge",
                current: 0,
                total: total as u64,
            },
        )
    });
    stream::iter(jobs.into_iter().map(|(project, file, project_type)| {
        let app = app.cloned();
        let completed = completed.clone();
        let key = key.clone();
        async move {
            install_file(client, &key, game_dir, &project, &file, &project_type).await?;
            let current = completed.fetch_add(1, Ordering::Relaxed) + 1;
            if let Some(app) = &app {
                let _ = app.emit(
                    "download-progress",
                    ProgressEvent {
                        operation: "modpack",
                        label: "Fichiers du modpack CurseForge",
                        current: current as u64,
                        total: total as u64,
                    },
                );
            }
            Ok::<(), AppError>(())
        }
    }))
    .buffer_unordered(concurrency)
    .try_collect::<Vec<()>>()
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn install_curseforge_modpack(
    project_id: String,
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Instance> {
    let key = api_key()?;
    let project_id = numeric_id(&project_id, "Identifiant du modpack CurseForge")?;
    let project: ModResponse = cf_json(
        &state.client,
        &key,
        &format!("{API}/mods/{project_id}"),
        "Modpack CurseForge",
    )
    .await?;
    let project = project.data;
    if project_type_from_class(project.class_id, "modpack") != "modpack" {
        return Err(AppError::new("Ce projet n’est pas un modpack"));
    }
    let file = match project.main_file_id.filter(|id| *id != 0) {
        Some(file_id) => {
            let file: FileResponse = cf_json(
                &state.client,
                &key,
                &format!("{API}/mods/{project_id}/files/{file_id}"),
                "Archive du modpack CurseForge",
            )
            .await?;
            file.data
        }
        None => {
            let files: FilesResponse = cf_query(
                &state.client,
                &key,
                &format!("/mods/{project_id}/files"),
                &[("pageSize", "1".to_string())],
                "Archive du modpack CurseForge",
            )
            .await?;
            files
                .data
                .into_iter()
                .next()
                .ok_or_else(|| AppError::new("Ce modpack n’a aucune archive"))?
        }
    };
    if !file.is_available {
        return Err(AppError::new(
            "L’archive de ce modpack n’est pas disponible",
        ));
    }
    if project.allow_mod_distribution == Some(false) {
        return Err(AppError::new(format!(
            "L’auteur de {} interdit le téléchargement via l’API CurseForge",
            project.name
        )));
    }
    let file_name = file.file_name.as_str();
    if file_name.is_empty() || file_name.contains(['/', '\\']) {
        return Err(AppError::new("Nom d’archive CurseForge dangereux"));
    }
    let url: DownloadUrlResponse = cf_json(
        &state.client,
        &key,
        &format!("{API}/mods/{project_id}/files/{}/download-url", file.id),
        "URL de l’archive CurseForge",
    )
    .await?;
    let url = url
        .data
        .filter(|url| url.starts_with("https://"))
        .ok_or_else(|| AppError::new("Aucune URL de téléchargement pour ce modpack"))?;
    let checksum = file
        .hashes
        .iter()
        .find(|hash| hash.algo == 1)
        .map(|hash| Checksum::Sha1(hash.value.clone()))
        .ok_or_else(|| AppError::new("Archive du modpack sans empreinte SHA-1"))?;
    let archive = state.store.root().join("cache").join(file_name);
    download_file_with_checksum(
        &state.client,
        &url,
        &archive,
        Some(&checksum),
        Some(&app),
        "modpack",
        &project.name,
    )
    .await?;
    let mut instance =
        import_archive(&state.client, &state.store, Some(&app), archive, None, "").await?;
    if let Some(icon) = logo_url(&project) {
        instance.icon_url = Some(icon.clone());
        let mut instances = state.store.instances()?;
        if let Some(stored) = instances.iter_mut().find(|item| item.id == instance.id) {
            stored.icon_url = Some(icon);
        }
        state.store.save_instances(&instances)?;
    }
    Ok(instance)
}

#[cfg(test)]
mod tests {
    use super::{loader_from_manifest, Manifest, ManifestLoader, ManifestMinecraft};
    use crate::models::Loader;

    fn minecraft(version: &str, loader: Option<&str>) -> ManifestMinecraft {
        ManifestMinecraft {
            version: version.into(),
            mod_loaders: loader
                .map(|id| {
                    vec![ManifestLoader {
                        id: id.into(),
                        primary: true,
                    }]
                })
                .unwrap_or_default(),
        }
    }

    #[test]
    fn maps_curseforge_loaders() {
        let (loader, version) =
            loader_from_manifest(&minecraft("1.20.1", Some("forge-47.2.0"))).unwrap();
        assert_eq!(loader, Loader::Forge);
        assert_eq!(version.as_deref(), Some("1.20.1-47.2.0"));

        let (loader, version) =
            loader_from_manifest(&minecraft("1.21.1", Some("fabric-loader-0.15.11"))).unwrap();
        assert_eq!(loader, Loader::Fabric);
        assert_eq!(version.as_deref(), Some("0.15.11"));

        let (loader, version) =
            loader_from_manifest(&minecraft("1.21.1", Some("neoforge-21.1.77"))).unwrap();
        assert_eq!(loader, Loader::Neoforge);
        assert_eq!(version.as_deref(), Some("21.1.77"));

        assert_eq!(
            loader_from_manifest(&minecraft("1.21.1", None)).unwrap(),
            (Loader::Vanilla, None)
        );
    }

    #[test]
    fn parses_curseforge_manifest() {
        let manifest: Manifest = serde_json::from_str(
            r#"{
                "manifestType": "minecraftModpack",
                "name": "Pack",
                "minecraft": {"version": "1.20.1", "modLoaders": [{"id": "forge-47.2.0", "primary": true}]},
                "files": [
                    {"projectID": 238222, "fileID": 5000, "required": true},
                    {"projectID": 1, "fileID": 2, "required": false}
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(manifest.name.as_deref(), Some("Pack"));
        assert_eq!(manifest.minecraft.version, "1.20.1");
        assert_eq!(manifest.files[0].project_id, 238222);
        assert!(manifest.files[0].required);
        assert!(!manifest.files[1].required);
    }
}
