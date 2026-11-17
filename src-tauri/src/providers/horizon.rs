use crate::download::{download_file_with_checksum, Checksum};
use crate::error::{AppError, Result};
use crate::http::ensure_success;
use crate::models::{ContentEntry, Instance, Loader, ProgressEvent, ProjectHit};
use crate::providers::common::ContentMetadata;
use crate::providers::modrinth::ModrinthVersion;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::Path;
use tauri::{AppHandle, Emitter};

const INDEX_URL: &str = "https://horizon-rust.github.io/horizon-mods/index.json";
const INDEX_MAX: usize = 16 * 1024 * 1024;
const INDEX_FRESH_SECONDS: u64 = 600;
const MOD_API: &str = "0.2";
const SHADER_APIS: [&str; 2] = ["0.1", "0.2"];
const MODPACK_FORMAT: &str = "1";
const NATIVE_DIR: &str = "horizon-native-mods";
const NATIVE_MAX_ENTRIES: usize = 512;
const NATIVE_MAX_FILE: u64 = 32 * 1024 * 1024;
const NATIVE_MAX_TOTAL: u64 = 128 * 1024 * 1024;
const DOWNLOAD_HOSTS: [&str; 2] = [
    "https://github.com/horizon-rust/horizon-mods/releases/download/",
    "https://github.com/ludovic111/horizon-mods/releases/download/",
];

#[derive(Debug, Deserialize)]
struct Index {
    schema: u32,
    #[serde(default)]
    projects: Vec<Project>,
    #[serde(default)]
    revoked: Vec<Revoked>,
}

#[derive(Debug, Deserialize)]
struct Project {
    id: String,
    kind: String,
    name: String,
    summary: String,
    #[serde(default)]
    authors: Vec<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    versions: Vec<Version>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    stars: u64,
}

#[derive(Debug, Deserialize)]
struct Version {
    version: String,
    #[serde(default)]
    published: String,
    #[serde(default)]
    api: Option<String>,
    #[serde(default)]
    game: String,
    #[serde(default)]
    broken_on: Option<serde_json::Value>,
    file: File,
}

#[derive(Debug, Deserialize)]
struct File {
    name: String,
    url: String,
    size: u64,
    sha256: String,
}

#[derive(Debug, Deserialize)]
struct Revoked {
    id: String,
    #[serde(default)]
    sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PackManifest {
    format: u32,
    id: String,
    name: String,
    version: String,
    game: String,
    #[serde(default)]
    mods: Vec<PackMod>,
}

#[derive(Debug, Deserialize)]
struct PackMod {
    id: String,
    version: String,
    sha256: String,
    #[serde(default = "default_true")]
    on: bool,
}

fn default_true() -> bool {
    true
}

async fn load_index(state: &crate::AppState) -> Result<Index> {
    let cache = state
        .store
        .root()
        .join("cache")
        .join("horizon-mods")
        .join("index.json");
    let fresh = fs::metadata(&cache)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|elapsed| elapsed.as_secs() < INDEX_FRESH_SECONDS);
    if fresh {
        if let Ok(index) = read_index(&cache) {
            return Ok(index);
        }
    }
    match fetch_index(&state.client, &cache).await {
        Ok(index) => Ok(index),
        Err(error) => read_index(&cache).map_err(|_| error),
    }
}

fn read_index(path: &Path) -> Result<Index> {
    let bytes = fs::read(path)?;
    let index: Index = serde_json::from_slice(&bytes)?;
    validate_schema(&index)?;
    Ok(index)
}

async fn fetch_index(client: &reqwest::Client, cache: &Path) -> Result<Index> {
    let response = client.get(INDEX_URL).send().await?;
    let response = ensure_success(response, "Registre Horizon").await?;
    if response
        .content_length()
        .is_some_and(|length| length > INDEX_MAX as u64)
    {
        return Err(AppError::new(
            "Le registre Horizon est anormalement volumineux",
        ));
    }
    let bytes = response.bytes().await?;
    if bytes.len() > INDEX_MAX {
        return Err(AppError::new(
            "Le registre Horizon est anormalement volumineux",
        ));
    }
    let index: Index = serde_json::from_slice(&bytes)?;
    validate_schema(&index)?;
    if let Some(parent) = cache.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(cache, &bytes)?;
    Ok(index)
}

fn validate_schema(index: &Index) -> Result<()> {
    if index.schema == 1 {
        return Ok(());
    }
    Err(AppError::new(format!(
        "Registre Horizon schéma {} non pris en charge : mettez XalLauncher à jour",
        index.schema
    )))
}

fn browser_kind(kind: &str) -> Option<&'static str> {
    match kind {
        "mod" => Some("mod"),
        "native-mod" => Some("native-mod"),
        "modpack" => Some("modpack"),
        "resourcepack" => Some("resourcepack"),
        "shader" | "shaderpack" => Some("shader"),
        _ => None,
    }
}

fn api_line(api: &str, line: &str) -> bool {
    api == line || api.starts_with(&format!("{line}."))
}

fn version_compatible(kind: &str, version: &Version) -> bool {
    if version.game != "26.3" {
        return false;
    }
    match kind {
        "mod" => version
            .api
            .as_deref()
            .is_some_and(|api| api_line(api, MOD_API)),
        "shader" => version
            .api
            .as_deref()
            .is_some_and(|api| SHADER_APIS.iter().any(|line| api_line(api, line))),
        "native-mod" => {
            version.broken_on.is_none()
                && version
                    .api
                    .as_deref()
                    .is_some_and(|api| api == "native-1" || api.starts_with("native-1/"))
        }
        "modpack" => version
            .api
            .as_deref()
            .is_some_and(|api| api_line(api, MODPACK_FORMAT)),
        "resourcepack" | "shaderpack" => true,
        _ => false,
    }
}

fn version_revoked(index: &Index, project_id: &str, version: &Version) -> bool {
    index.revoked.iter().any(|revoked| {
        revoked.id == project_id
            && revoked
                .sha256
                .as_deref()
                .is_none_or(|hash| hash.eq_ignore_ascii_case(&version.file.sha256))
    })
}

fn compatible_versions<'a>(index: &Index, project: &'a Project) -> Vec<&'a Version> {
    project
        .versions
        .iter()
        .filter(|version| version_compatible(&project.kind, version))
        .filter(|version| !version_revoked(index, &project.id, version))
        .collect()
}

fn relevance(project: &Project, query: &str) -> i32 {
    if query.is_empty() {
        return 0;
    }
    let name = project.name.to_lowercase();
    if name == query {
        return 0;
    }
    if name.contains(query) {
        return 1;
    }
    let summary = project.summary.to_lowercase();
    if summary.contains(query) {
        return 2;
    }
    let haystack = format!(
        "{} {} {} {}",
        summary,
        project.categories.join(" "),
        project.tags.join(" "),
        project.authors.join(" ")
    )
    .to_lowercase();
    if haystack.contains(query) {
        3
    } else {
        4
    }
}

#[tauri::command]
pub async fn search_horizon(
    query: String,
    project_type: String,
    version_id: Option<String>,
    loader: Option<Loader>,
    offset: Option<u32>,
    index: Option<String>,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Vec<ProjectHit>> {
    if loader.is_some_and(|loader| loader != Loader::Horizon) {
        return Ok(Vec::new());
    }
    if version_id
        .as_deref()
        .is_some_and(|version| version != crate::horizon::HORIZON_MINECRAFT)
    {
        return Ok(Vec::new());
    }
    let data = load_index(&state).await?;
    let sort = match index.as_deref() {
        Some("downloads") => "downloads",
        Some("follows") => "follows",
        Some("newest") => "newest",
        Some("updated") => "updated",
        _ => "relevance",
    };
    let query = query.trim().to_lowercase();
    let mut hits = Vec::new();
    for project in &data.projects {
        if browser_kind(&project.kind) != Some(project_type.as_str()) {
            continue;
        }
        let versions = compatible_versions(&data, project);
        if versions.is_empty() {
            continue;
        }
        let score = relevance(project, &query);
        if score == 4 {
            continue;
        }
        let newest = versions
            .iter()
            .map(|version| version.published.as_str())
            .max()
            .unwrap_or_default()
            .to_string();
        hits.push((
            score,
            newest,
            ProjectHit {
                project_id: project.id.clone(),
                slug: project.id.clone(),
                title: project.name.clone(),
                description: project.summary.clone(),
                author: if project.authors.is_empty() {
                    "Horizon".into()
                } else {
                    project.authors.join(", ")
                },
                icon_url: project.icon.clone(),
                downloads: project.downloads,
                follows: project.stars,
                project_type: project_type.clone(),
                versions: vec![crate::horizon::HORIZON_MINECRAFT.to_string()],
                provider: Some("horizon".into()),
            },
        ));
    }
    hits.sort_by(|left, right| match sort {
        "downloads" => right.2.downloads.cmp(&left.2.downloads),
        "follows" => right.2.follows.cmp(&left.2.follows),
        "newest" | "updated" => right.1.cmp(&left.1),
        _ => left
            .0
            .cmp(&right.0)
            .then_with(|| right.2.downloads.cmp(&left.2.downloads)),
    });
    let offset = offset.unwrap_or(0) as usize;
    Ok(hits
        .into_iter()
        .skip(offset)
        .take(24)
        .map(|(_, _, hit)| hit)
        .collect())
}

fn find_project<'a>(index: &'a Index, project_id: &str) -> Result<&'a Project> {
    index
        .projects
        .iter()
        .find(|project| project.id == project_id)
        .ok_or_else(|| AppError::new("Projet Horizon introuvable"))
}

fn find_version<'a>(
    index: &Index,
    project: &'a Project,
    version_id: Option<&str>,
) -> Result<&'a Version> {
    let versions = compatible_versions(index, project);
    if let Some(version_id) = version_id {
        return versions
            .into_iter()
            .find(|version| version.version == version_id)
            .ok_or_else(|| {
                AppError::new("Cette version Horizon n’est pas compatible avec Minecraft 26.3")
            });
    }
    versions
        .into_iter()
        .next()
        .ok_or_else(|| AppError::new("Aucune version Horizon compatible"))
}

#[tauri::command]
pub async fn list_horizon_versions(
    project_id: String,
    instance_id: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Vec<ModrinthVersion>> {
    let instance = state.store.instance(&instance_id)?;
    if instance.loader != Loader::Horizon {
        return Err(AppError::new(
            "Ce projet Horizon nécessite une instance Horizon",
        ));
    }
    let index = load_index(&state).await?;
    let project = find_project(&index, &project_id)?;
    Ok(compatible_versions(&index, project)
        .into_iter()
        .map(|version| ModrinthVersion {
            id: version.version.clone(),
            name: format!("{} {}", project.name, version.version),
            version_number: version.version.clone(),
            version_type: "release".into(),
            date_published: version.published.clone(),
            game_versions: vec![version.game.clone()],
            loaders: vec!["horizon".into()],
        })
        .collect())
}

fn safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['/', '\\'])
        && name != "."
        && name != ".."
        && !name.starts_with('.')
        && !name.chars().any(|character| {
            character.is_control() || matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*')
        })
}

fn sha256(value: &str) -> Result<String> {
    if value.len() == 64 && value.chars().all(|character| character.is_ascii_hexdigit()) {
        return Ok(value.to_ascii_lowercase());
    }
    Err(AppError::new("Empreinte SHA-256 Horizon invalide"))
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

pub(crate) fn mod_enabled(game_dir: &Path, id: &str) -> bool {
    let key = format!("mod.{id}");
    fs::read_to_string(game_dir.join("horizon-mods.txt"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once(':'))
        .any(|(name, value)| name.trim() == key && value.trim() == "true")
}

pub(crate) fn set_mod_enabled(game_dir: &Path, id: &str, enabled: bool) -> Result<()> {
    let path = game_dir.join("horizon-mods.txt");
    let key = format!("mod.{id}");
    let mut lines: Vec<String> = fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .filter(|line| {
            line.split_once(':')
                .is_none_or(|(name, _)| name.trim() != key)
        })
        .map(str::to_owned)
        .collect();
    if enabled {
        lines.push(format!("{key}:true"));
    }
    let mut content = lines.join("\n");
    if !content.is_empty() {
        content.push('\n');
    }
    fs::write(path, content)?;
    Ok(())
}

fn installed_horizon(
    game_dir: &Path,
    project_id: &str,
    version_id: &str,
    kind: &str,
) -> Option<ContentEntry> {
    for (folder_kind, folder) in [
        ("mod", "horizon-mods"),
        ("native-mod", NATIVE_DIR),
        ("resourcepack", "resourcepacks"),
        ("shader", "shaderpacks"),
    ] {
        if folder_kind != kind {
            continue;
        }
        let directory = game_dir.join(folder);
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') || !name.ends_with(".xal.json") {
                continue;
            }
            let Ok(bytes) = fs::read(entry.path()) else {
                continue;
            };
            let Ok(metadata) = serde_json::from_slice::<ContentMetadata>(&bytes) else {
                continue;
            };
            if metadata.project_id != project_id || metadata.version_id != version_id {
                continue;
            }
            let Some(file_name) = name.trim_start_matches('.').strip_suffix(".xal.json") else {
                continue;
            };
            let enabled_path = directory.join(file_name);
            let disabled_path = directory.join(format!("{file_name}.disabled"));
            let (path, file_enabled) = if enabled_path.is_file() {
                (enabled_path, true)
            } else if disabled_path.is_file() {
                (disabled_path, false)
            } else if enabled_path.is_dir() {
                (enabled_path, true)
            } else {
                continue;
            };
            let enabled = if kind == "mod" {
                file_enabled && mod_enabled(game_dir, mod_id_from_file(file_name))
            } else if kind == "native-mod" {
                file_enabled && mod_enabled(game_dir, file_name)
            } else {
                file_enabled
            };
            let size = if path.is_dir() {
                directory_size(&path)
            } else {
                path.metadata().ok()?.len()
            };
            return Some(ContentEntry {
                file_name: path.file_name()?.to_string_lossy().into_owned(),
                kind: kind.into(),
                enabled,
                size,
                modified_at: path
                    .metadata()
                    .ok()
                    .and_then(|meta| meta.modified().ok())
                    .map(chrono::DateTime::<chrono::Utc>::from)
                    .unwrap_or_else(chrono::Utc::now),
                project_id: Some(metadata.project_id),
                project_title: Some(metadata.project_title),
                version_id: Some(metadata.version_id),
                icon_url: metadata.icon_url,
                provider: Some("horizon".into()),
            });
        }
    }
    None
}

pub(crate) fn mod_id_from_file(file_name: &str) -> &str {
    file_name
        .strip_suffix(".hmod")
        .or_else(|| file_name.strip_suffix(".hmod.disabled"))
        .unwrap_or(file_name)
}

fn remove_previous_versions(directory: &Path, project_id: &str, keep_file: &str) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with('.') || !name.ends_with(".xal.json") {
            continue;
        }
        let Ok(bytes) = fs::read(entry.path()) else {
            continue;
        };
        let Ok(metadata) = serde_json::from_slice::<ContentMetadata>(&bytes) else {
            continue;
        };
        if metadata.project_id != project_id {
            continue;
        }
        let Some(file_name) = name.trim_start_matches('.').strip_suffix(".xal.json") else {
            continue;
        };
        if file_name == keep_file {
            continue;
        }
        let _ = fs::remove_file(entry.path());
        let _ = fs::remove_file(directory.join(file_name));
        let _ = fs::remove_file(directory.join(format!("{file_name}.disabled")));
    }
}

fn extract_hnmod(archive: &Path, destination: &Path) -> Result<()> {
    let file = fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    if zip.len() > NATIVE_MAX_ENTRIES {
        return Err(AppError::new("Bundle natif Horizon trop volumineux"));
    }
    let mut total = 0_u64;
    for index in 0..zip.len() {
        let entry = zip.by_index(index)?;
        let path = entry
            .enclosed_name()
            .ok_or_else(|| AppError::new("Chemin dangereux dans le bundle natif Horizon"))?
            .to_path_buf();
        let target = destination.join(&path);
        if !target.starts_with(destination) {
            return Err(AppError::new(
                "Chemin dangereux dans le bundle natif Horizon",
            ));
        }
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::File::create(&target)?;
        let mut limited = entry.take(NATIVE_MAX_FILE + 1);
        let copied = std::io::copy(&mut limited, &mut output)?;
        if copied > NATIVE_MAX_FILE {
            return Err(AppError::new("Fichier natif Horizon trop volumineux"));
        }
        total += copied;
        if total > NATIVE_MAX_TOTAL {
            return Err(AppError::new("Bundle natif Horizon trop volumineux"));
        }
    }
    Ok(())
}

fn validate_native_manifest(directory: &Path, project_id: &str, version_id: &str) -> Result<()> {
    let bytes = fs::read(directory.join("horizon-native.json"))?;
    let manifest: serde_json::Value = serde_json::from_slice(&bytes)?;
    let valid = manifest["schema"].as_u64() == Some(1)
        && manifest["game"].as_str() == Some(crate::horizon::HORIZON_MINECRAFT)
        && manifest["id"].as_str() == Some(project_id)
        && manifest["version"].as_str() == Some(version_id);
    if !valid {
        return Err(AppError::new(
            "Le manifeste du bundle natif Horizon ne correspond pas au registre",
        ));
    }
    Ok(())
}

async fn install_horizon_native(
    project: &Project,
    version: &Version,
    game_dir: &Path,
    state: &crate::AppState,
    app: &AppHandle,
) -> Result<ContentEntry> {
    if !safe_file_name(&project.id) {
        return Err(AppError::new("Identifiant de mod natif Horizon dangereux"));
    }
    if !safe_file_name(&version.file.name)
        || !version.file.name.to_ascii_lowercase().ends_with(".hnmod")
        || !DOWNLOAD_HOSTS
            .iter()
            .any(|host| version.file.url.starts_with(host))
    {
        return Err(AppError::new("Téléchargement natif Horizon non autorisé"));
    }
    let directory = game_dir.join(NATIVE_DIR);
    fs::create_dir_all(&directory)?;
    let archive = directory.join(format!(".{}.hnmod.download", project.id));
    let _ = fs::remove_file(&archive);
    let checksum = Checksum::Sha256(sha256(&version.file.sha256)?);
    if let Err(error) = download_file_with_checksum(
        &state.client,
        &version.file.url,
        &archive,
        Some(&checksum),
        Some(app),
        "content",
        &project.name,
    )
    .await
    {
        let _ = fs::remove_file(&archive);
        return Err(error);
    }
    let size = archive.metadata()?.len();
    if size != version.file.size {
        let _ = fs::remove_file(&archive);
        return Err(AppError::new(format!(
            "Taille incorrecte pour {} : {size} octets au lieu de {}",
            project.name, version.file.size
        )));
    }
    let temporary = directory.join(format!(".{}.tmp", project.id));
    let _ = fs::remove_dir_all(&temporary);
    fs::create_dir_all(&temporary)?;
    if let Err(error) = extract_hnmod(&archive, &temporary)
        .and_then(|_| validate_native_manifest(&temporary, &project.id, &version.version))
    {
        let _ = fs::remove_dir_all(&temporary);
        let _ = fs::remove_file(&archive);
        return Err(error);
    }
    let installed = directory.join(&project.id);
    if installed.exists() {
        fs::remove_dir_all(&installed)?;
    }
    if let Err(error) = fs::rename(&temporary, &installed) {
        let _ = fs::remove_dir_all(&temporary);
        let _ = fs::remove_file(&archive);
        return Err(error.into());
    }
    let _ = fs::remove_file(&archive);
    let metadata = ContentMetadata {
        project_id: project.id.clone(),
        project_title: project.name.clone(),
        version_id: version.version.clone(),
        icon_url: project.icon.clone(),
        provider: Some("horizon".into()),
    };
    fs::write(
        directory.join(format!(".{}.xal.json", project.id)),
        serde_json::to_vec_pretty(&metadata)?,
    )?;
    set_mod_enabled(game_dir, &project.id, true)?;
    Ok(ContentEntry {
        file_name: project.id.clone(),
        kind: "native-mod".into(),
        enabled: true,
        size: directory_size(&installed),
        modified_at: chrono::Utc::now(),
        project_id: Some(project.id.clone()),
        project_title: Some(project.name.clone()),
        version_id: Some(version.version.clone()),
        icon_url: project.icon.clone(),
        provider: Some("horizon".into()),
    })
}

#[tauri::command]
pub async fn install_horizon_project(
    project_id: String,
    instance_id: String,
    version_id: Option<String>,
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<ContentEntry> {
    let instance = state.store.instance(&instance_id)?;
    if instance.loader != Loader::Horizon {
        return Err(AppError::new(
            "Ce contenu Horizon nécessite une instance Horizon",
        ));
    }
    let index = load_index(&state).await?;
    let project = find_project(&index, &project_id)?;
    let version = find_version(&index, project, version_id.as_deref())?;
    let kind = browser_kind(&project.kind)
        .ok_or_else(|| AppError::new("Ce type de contenu Horizon n’est pas pris en charge"))?;
    if !safe_file_name(&version.file.name) {
        return Err(AppError::new("Nom de fichier Horizon dangereux"));
    }
    if !DOWNLOAD_HOSTS
        .iter()
        .any(|host| version.file.url.starts_with(host))
    {
        return Err(AppError::new(
            "Le registre Horizon pointe vers un hôte de téléchargement non autorisé",
        ));
    }
    let checksum = Checksum::Sha256(sha256(&version.file.sha256)?);
    let game_dir = state.store.game_dir(&instance_id);
    if let Some(installed) = installed_horizon(&game_dir, &project.id, &version.version, kind) {
        return Ok(installed);
    }
    if kind == "native-mod" {
        return install_horizon_native(project, version, &game_dir, state.inner(), &app).await;
    }
    let (folder, file_name) = match kind {
        "mod" => ("horizon-mods", format!("{}.hmod", project.id)),
        "resourcepack" => ("resourcepacks", version.file.name.clone()),
        "shader" => ("shaderpacks", version.file.name.clone()),
        _ => return Err(AppError::new("Type de contenu Horizon inconnu")),
    };
    if !safe_file_name(&file_name) {
        return Err(AppError::new("Nom de fichier Horizon dangereux"));
    }
    let directory = game_dir.join(folder);
    fs::create_dir_all(&directory)?;
    let destination = directory.join(&file_name);
    download_file_with_checksum(
        &state.client,
        &version.file.url,
        &destination,
        Some(&checksum),
        Some(&app),
        "content",
        &project.name,
    )
    .await?;
    let size = destination.metadata()?.len();
    if size != version.file.size {
        let _ = fs::remove_file(&destination);
        return Err(AppError::new(format!(
            "Taille incorrecte pour {} : {size} octets au lieu de {}",
            project.name, version.file.size
        )));
    }
    remove_previous_versions(&directory, &project.id, &file_name);
    if kind == "mod" {
        let _ = fs::remove_file(directory.join(format!("{}.hmod.disabled", project.id)));
        set_mod_enabled(&game_dir, &project.id, true)?;
    }
    let metadata = ContentMetadata {
        project_id: project.id.clone(),
        project_title: project.name.clone(),
        version_id: version.version.clone(),
        icon_url: project.icon.clone(),
        provider: Some("horizon".into()),
    };
    fs::write(
        directory.join(format!(".{file_name}.xal.json")),
        serde_json::to_vec_pretty(&metadata)?,
    )?;
    let _ = app.emit(
        "download-progress",
        ProgressEvent {
            operation: "content",
            label: "Installation des contenus",
            current: 1,
            total: 1,
        },
    );
    Ok(ContentEntry {
        file_name,
        kind: kind.into(),
        enabled: true,
        size,
        modified_at: chrono::Utc::now(),
        project_id: Some(project.id.clone()),
        project_title: Some(project.name.clone()),
        version_id: Some(version.version.clone()),
        icon_url: project.icon.clone(),
        provider: Some("horizon".into()),
    })
}

fn read_pack_manifest(archive: &Path) -> Result<PackManifest> {
    let file = fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let entry = zip
        .by_name("horizon-modpack.toml")
        .map_err(|_| AppError::new("Cette archive n’est pas un modpack Horizon valide"))?;
    if entry.size() > 1_048_576 {
        return Err(AppError::new(
            "Manifeste de modpack Horizon trop volumineux",
        ));
    }
    let mut text = String::new();
    entry.take(1_048_577).read_to_string(&mut text)?;
    if text.len() > 1_048_576 {
        return Err(AppError::new(
            "Manifeste de modpack Horizon trop volumineux",
        ));
    }
    toml::from_str(&text)
        .map_err(|error| AppError::new(format!("Manifeste de modpack Horizon invalide : {error}")))
}

fn validate_pack_manifest(
    manifest: &PackManifest,
    project: &Project,
    version: &Version,
) -> Result<()> {
    if manifest.format != 1
        || manifest.id != project.id
        || manifest.game != crate::horizon::HORIZON_MINECRAFT
        || manifest.version != version.version
    {
        return Err(AppError::new(
            "Le manifeste du modpack Horizon ne correspond pas au registre",
        ));
    }
    if manifest.name.trim().is_empty() {
        return Err(AppError::new("Le modpack Horizon n’a pas de nom"));
    }
    if manifest.mods.is_empty() {
        return Err(AppError::new("Le modpack Horizon ne contient aucun mod"));
    }
    let mut seen = HashSet::new();
    for requirement in &manifest.mods {
        if !safe_file_name(&requirement.id) || !seen.insert(requirement.id.as_str()) {
            return Err(AppError::new("Identifiant de mod dupliqué ou dangereux"));
        }
        sha256(&requirement.sha256)?;
    }
    Ok(())
}

fn pack_requirement<'a>(
    index: &'a Index,
    requirement: &PackMod,
) -> Result<(&'a Project, &'a Version)> {
    let project = find_project(index, &requirement.id)?;
    if project.kind != "mod" && project.kind != "native-mod" {
        return Err(AppError::new(format!(
            "Le modpack Horizon référence un contenu non installable : {}",
            requirement.id
        )));
    }
    let version = project
        .versions
        .iter()
        .find(|version| {
            version.version == requirement.version
                && version
                    .file
                    .sha256
                    .eq_ignore_ascii_case(&requirement.sha256)
        })
        .ok_or_else(|| {
            AppError::new(format!(
                "Version {} introuvable ou empreinte incorrecte pour {}",
                requirement.version, requirement.id
            ))
        })?;
    let native_ok = project.kind == "native-mod"
        && version.game == crate::horizon::HORIZON_MINECRAFT
        && version
            .api
            .as_deref()
            .is_some_and(|api| api == "native-1" || api.starts_with("native-1/"));
    if (!version_compatible(&project.kind, version) && !native_ok)
        || version_revoked(index, &project.id, version)
    {
        return Err(AppError::new(format!(
            "La version {} de {} n’est pas compatible avec ce build Horizon",
            requirement.version, requirement.id
        )));
    }
    Ok((project, version))
}

const PACK_OVERRIDE_MAX: u64 = 64 * 1024 * 1024;
const PACK_OVERRIDES_MAX: u64 = 256 * 1024 * 1024;

fn override_allowed(path: &str, mods: &HashSet<&str>) -> bool {
    if matches!(
        path,
        "options.txt" | "horizon-graphics.txt" | "horizon-extras.txt"
    ) {
        return true;
    }
    if let Some(name) = path.strip_prefix("resourcepacks/") {
        return !name.contains('/') && name.to_ascii_lowercase().ends_with(".zip");
    }
    if let Some(rest) = path.strip_prefix("horizon-mods/.horizon-mod-data/") {
        let mut parts = rest.splitn(2, '/');
        let id = parts.next().unwrap_or_default();
        return parts.next().is_some() && mods.contains(id);
    }
    false
}

async fn install_pack_content(
    project: &Project,
    version: &Version,
    game_dir: &Path,
    enabled: bool,
    state: &crate::AppState,
    app: &AppHandle,
) -> Result<()> {
    if !safe_file_name(&version.file.name)
        || !version.file.name.to_ascii_lowercase().ends_with(".hmod")
        || !DOWNLOAD_HOSTS
            .iter()
            .any(|host| version.file.url.starts_with(host))
    {
        return Err(AppError::new("Téléchargement Horizon non autorisé"));
    }
    let file_name = format!("{}.hmod", project.id);
    if !safe_file_name(&file_name) {
        return Err(AppError::new("Nom de fichier Horizon dangereux"));
    }
    let directory = game_dir.join("horizon-mods");
    fs::create_dir_all(&directory)?;
    let destination = directory.join(&file_name);
    let checksum = Checksum::Sha256(sha256(&version.file.sha256)?);
    download_file_with_checksum(
        &state.client,
        &version.file.url,
        &destination,
        Some(&checksum),
        Some(app),
        "modpack",
        &project.name,
    )
    .await?;
    let size = destination.metadata()?.len();
    if size != version.file.size {
        let _ = fs::remove_file(&destination);
        return Err(AppError::new(format!(
            "Taille incorrecte pour {} : {size} octets au lieu de {}",
            project.name, version.file.size
        )));
    }
    remove_previous_versions(&directory, &project.id, &file_name);
    let _ = fs::remove_file(directory.join(format!("{}.hmod.disabled", project.id)));
    set_mod_enabled(game_dir, &project.id, enabled)?;
    let metadata = ContentMetadata {
        project_id: project.id.clone(),
        project_title: project.name.clone(),
        version_id: version.version.clone(),
        icon_url: project.icon.clone(),
        provider: Some("horizon".into()),
    };
    fs::write(
        directory.join(format!(".{file_name}.xal.json")),
        serde_json::to_vec_pretty(&metadata)?,
    )?;
    Ok(())
}

async fn install_pack_files(
    archive: &Path,
    state: &crate::AppState,
    app: &AppHandle,
    index: &Index,
    manifest: &PackManifest,
    game_dir: &Path,
) -> Result<()> {
    let total = manifest.mods.len() as u64;
    let _ = app.emit(
        "download-progress",
        ProgressEvent {
            operation: "modpack",
            label: "Fichiers du modpack Horizon",
            current: 0,
            total,
        },
    );
    for (number, requirement) in manifest.mods.iter().enumerate() {
        let (project, version) = pack_requirement(index, requirement)?;
        if project.kind == "native-mod" {
            install_horizon_native(project, version, game_dir, state, app).await?;
            if !requirement.on {
                set_mod_enabled(game_dir, &project.id, false)?;
            }
        } else {
            install_pack_content(project, version, game_dir, requirement.on, state, app).await?;
        }
        let _ = app.emit(
            "download-progress",
            ProgressEvent {
                operation: "modpack",
                label: "Fichiers du modpack Horizon",
                current: number as u64 + 1,
                total,
            },
        );
    }
    extract_pack_overrides(archive, game_dir, manifest)
}

fn extract_pack_overrides(archive: &Path, game_dir: &Path, manifest: &PackManifest) -> Result<()> {
    let file = fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let mods: HashSet<&str> = manifest.mods.iter().map(|item| item.id.as_str()).collect();
    let mut total = 0_u64;
    for index in 0..zip.len() {
        let entry = zip.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name()?;
        let Some(relative) = name.strip_prefix("overrides/") else {
            continue;
        };
        if relative.is_empty() || relative.contains('\\') || !override_allowed(relative, &mods) {
            return Err(AppError::new(format!(
                "Fichier d’overrides non autorisé dans le modpack Horizon : {relative}"
            )));
        }
        let relative_path = crate::modpacks::safe_relative(relative)?;
        let target = game_dir.join(&relative_path);
        if !target.starts_with(game_dir) {
            return Err(AppError::new("Chemin d’override Horizon dangereux"));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::File::create(&target)?;
        let mut limited = entry.take(PACK_OVERRIDE_MAX + 1);
        let copied = std::io::copy(&mut limited, &mut output)?;
        if copied > PACK_OVERRIDE_MAX {
            return Err(AppError::new("Fichier d’override Horizon trop volumineux"));
        }
        total += copied;
        if total > PACK_OVERRIDES_MAX {
            return Err(AppError::new(
                "Overrides du modpack Horizon trop volumineux",
            ));
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn install_horizon_modpack(
    project_id: String,
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Instance> {
    let index = load_index(&state).await?;
    let project = find_project(&index, &project_id)?;
    if project.kind != "modpack" {
        return Err(AppError::new("Ce projet n’est pas un modpack Horizon"));
    }
    let version = find_version(&index, project, None)?;
    if !safe_file_name(&version.file.name)
        || !version.file.name.to_ascii_lowercase().ends_with(".hpack")
        || !DOWNLOAD_HOSTS
            .iter()
            .any(|host| version.file.url.starts_with(host))
    {
        return Err(AppError::new(
            "Téléchargement du modpack Horizon non autorisé",
        ));
    }
    let checksum = Checksum::Sha256(sha256(&version.file.sha256)?);
    let cache = state
        .store
        .root()
        .join("cache")
        .join(format!("horizon-{}-{}.hpack", project.id, version.version));
    if let Some(parent) = cache.parent() {
        fs::create_dir_all(parent)?;
    }
    let _ = fs::remove_file(&cache);
    download_file_with_checksum(
        &state.client,
        &version.file.url,
        &cache,
        Some(&checksum),
        Some(&app),
        "modpack",
        &project.name,
    )
    .await?;
    let size = cache.metadata()?.len();
    if size != version.file.size {
        let _ = fs::remove_file(&cache);
        return Err(AppError::new(format!(
            "Taille incorrecte pour le modpack {} : {size} octets au lieu de {}",
            project.name, version.file.size
        )));
    }
    let manifest = match read_pack_manifest(&cache)
        .and_then(|manifest| validate_pack_manifest(&manifest, project, version).map(|_| manifest))
    {
        Ok(manifest) => manifest,
        Err(error) => {
            let _ = fs::remove_file(&cache);
            return Err(error);
        }
    };
    let id = uuid::Uuid::new_v4().to_string();
    let game_dir = state.store.game_dir(&id);
    fs::create_dir_all(&game_dir)?;
    if let Err(error) =
        install_pack_files(&cache, state.inner(), &app, &index, &manifest, &game_dir).await
    {
        let _ = fs::remove_dir_all(state.store.instance_dir(&id));
        let _ = fs::remove_file(&cache);
        return Err(error);
    }
    let instance = Instance {
        id: id.clone(),
        name: manifest.name.clone(),
        version_id: crate::horizon::HORIZON_MINECRAFT.into(),
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
        icon_url: project.icon.clone(),
        icon_file: None,
        modules: Default::default(),
        created_at: chrono::Utc::now(),
        last_played_at: None,
        total_play_seconds: 0,
    };
    let marker = serde_json::json!({
        "id": project.id,
        "version": version.version,
        "name": project.name,
    });
    if let Err(error) = fs::write(
        state.store.instance_dir(&id).join("horizon-modpack.json"),
        serde_json::to_vec_pretty(&marker)?,
    ) {
        let _ = fs::remove_dir_all(state.store.instance_dir(&id));
        let _ = fs::remove_file(&cache);
        return Err(error.into());
    }
    let mut instances = match state.store.instances() {
        Ok(instances) => instances,
        Err(error) => {
            let _ = fs::remove_dir_all(state.store.instance_dir(&id));
            let _ = fs::remove_file(&cache);
            return Err(error);
        }
    };
    instances.push(instance.clone());
    if let Err(error) = state.store.save_instances(&instances) {
        let _ = fs::remove_dir_all(state.store.instance_dir(&id));
        let _ = fs::remove_file(&cache);
        return Err(error);
    }
    let _ = fs::remove_file(&cache);
    Ok(instance)
}

#[cfg(test)]
mod tests {
    use super::{
        browser_kind, mod_enabled, override_allowed, pack_requirement, set_mod_enabled,
        validate_native_manifest, version_compatible, version_revoked, File, Index, PackManifest,
        PackMod, Project, Revoked, Version,
    };
    use std::collections::HashSet;

    fn version(game: &str, api: &str, sha256: &str) -> Version {
        Version {
            version: "1.0.0".into(),
            published: "2026-11-09".into(),
            api: Some(api.into()),
            game: game.into(),
            broken_on: None,
            file: File {
                name: "mod.hmod".into(),
                url: "https://github.com/horizon-rust/horizon-mods/releases/download/x/mod.hmod"
                    .into(),
                size: 1,
                sha256: sha256.into(),
            },
        }
    }

    fn index(revoked: Vec<Revoked>) -> Index {
        Index {
            schema: 1,
            projects: Vec::new(),
            revoked,
        }
    }

    #[test]
    fn accepts_only_supported_horizon_content() {
        assert!(version_compatible(
            "mod",
            &version("26.3", "0.2", &"a".repeat(64))
        ));
        assert!(!version_compatible(
            "mod",
            &version("26.2", "0.2", &"a".repeat(64))
        ));
        assert!(!version_compatible(
            "mod",
            &version("26.3", "0.1", &"a".repeat(64))
        ));
        assert!(version_compatible(
            "native-mod",
            &version("26.3", "native-1/abc/def", &"a".repeat(64))
        ));
        assert!(!version_compatible(
            "native-mod",
            &version("26.3", "native-2/abc/def", &"a".repeat(64))
        ));
        let mut broken = version("26.3", "native-1/abc/def", &"a".repeat(64));
        broken.broken_on =
            Some(serde_json::json!({ "horizon": "abc", "code": "E_TARGET_NOT_FOUND" }));
        assert!(!version_compatible("native-mod", &broken));
        assert!(version_compatible(
            "shader",
            &version("26.3", "0.2", &"a".repeat(64))
        ));
        assert!(version_compatible(
            "shader",
            &version("26.3", "0.1", &"a".repeat(64))
        ));
        assert!(!version_compatible(
            "shader",
            &version("26.3", "0.3", &"a".repeat(64))
        ));
        assert!(version_compatible(
            "modpack",
            &version("26.3", "1", &"a".repeat(64))
        ));
        assert!(version_compatible(
            "resourcepack",
            &version("26.3", "0.1", &"a".repeat(64))
        ));
        assert_eq!(browser_kind("shaderpack"), Some("shader"));
        assert_eq!(browser_kind("native-mod"), Some("native-mod"));
        assert_eq!(browser_kind("modpack"), Some("modpack"));
    }

    #[test]
    fn hides_revoked_versions() {
        let sha = "a".repeat(64);
        let version = version("26.3", "0.2", &sha);
        assert!(version_revoked(
            &index(vec![Revoked {
                id: "mod".into(),
                sha256: Some(sha.clone()),
            }]),
            "mod",
            &version
        ));
        assert!(version_revoked(
            &index(vec![Revoked {
                id: "mod".into(),
                sha256: None,
            }]),
            "mod",
            &version
        ));
        assert!(!version_revoked(&index(Vec::new()), "mod", &version));
    }

    #[test]
    fn pack_accepts_a_broken_native_version_for_a_later_build() {
        let sha = "a".repeat(64);
        let mut version = version("26.3", "native-1/abc/def", &sha);
        version.broken_on =
            Some(serde_json::json!({ "horizon": "abc", "code": "E_TARGET_NOT_FOUND" }));
        let project = Project {
            id: "atlas".into(),
            kind: "native-mod".into(),
            name: "Atlas".into(),
            summary: String::new(),
            authors: Vec::new(),
            icon: None,
            categories: Vec::new(),
            tags: Vec::new(),
            versions: vec![version],
            downloads: 0,
            stars: 0,
        };
        let index = Index {
            schema: 1,
            projects: vec![project],
            revoked: Vec::new(),
        };
        let requirement = PackMod {
            id: "atlas".into(),
            version: "1.0.0".into(),
            sha256: sha,
            on: true,
        };
        assert!(pack_requirement(&index, &requirement).is_ok());
    }

    #[test]
    fn parses_a_horizon_pack_manifest() {
        let text = format!(
            "format = 1\nid = \"cozy\"\nname = \"Cozy\"\nversion = \"1.0.0\"\ngame = \"26.3\"\n\n[[mods]]\nid = \"coords_hud\"\nversion = \"0.1.0\"\nsha256 = \"{}\"\n",
            "a".repeat(64)
        );
        let manifest: PackManifest = toml::from_str(&text).unwrap();
        assert_eq!(manifest.format, 1);
        assert_eq!(manifest.id, "cozy");
        assert_eq!(manifest.mods.len(), 1);
        assert!(manifest.mods[0].on);
    }

    #[test]
    fn accepts_only_registry_override_paths() {
        let mods: HashSet<&str> = ["toggle_sprint"].into_iter().collect();
        assert!(override_allowed("options.txt", &mods));
        assert!(override_allowed("resourcepacks/foo.zip", &mods));
        assert!(override_allowed(
            "horizon-mods/.horizon-mod-data/toggle_sprint/background",
            &mods
        ));
        assert!(!override_allowed("resourcepacks/foo.txt", &mods));
        assert!(!override_allowed(
            "horizon-mods/.horizon-mod-data/other/background",
            &mods
        ));
        assert!(!override_allowed("config/evil.toml", &mods));
    }

    #[test]
    fn validates_a_native_manifest_against_the_registry() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("horizon-native.json"),
            r#"{"schema":1,"game":"26.3","id":"zoom","version":"2.0.0"}"#,
        )
        .unwrap();
        assert!(validate_native_manifest(directory.path(), "zoom", "2.0.0").is_ok());
        assert!(validate_native_manifest(directory.path(), "zoom", "1.0.0").is_err());
    }

    #[test]
    fn toggles_mods_in_horizon_mods_txt() {
        let directory = tempfile::tempdir().unwrap();
        set_mod_enabled(directory.path(), "coords_hud", true).unwrap();
        assert!(mod_enabled(directory.path(), "coords_hud"));
        assert!(
            std::fs::read_to_string(directory.path().join("horizon-mods.txt"))
                .unwrap()
                .contains("mod.coords_hud:true")
        );
        set_mod_enabled(directory.path(), "coords_hud", false).unwrap();
        assert!(!mod_enabled(directory.path(), "coords_hud"));
    }
}
