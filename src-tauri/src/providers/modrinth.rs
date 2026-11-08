use crate::download::download_file;
use crate::error::{AppError, Result};
use crate::http::{ensure_success, get_json};
use crate::models::{ContentEntry, Loader, ProjectHit};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::AppHandle;

#[derive(Deserialize)]
struct SearchResponse {
    hits: Vec<SearchHit>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct SearchHit {
    project_id: String,
    slug: String,
    title: String,
    description: String,
    author: String,
    #[serde(default)]
    icon_url: Option<String>,
    downloads: u64,
    #[serde(default)]
    follows: u64,
    project_type: String,
    #[serde(default)]
    versions: Vec<String>,
}

#[tauri::command]
pub async fn search_modrinth(
    query: String,
    project_type: String,
    version_id: Option<String>,
    loader: Option<Loader>,
    offset: Option<u32>,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Vec<ProjectHit>> {
    let mut facets = vec![json!([format!("project_type:{project_type}")])];
    if let Some(version) = version_id.filter(|value| !value.is_empty()) {
        facets.push(json!([format!("versions:{version}")]));
    }
    if project_type == "mod" {
        if let Some(loader) = loader {
            facets.push(json!([format!("categories:{}", loader_name(loader))]));
        }
    }
    let response = state
        .client
        .get("https://api.modrinth.com/v2/search")
        .query(&[
            ("query", query),
            ("limit", "24".into()),
            ("offset", offset.unwrap_or(0).to_string()),
            ("index", "relevance".into()),
            ("facets", serde_json::to_string(&facets)?),
        ])
        .send()
        .await?;
    let response: SearchResponse = ensure_success(response, "Recherche Modrinth")
        .await?
        .json()
        .await?;
    Ok(response
        .hits
        .into_iter()
        .map(|hit| ProjectHit {
            project_id: hit.project_id,
            slug: hit.slug,
            title: hit.title,
            description: hit.description,
            author: hit.author,
            icon_url: hit.icon_url,
            downloads: hit.downloads,
            follows: hit.follows,
            project_type: hit.project_type,
            versions: hit.versions,
        })
        .collect())
}

fn loader_name(loader: Loader) -> &'static str {
    match loader {
        Loader::Fabric => "fabric",
        Loader::Quilt => "quilt",
        Loader::Forge => "forge",
        Loader::Neoforge => "neoforge",
        Loader::Vanilla => "vanilla",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentMetadata {
    pub project_id: String,
    pub project_title: String,
    pub version_id: String,
    pub icon_url: Option<String>,
}

#[tauri::command]
pub async fn install_modrinth_project(
    project_id: String,
    instance_id: String,
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<ContentEntry> {
    let mut visited = std::collections::HashSet::new();
    install_project_tree(&state, &app, &instance_id, &project_id, &mut visited)
        .await?
        .ok_or_else(|| AppError::new("Ce contenu est déjà installé"))
}

fn install_project_tree<'a>(
    state: &'a crate::AppState,
    app: &'a AppHandle,
    instance_id: &'a str,
    project_id: &'a str,
    visited: &'a mut std::collections::HashSet<String>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<ContentEntry>>> + Send + 'a>>
{
    Box::pin(async move {
        if !visited.insert(project_id.to_string()) {
            return Ok(None);
        }
        let instance = state.store.instance(instance_id)?;
        let project: Value = get_json(
            &state.client,
            &format!("https://api.modrinth.com/v2/project/{project_id}"),
        )
        .await?;
        let project_type = project["project_type"].as_str().unwrap_or("mod");
        if project_type == "modpack" {
            return Err(AppError::new(
                "Les modpacks s’installent depuis leur page avec le bouton dédié",
            ));
        }
        let mut request = state
            .client
            .get(format!(
                "https://api.modrinth.com/v2/project/{project_id}/version"
            ))
            .query(&[(
                "game_versions",
                serde_json::to_string(&json!([instance.version_id]))?,
            )]);
        if project_type == "mod" {
            if instance.loader == Loader::Vanilla {
                return Err(AppError::new(
                    "Un modloader est requis pour installer ce mod",
                ));
            }
            request = request.query(&[(
                "loaders",
                serde_json::to_string(&json!([loader_name(instance.loader)]))?,
            )]);
        }
        let versions: Vec<Value> =
            ensure_success(request.send().await?, "Recherche d’une version compatible")
                .await?
                .json()
                .await?;
        let version = versions
            .first()
            .ok_or_else(|| {
                AppError::new(format!(
                    "Aucune version compatible pour {}",
                    project["title"].as_str().unwrap_or(project_id)
                ))
            })?
            .clone();
        for dependency in version["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|dependency| dependency["dependency_type"] == "required")
        {
            let dependency_id = dependency["project_id"].as_str();
            if dependency_id.is_none() {
                continue;
            }
            install_project_tree(state, app, instance_id, dependency_id.unwrap(), visited).await?;
        }
        install_version(state, app, instance_id, &project, &version, project_type)
            .await
            .map(Some)
    })
}

fn remove_previous_versions(directory: &std::path::Path, project_id: &str, keep_file: &str) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with('.') || !name.ends_with(".xal.json") {
            continue;
        }
        let Ok(bytes) = std::fs::read(entry.path()) else {
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
        let _ = std::fs::remove_file(entry.path());
        let _ = std::fs::remove_file(directory.join(file_name));
        let _ = std::fs::remove_file(directory.join(format!("{file_name}.disabled")));
    }
}

async fn install_version(
    state: &crate::AppState,
    app: &AppHandle,
    instance_id: &str,
    project: &Value,
    version: &Value,
    project_type: &str,
) -> Result<ContentEntry> {
    let files = version["files"]
        .as_array()
        .ok_or_else(|| AppError::new("Publication Modrinth sans fichier"))?;
    let file = files
        .iter()
        .find(|file| file["primary"] == true)
        .or_else(|| files.first())
        .ok_or_else(|| AppError::new("Publication Modrinth sans fichier"))?;
    let file_name = file["filename"]
        .as_str()
        .ok_or_else(|| AppError::new("Nom de fichier Modrinth absent"))?;
    if file_name.contains(['/', '\\']) {
        return Err(AppError::new("Nom de fichier distant dangereux"));
    }
    let folder = match project_type {
        "mod" => "mods",
        "resourcepack" => "resourcepacks",
        "shader" => "shaderpacks",
        _ => return Err(AppError::new("Type de contenu non pris en charge")),
    };
    let game_dir = state.store.game_dir(instance_id);
    let destination = game_dir.join(folder).join(file_name);
    let url = file["url"]
        .as_str()
        .ok_or_else(|| AppError::new("URL de téléchargement absente"))?;
    let sha1 = file["hashes"]["sha1"].as_str();
    download_file(
        &state.client,
        url,
        &destination,
        sha1,
        Some(app),
        "content",
        project["title"].as_str().unwrap_or(file_name),
    )
    .await?;
    remove_previous_versions(
        &game_dir.join(folder),
        project["id"].as_str().unwrap_or(""),
        file_name,
    );
    let metadata = ContentMetadata {
        project_id: project["id"].as_str().unwrap_or("").into(),
        project_title: project["title"].as_str().unwrap_or(file_name).into(),
        version_id: version["id"].as_str().unwrap_or("").into(),
        icon_url: project["icon_url"].as_str().map(str::to_string),
    };
    let sidecar = game_dir.join(folder).join(format!(".{file_name}.xal.json"));
    std::fs::write(sidecar, serde_json::to_vec_pretty(&metadata)?)?;
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
    })
}
