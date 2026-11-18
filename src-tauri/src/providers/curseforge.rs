use crate::download::{download_file_with_checksum, Checksum};
use crate::error::{AppError, Result};
use crate::http::ensure_success;
use crate::models::{ContentEntry, Instance, Loader, ProgressEvent, ProjectHit};
use crate::content_meta::ContentMetadata;
use crate::providers::common;
use crate::providers::modrinth::ModrinthVersion;
use futures_util::{stream, StreamExt, TryStreamExt};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::{AppHandle, Emitter};

const API: &str = "https://api.curseforge.com/v1";
const GAME_ID: u64 = 432;
const MOD_CLASS: u64 = 6;
const RESOURCEPACK_CLASS: u64 = 12;
const SHADER_CLASS: u64 = 6552;
const MODPACK_CLASS: u64 = 4471;

pub mod pack;

#[derive(Deserialize)]
struct SearchResponse {
    data: Vec<CfMod>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfMod {
    id: u64,
    name: String,
    slug: String,
    summary: String,
    #[serde(default)]
    authors: Vec<CfAuthor>,
    #[serde(default)]
    logo: Option<CfAsset>,
    #[serde(default)]
    download_count: u64,
    #[serde(default)]
    thumbs_up_count: u64,
    #[serde(default)]
    class_id: Option<u64>,
    #[serde(default)]
    latest_files_indexes: Vec<CfFileIndex>,
    #[serde(default)]
    main_file_id: Option<u64>,
    #[serde(default)]
    allow_mod_distribution: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct CfAuthor {
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfAsset {
    thumbnail_url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfFileIndex {
    game_version: String,
}

#[derive(Deserialize)]
struct ModResponse {
    data: CfMod,
}

#[derive(Deserialize)]
struct FilesResponse {
    data: Vec<CfFile>,
}

#[derive(Deserialize)]
struct FileResponse {
    data: CfFile,
}

#[derive(Deserialize)]
struct DownloadUrlResponse {
    data: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfFile {
    id: u64,
    file_name: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    release_type: u64,
    #[serde(default)]
    file_date: String,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    hashes: Vec<CfHash>,
    #[serde(default)]
    dependencies: Vec<CfDependency>,
    #[serde(default)]
    is_available: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct CfHash {
    value: String,
    algo: u64,
}

fn key_status() -> Result<CurseforgeKeyStatus> {
    if crate::secrets::curseforge_builtin_key().is_some() {
        return Ok(CurseforgeKeyStatus {
            configured: true,
            source: Some("builtin".into()),
        });
    }
    if crate::secrets::curseforge_env_key().is_some() {
        return Ok(CurseforgeKeyStatus {
            configured: true,
            source: Some("environment".into()),
        });
    }
    let configured = crate::secrets::curseforge_vault_key()?.is_some();
    Ok(CurseforgeKeyStatus {
        configured,
        source: configured.then(|| "vault".into()),
    })
}

pub(crate) fn api_key() -> Result<String> {
    crate::secrets::curseforge_key()?.ok_or_else(|| {
        AppError::new(
            "Cette version de XalLauncher ne contient pas de clé CurseForge. Utilisez une release officielle ou ajoutez une clé dans Paramètres > CurseForge.",
        )
    })
}

async fn response_json<T: DeserializeOwned>(
    response: reqwest::Response,
    action: &str,
) -> Result<T> {
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(AppError::new(
            "Accès CurseForge refusé : vérifiez la clé API ou la distribution autorisée par l’auteur.",
        ));
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(AppError::new(
            "Limite de requêtes CurseForge atteinte. Réessayez dans une minute.",
        ));
    }
    ensure_success(response, action)
        .await?
        .json()
        .await
        .map_err(Into::into)
}

async fn cf_json<T: DeserializeOwned>(
    client: &reqwest::Client,
    key: &str,
    url: &str,
    action: &str,
) -> Result<T> {
    let response = client
        .get(url)
        .header("x-api-key", key)
        .header("Accept", "application/json")
        .send()
        .await?;
    response_json(response, action).await
}

async fn cf_query<T: DeserializeOwned>(
    client: &reqwest::Client,
    key: &str,
    path: &str,
    query: &[(&str, String)],
    action: &str,
) -> Result<T> {
    let response = client
        .get(format!("{API}{path}"))
        .query(query)
        .header("x-api-key", key)
        .header("Accept", "application/json")
        .send()
        .await?;
    response_json(response, action).await
}

pub(crate) async fn cf_post_json<T: DeserializeOwned>(
    client: &reqwest::Client,
    key: &str,
    path: &str,
    body: &Value,
    action: &str,
) -> Result<T> {
    let response = client
        .post(format!("{API}{path}"))
        .json(body)
        .header("x-api-key", key)
        .header("Accept", "application/json")
        .send()
        .await?;
    response_json(response, action).await
}

fn class_id(project_type: &str) -> Result<u64> {
    match project_type {
        "mod" => Ok(MOD_CLASS),
        "resourcepack" => Ok(RESOURCEPACK_CLASS),
        "shader" => Ok(SHADER_CLASS),
        "modpack" => Ok(MODPACK_CLASS),
        _ => Err(AppError::new(
            "Type de contenu CurseForge non pris en charge",
        )),
    }
}

fn project_type_from_class(class_id: Option<u64>, fallback: &str) -> String {
    match class_id {
        Some(MOD_CLASS) => "mod",
        Some(RESOURCEPACK_CLASS) => "resourcepack",
        Some(SHADER_CLASS) => "shader",
        Some(MODPACK_CLASS) => "modpack",
        _ => fallback,
    }
    .to_string()
}

fn loader_type(loader: Loader) -> Option<u64> {
    match loader {
        Loader::Forge => Some(1),
        Loader::Fabric => Some(4),
        Loader::Quilt => Some(5),
        Loader::Neoforge => Some(6),
        Loader::Vanilla | Loader::Horizon => None,
    }
}

fn loader_token(loader: Loader) -> Option<&'static str> {
    match loader {
        Loader::Forge => Some("Forge"),
        Loader::Fabric => Some("Fabric"),
        Loader::Quilt => Some("Quilt"),
        Loader::Neoforge => Some("NeoForge"),
        Loader::Vanilla | Loader::Horizon => None,
    }
}

fn sort_field(index: Option<&str>) -> Option<u64> {
    match index {
        Some("downloads") => Some(6),
        Some("follows") => Some(2),
        Some("newest") | Some("updated") => Some(3),
        _ => None,
    }
}

fn is_game_version(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
}

fn file_loaders(file: &CfFile) -> Vec<String> {
    ["Forge", "Fabric", "Quilt", "NeoForge"]
        .into_iter()
        .filter(|token| {
            file.game_versions
                .iter()
                .any(|value| value.eq_ignore_ascii_case(token))
        })
        .map(str::to_string)
        .collect()
}

fn file_matches_instance(file: &CfFile, instance: &Instance, project_type: &str) -> bool {
    if !file.is_available
        || !file
            .game_versions
            .iter()
            .any(|version| version == &instance.version_id)
    {
        return false;
    }
    if project_type != "mod" {
        return true;
    }
    loader_token(instance.loader).is_none_or(|token| {
        file.game_versions
            .iter()
            .any(|value| value.eq_ignore_ascii_case(token))
    })
}

fn logo_url(project: &CfMod) -> Option<String> {
    project
        .logo
        .as_ref()
        .map(|logo| logo.thumbnail_url.clone())
        .filter(|url| url.starts_with("https://"))
}

fn version_summary(file: &CfFile) -> ModrinthVersion {
    let label = file
        .display_name
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| file.file_name.clone());
    ModrinthVersion {
        id: file.id.to_string(),
        name: label.clone(),
        version_number: label,
        version_type: match file.release_type {
            2 => "beta",
            3 => "alpha",
            _ => "release",
        }
        .into(),
        date_published: file.file_date.clone(),
        game_versions: file
            .game_versions
            .iter()
            .filter(|value| is_game_version(value))
            .cloned()
            .collect(),
        loaders: file_loaders(file),
        file_name: Some(file.file_name.clone()),
    }
}

#[tauri::command]
pub fn curseforge_key_status() -> Result<CurseforgeKeyStatus> {
    key_status()
}

#[tauri::command]
pub fn set_curseforge_api_key(key: String) -> Result<CurseforgeKeyStatus> {
    crate::secrets::set_curseforge_key(&key)?;
    key_status()
}

#[tauri::command]
pub fn clear_curseforge_api_key() -> Result<CurseforgeKeyStatus> {
    crate::secrets::clear_curseforge_key()?;
    key_status()
}

#[tauri::command]
pub async fn test_curseforge_api_key(state: tauri::State<'_, crate::AppState>) -> Result<()> {
    let key = api_key()?;
    let url = format!("{API}/games/{GAME_ID}");
    let _: Value = cf_json(&state.client, &key, &url, "Validation de la clé CurseForge").await?;
    Ok(())
}

#[tauri::command]
pub async fn search_curseforge(
    query: String,
    project_type: String,
    version_id: Option<String>,
    loader: Option<Loader>,
    offset: Option<u32>,
    index: Option<String>,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Vec<ProjectHit>> {
    let key = api_key()?;
    let class = class_id(&project_type)?;
    let mut params = vec![
        ("gameId", GAME_ID.to_string()),
        ("classId", class.to_string()),
        ("index", offset.unwrap_or(0).to_string()),
        ("pageSize", "24".to_string()),
    ];
    if let Some(version) = version_id.filter(|value| !value.is_empty()) {
        params.push(("gameVersion", version));
    }
    if project_type == "mod" {
        if let Some(loader) = loader.and_then(loader_type) {
            params.push(("modLoaderType", loader.to_string()));
        }
    }
    if !query.trim().is_empty() {
        params.push(("searchFilter", query));
    }
    if let Some(field) = sort_field(index.as_deref()) {
        params.push(("sortField", field.to_string()));
        params.push(("sortOrder", "desc".into()));
    }
    let response: SearchResponse = cf_query(
        &state.client,
        &key,
        "/mods/search",
        &params,
        "Recherche CurseForge",
    )
    .await?;
    Ok(response
        .data
        .into_iter()
        .map(|project| {
            let mut versions = Vec::new();
            for entry in &project.latest_files_indexes {
                if is_game_version(&entry.game_version) && !versions.contains(&entry.game_version) {
                    versions.push(entry.game_version.clone());
                }
            }
            let icon_url = logo_url(&project);
            let author = project
                .authors
                .first()
                .map(|author| author.name.clone())
                .unwrap_or_else(|| "Auteur inconnu".into());
            let project_type = project_type_from_class(project.class_id, &project_type);
            ProjectHit {
                project_id: project.id.to_string(),
                slug: project.slug,
                title: project.name,
                description: project.summary,
                author,
                icon_url,
                downloads: project.download_count,
                follows: project.thumbs_up_count,
                project_type,
                versions,
                provider: Some("curseforge".into()),
            }
        })
        .collect())
}

fn numeric_id(value: &str, label: &str) -> Result<u64> {
    value
        .parse::<u64>()
        .map_err(|_| AppError::new(format!("{label} invalide")))
}

#[tauri::command]
pub async fn list_curseforge_versions(
    project_id: String,
    instance_id: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Vec<ModrinthVersion>> {
    let key = api_key()?;
    let project_id = numeric_id(&project_id, "Identifiant du projet CurseForge")?;
    let instance = state.store.instance(&instance_id)?;
    let url = format!("{API}/mods/{project_id}");
    let project: ModResponse = cf_json(&state.client, &key, &url, "Projet CurseForge").await?;
    let project_type = project_type_from_class(project.data.class_id, "mod");
    if project_type == "modpack" {
        return Err(AppError::new(
            "Les modpacks ne s’installent pas dans une instance existante",
        ));
    }
    if project_type == "mod" && instance.loader == Loader::Vanilla {
        return Err(AppError::new(
            "Un modloader est requis pour installer ce mod",
        ));
    }
    let mut params = vec![
        ("gameVersion", instance.version_id.clone()),
        ("index", "0".to_string()),
        ("pageSize", "50".to_string()),
    ];
    if project_type == "mod" {
        if let Some(loader) = loader_type(instance.loader) {
            params.push(("modLoaderType", loader.to_string()));
        }
    }
    let files: FilesResponse = cf_query(
        &state.client,
        &key,
        &format!("/mods/{project_id}/files"),
        &params,
        "Versions CurseForge",
    )
    .await?;
    Ok(files
        .data
        .into_iter()
        .filter(|file| file_matches_instance(file, &instance, &project_type))
        .map(|file| version_summary(&file))
        .collect())
}

async fn project_version(
    state: &crate::AppState,
    key: &str,
    instance: &Instance,
    project_id: u64,
    requested_version: Option<&str>,
) -> Result<(CfMod, CfFile, String)> {
    let url = format!("{API}/mods/{project_id}");
    let project: ModResponse = cf_json(&state.client, key, &url, "Projet CurseForge").await?;
    let project_type = project_type_from_class(project.data.class_id, "mod");
    if project_type == "modpack" {
        return Err(AppError::new(
            "Les modpacks s’installent depuis leur page avec le bouton dédié",
        ));
    }
    if project_type == "mod" && instance.loader == Loader::Vanilla {
        return Err(AppError::new(
            "Un modloader est requis pour installer ce mod",
        ));
    }
    if let Some(file_id) = requested_version {
        let file_id = numeric_id(file_id, "Identifiant de fichier CurseForge")?;
        let file: FileResponse = cf_json(
            &state.client,
            key,
            &format!("{API}/mods/{project_id}/files/{file_id}"),
            "Fichier CurseForge",
        )
        .await?;
        if !file_matches_instance(&file.data, instance, &project_type) {
            return Err(AppError::new(format!(
                "La version sélectionnée de {} n’est pas compatible avec cette instance",
                project.data.name
            )));
        }
        return Ok((project.data, file.data, project_type));
    }
    let mut params = vec![
        ("gameVersion", instance.version_id.clone()),
        ("index", "0".to_string()),
        ("pageSize", "50".to_string()),
    ];
    if project_type == "mod" {
        if let Some(loader) = loader_type(instance.loader) {
            params.push(("modLoaderType", loader.to_string()));
        }
    }
    let files: FilesResponse = cf_query(
        &state.client,
        key,
        &format!("/mods/{project_id}/files"),
        &params,
        "Versions CurseForge",
    )
    .await?;
    let file = files
        .data
        .into_iter()
        .find(|file| file_matches_instance(file, instance, &project_type))
        .ok_or_else(|| {
            AppError::new(format!(
                "Aucune version compatible pour {}",
                project.data.name
            ))
        })?;
    Ok((project.data, file, project_type))
}

struct InstallJob {
    project: CfMod,
    file: CfFile,
    project_type: String,
}

fn resolve_project_tree<'a>(
    state: &'a crate::AppState,
    key: &'a str,
    instance_id: &'a str,
    project_id: u64,
    version_id: Option<&'a str>,
    visited: &'a mut HashSet<u64>,
    jobs: &'a mut Vec<InstallJob>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<ContentEntry>>> + Send + 'a>>
{
    Box::pin(async move {
        if !visited.insert(project_id) {
            return Ok(None);
        }
        if let Some(installed) = common::installed_content(
            &state.store,
            instance_id,
            "curseforge",
            &project_id.to_string(),
            version_id,
        ) {
            return Ok(Some(installed));
        }
        let instance = state.store.instance(instance_id)?;
        let (project, file, project_type) =
            project_version(state, key, &instance, project_id, version_id).await?;
        let dependencies = file.dependencies.clone();
        for dependency in dependencies
            .into_iter()
            .filter(|dependency| dependency.relation_type == 3 && dependency.mod_id != 0)
        {
            resolve_project_tree(
                state,
                key,
                instance_id,
                dependency.mod_id,
                None,
                visited,
                jobs,
            )
            .await?;
        }
        jobs.push(InstallJob {
            project,
            file,
            project_type,
        });
        Ok(None)
    })
}

async fn install_version(
    state: &crate::AppState,
    key: &str,
    instance_id: &str,
    project: &CfMod,
    file: &CfFile,
    project_type: &str,
) -> Result<ContentEntry> {
    let instance_dir = state.store.instance_dir(instance_id);
    install_file(&state.client, key, &instance_dir, project, file, project_type).await
}

async fn install_file(
    client: &reqwest::Client,
    key: &str,
    instance_dir: &Path,
    project: &CfMod,
    file: &CfFile,
    project_type: &str,
) -> Result<ContentEntry> {
    let game_dir = instance_dir.join("game");
    if project.allow_mod_distribution == Some(false) {
        return Err(AppError::new(format!(
            "L’auteur de {} interdit le téléchargement via l’API CurseForge",
            project.name
        )));
    }
    let file_name = file.file_name.as_str();
    if file_name.is_empty() || file_name.contains(['/', '\\']) {
        return Err(AppError::new("Nom de fichier distant dangereux"));
    }
    let folder = match project_type {
        "mod" => "mods",
        "resourcepack" => "resourcepacks",
        "shader" => "shaderpacks",
        _ => return Err(AppError::new("Type de contenu non pris en charge")),
    };
    let destination = game_dir.join(folder).join(file_name);
    let url: DownloadUrlResponse = cf_json(
        client,
        key,
        &format!("{API}/mods/{}/files/{}/download-url", project.id, file.id),
        "URL de téléchargement CurseForge",
    )
    .await?;
    let url = url
        .data
        .filter(|url| url.starts_with("https://"))
        .ok_or_else(|| {
            AppError::new(format!(
                "Aucune URL de téléchargement CurseForge pour {}",
                project.name
            ))
        })?;
    let checksum = file
        .hashes
        .iter()
        .find(|hash| hash.algo == 1)
        .map(|hash| Checksum::Sha1(hash.value.clone()))
        .ok_or_else(|| {
            AppError::new(format!(
                "Fichier CurseForge sans empreinte SHA-1 pour {}",
                project.name
            ))
        })?;
    download_file_with_checksum(
        client,
        &url,
        &destination,
        Some(&checksum),
        None,
        "content",
        &project.name,
    )
    .await?;
    let project_id = project.id.to_string();
    common::remove_previous_versions(instance_dir, folder, "curseforge", &project_id, file_name);
    let metadata = ContentMetadata {
        project_id,
        project_title: project.name.clone(),
        version_id: file.id.to_string(),
        icon_url: logo_url(project),
        provider: Some("curseforge".into()),
    };
    crate::content_meta::write(instance_dir, folder, file_name, &metadata)?;
    Ok(ContentEntry {
        file_name: file_name.into(),
        kind: project_type.into(),
        enabled: true,
        size: destination.metadata()?.len(),
        modified_at: chrono::Utc::now(),
        project_id: Some(metadata.project_id),
        project_title: Some(metadata.project_title),
        version_id: Some(metadata.version_id),
        icon_url: metadata.icon_url,
        provider: metadata.provider,
    })
}

#[tauri::command]
pub async fn install_curseforge_project(
    project_id: String,
    instance_id: String,
    version_id: Option<String>,
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<ContentEntry> {
    let key = api_key()?;
    let project_id = numeric_id(&project_id, "Identifiant du projet CurseForge")?;
    let mut visited = HashSet::new();
    let mut jobs = Vec::new();
    let existing = resolve_project_tree(
        &state,
        &key,
        &instance_id,
        project_id,
        version_id.as_deref(),
        &mut visited,
        &mut jobs,
    )
    .await?;
    if jobs.is_empty() {
        return existing.ok_or_else(|| AppError::new("Ce contenu est déjà installé"));
    }
    let targets: Vec<(String, String)> = jobs
        .iter()
        .map(|job| (job.project_type.clone(), job.project.id.to_string()))
        .collect();
    let total = jobs.len();
    let concurrency = state.store.settings()?.download_concurrency.clamp(1, 32);
    let completed = std::sync::Arc::new(AtomicUsize::new(0));
    let _ = app.emit(
        "download-progress",
        ProgressEvent {
            operation: "content",
            label: "Installation des contenus CurseForge",
            current: 0,
            total: total as u64,
        },
    );
    let state_ref = &state;
    let key_ref = key.as_str();
    let instance_id_ref = instance_id.as_str();
    let entries = stream::iter(jobs.into_iter().map(|job| {
        let app = app.clone();
        let completed = completed.clone();
        async move {
            let entry = install_version(
                state_ref,
                key_ref,
                instance_id_ref,
                &job.project,
                &job.file,
                &job.project_type,
            )
            .await?;
            let current = completed.fetch_add(1, Ordering::Relaxed) + 1;
            let _ = app.emit(
                "download-progress",
                ProgressEvent {
                    operation: "content",
                    label: "Installation des contenus CurseForge",
                    current: current as u64,
                    total: total as u64,
                },
            );
            Ok::<ContentEntry, AppError>(entry)
        }
    }))
    .buffer_unordered(concurrency)
    .try_collect::<Vec<_>>()
    .await?;
    if let Err(error) = crate::providers::cross::remove_duplicates(
        &state,
        &instance_id,
        "curseforge",
        &targets,
    )
    .await
    {
        tracing::warn!("Déduplication inter-plateformes ignorée : {error}");
    }
    let main_id = project_id.to_string();
    entries
        .into_iter()
        .find(|entry| entry.project_id.as_deref() == Some(main_id.as_str()))
        .or(existing)
        .ok_or_else(|| AppError::new("Contenu installé mais introuvable"))
}

#[cfg(test)]
mod tests {
    use super::{
        class_id, is_game_version, loader_token, project_type_from_class, sort_field,
        version_summary, CfFile,
    };
    use crate::models::Loader;

    #[test]
    fn maps_curseforge_classes_and_loaders() {
        assert_eq!(class_id("mod").unwrap(), 6);
        assert_eq!(class_id("resourcepack").unwrap(), 12);
        assert_eq!(class_id("shader").unwrap(), 6552);
        assert_eq!(class_id("modpack").unwrap(), 4471);
        assert!(class_id("world").is_err());
        assert_eq!(project_type_from_class(Some(12), "mod"), "resourcepack");
        assert_eq!(project_type_from_class(Some(6552), "mod"), "shader");
        assert_eq!(project_type_from_class(None, "mod"), "mod");
        assert_eq!(loader_token(Loader::Neoforge), Some("NeoForge"));
        assert_eq!(loader_token(Loader::Vanilla), None);
    }

    #[test]
    fn maps_search_sorting() {
        assert_eq!(sort_field(Some("downloads")), Some(6));
        assert_eq!(sort_field(Some("follows")), Some(2));
        assert_eq!(sort_field(Some("newest")), Some(3));
        assert_eq!(sort_field(Some("relevance")), None);
    }

    #[test]
    fn summarizes_curseforge_file() {
        let file = CfFile {
            id: 5000,
            file_name: "jei-1.21.1.jar".into(),
            display_name: Some("JEI 19.0".into()),
            release_type: 2,
            file_date: "2025-03-01T00:00:00Z".into(),
            game_versions: vec!["1.21.1".into(), "Fabric".into(), "Client".into()],
            hashes: Vec::new(),
            dependencies: Vec::new(),
            is_available: true,
        };
        let version = version_summary(&file);
        assert_eq!(version.id, "5000");
        assert_eq!(version.version_type, "beta");
        assert_eq!(version.game_versions, vec!["1.21.1"]);
        assert_eq!(version.loaders, vec!["Fabric"]);
        assert_eq!(version.file_name.as_deref(), Some("jei-1.21.1.jar"));
        assert!(is_game_version("1.21.1"));
        assert!(!is_game_version("Fabric"));
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfDependency {
    mod_id: u64,
    relation_type: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseforgeKeyStatus {
    pub configured: bool,
    pub source: Option<String>,
}
