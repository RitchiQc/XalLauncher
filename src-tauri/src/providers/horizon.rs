use crate::download::{download_file_with_checksum, Checksum};
use crate::error::{AppError, Result};
use crate::http::ensure_success;
use crate::models::{ContentEntry, Loader, ProgressEvent, ProjectHit};
use crate::providers::common::ContentMetadata;
use crate::providers::modrinth::ModrinthVersion;
use serde::Deserialize;
use std::fs;
use std::path::Path;
use tauri::{AppHandle, Emitter};

const INDEX_URL: &str = "https://horizon-rust.github.io/horizon-mods/index.json";
const INDEX_MAX: usize = 16 * 1024 * 1024;
const INDEX_FRESH_SECONDS: u64 = 600;
const MOD_API: &str = "0.2";
const SHADER_API: &str = "0.2";
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
        "resourcepack" => Some("resourcepack"),
        "shader" | "shaderpack" => Some("shader"),
        _ => None,
    }
}

fn version_compatible(kind: &str, version: &Version) -> bool {
    if version.game != "26.3" {
        return false;
    }
    match kind {
        "mod" => version
            .api
            .as_deref()
            .is_some_and(|api| api == MOD_API || api.starts_with("0.2.")),
        "shader" => version
            .api
            .as_deref()
            .is_some_and(|api| api == SHADER_API || api.starts_with("0.2.")),
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
        if project_type != "mod" && project_type != "resourcepack" && project_type != "shader" {
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

fn installed_horizon(game_dir: &Path, project_id: &str, version_id: &str) -> Option<ContentEntry> {
    for (kind, folder) in [
        ("mod", "horizon-mods"),
        ("resourcepack", "resourcepacks"),
        ("shader", "shaderpacks"),
    ] {
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
            } else {
                continue;
            };
            let meta = path.metadata().ok()?;
            let enabled = if kind == "mod" {
                file_enabled && mod_enabled(game_dir, mod_id_from_file(file_name))
            } else {
                file_enabled
            };
            return Some(ContentEntry {
                file_name: path.file_name()?.to_string_lossy().into_owned(),
                kind: kind.into(),
                enabled,
                size: meta.len(),
                modified_at: meta
                    .modified()
                    .map(chrono::DateTime::<chrono::Utc>::from)
                    .unwrap_or_else(|_| chrono::Utc::now()),
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
    if let Some(installed) = installed_horizon(&game_dir, &project.id, &version.version) {
        return Ok(installed);
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

#[cfg(test)]
mod tests {
    use super::{
        browser_kind, mod_enabled, set_mod_enabled, version_compatible, version_revoked, File,
        Index, Revoked, Version,
    };

    fn version(game: &str, api: &str, sha256: &str) -> Version {
        Version {
            version: "1.0.0".into(),
            published: "2026-11-09".into(),
            api: Some(api.into()),
            game: game.into(),
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
        assert!(!version_compatible(
            "native-mod",
            &version("26.3", "native-1", &"a".repeat(64))
        ));
        assert!(version_compatible(
            "shader",
            &version("26.3", "0.2", &"a".repeat(64))
        ));
        assert!(!version_compatible(
            "shader",
            &version("26.3", "0.1", &"a".repeat(64))
        ));
        assert!(version_compatible(
            "resourcepack",
            &version("26.3", "0.1", &"a".repeat(64))
        ));
        assert_eq!(browser_kind("shaderpack"), Some("shader"));
        assert_eq!(browser_kind("native-mod"), None);
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
