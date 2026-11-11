use crate::download::{download_file_with_checksum, Checksum};
use crate::error::{AppError, Result};
use crate::http::{ensure_success, get_json};
use crate::models::{ContentEntry, Loader, ProgressEvent, ProjectHit};
use futures_util::{stream, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::{AppHandle, Emitter};

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
    index: Option<String>,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Vec<ProjectHit>> {
    let index = match index.as_deref() {
        Some("downloads") => "downloads",
        Some("follows") => "follows",
        Some("newest") => "newest",
        Some("updated") => "updated",
        _ => "relevance",
    };
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
            ("index", index.into()),
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthVersion {
    pub id: String,
    pub name: String,
    pub version_number: String,
    pub version_type: String,
    pub date_published: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
}

fn version_summary(version: &Value) -> ModrinthVersion {
    ModrinthVersion {
        id: version["id"].as_str().unwrap_or_default().into(),
        name: version["name"].as_str().unwrap_or_default().into(),
        version_number: version["version_number"]
            .as_str()
            .unwrap_or_default()
            .into(),
        version_type: version["version_type"].as_str().unwrap_or("release").into(),
        date_published: version["date_published"]
            .as_str()
            .unwrap_or_default()
            .into(),
        game_versions: serde_json::from_value(version["game_versions"].clone()).unwrap_or_default(),
        loaders: serde_json::from_value(version["loaders"].clone()).unwrap_or_default(),
    }
}

async fn compatible_versions(
    state: &crate::AppState,
    instance: &crate::models::Instance,
    project_id: &str,
    project_type: &str,
) -> Result<Vec<Value>> {
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
    ensure_success(request.send().await?, "Recherche d’une version compatible")
        .await?
        .json()
        .await
        .map_err(Into::into)
}

fn select_version(
    versions: Vec<Value>,
    version_id: Option<&str>,
    project_title: &str,
) -> Result<Value> {
    if let Some(selected) = version_id {
        return versions
            .into_iter()
            .find(|version| version["id"].as_str() == Some(selected))
            .ok_or_else(|| {
                AppError::new(format!(
                    "La version sélectionnée de {project_title} n’est pas compatible avec cette instance"
                ))
            });
    }
    versions
        .into_iter()
        .next()
        .ok_or_else(|| AppError::new(format!("Aucune version compatible pour {project_title}")))
}

struct InstallJob {
    project: Value,
    version: Value,
    project_type: String,
}

fn installed_content(
    game_dir: &std::path::Path,
    project_id: &str,
    version_id: Option<&str>,
) -> Option<ContentEntry> {
    for (kind, folder) in [
        ("mod", "mods"),
        ("resourcepack", "resourcepacks"),
        ("shader", "shaderpacks"),
    ] {
        let directory = game_dir.join(folder);
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
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
            if metadata.project_id != project_id
                || version_id.is_some_and(|id| metadata.version_id != id)
            {
                continue;
            }
            let base = name.trim_start_matches('.').strip_suffix(".xal.json")?;
            let enabled_path = directory.join(base);
            let disabled_path = directory.join(format!("{base}.disabled"));
            let (path, enabled) = if enabled_path.is_file() {
                (enabled_path, true)
            } else if disabled_path.is_file() {
                (disabled_path, false)
            } else {
                continue;
            };
            let file_meta = path.metadata().ok()?;
            return Some(ContentEntry {
                file_name: path.file_name()?.to_string_lossy().into_owned(),
                kind: kind.into(),
                enabled,
                size: file_meta.len(),
                modified_at: file_meta
                    .modified()
                    .map(chrono::DateTime::<chrono::Utc>::from)
                    .unwrap_or_else(|_| chrono::Utc::now()),
                project_id: Some(metadata.project_id),
                project_title: Some(metadata.project_title),
                version_id: Some(metadata.version_id),
                icon_url: metadata.icon_url,
            });
        }
    }
    None
}

fn ensure_version_compatible(
    version: &Value,
    instance: &crate::models::Instance,
    project_type: &str,
    project_title: &str,
) -> Result<()> {
    if project_type == "mod" && instance.loader == Loader::Vanilla {
        return Err(AppError::new(
            "Un modloader est requis pour installer ce mod",
        ));
    }
    let game_compatible = version["game_versions"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|game| game.as_str() == Some(instance.version_id.as_str()));
    let loader_compatible = project_type != "mod"
        || version["loaders"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|loader| loader.as_str() == Some(loader_name(instance.loader)));
    if game_compatible && loader_compatible {
        Ok(())
    } else {
        Err(AppError::new(format!(
            "La version sélectionnée de {project_title} n’est pas compatible avec cette instance"
        )))
    }
}

async fn project_version(
    state: &crate::AppState,
    instance: &crate::models::Instance,
    project_id: &str,
    requested_version: Option<&str>,
) -> Result<(Value, Value)> {
    let project_url = format!("https://api.modrinth.com/v2/project/{project_id}");
    let project_request = get_json(&state.client, &project_url);
    if let Some(version_id) = requested_version {
        let version_url = format!("https://api.modrinth.com/v2/version/{version_id}");
        let version_request = get_json(&state.client, &version_url);
        let (project, version): (Value, Value) =
            tokio::try_join!(project_request, version_request)?;
        let project_type = project["project_type"].as_str().unwrap_or("mod");
        if project_type == "modpack" {
            return Err(AppError::new(
                "Les modpacks s’installent depuis leur page avec le bouton dédié",
            ));
        }
        ensure_version_compatible(
            &version,
            instance,
            project_type,
            project["title"].as_str().unwrap_or(project_id),
        )?;
        return Ok((project, version));
    }
    let project: Value = project_request.await?;
    let project_type = project["project_type"].as_str().unwrap_or("mod");
    if project_type == "modpack" {
        return Err(AppError::new(
            "Les modpacks s’installent depuis leur page avec le bouton dédié",
        ));
    }
    let title = project["title"].as_str().unwrap_or(project_id);
    let versions = compatible_versions(state, instance, project_id, project_type).await?;
    let version = select_version(versions, None, title)?;
    Ok((project, version))
}

#[tauri::command]
pub async fn list_modrinth_versions(
    project_id: String,
    instance_id: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Vec<ModrinthVersion>> {
    let instance = state.store.instance(&instance_id)?;
    let project: Value = get_json(
        &state.client,
        &format!("https://api.modrinth.com/v2/project/{project_id}"),
    )
    .await?;
    let project_type = project["project_type"].as_str().unwrap_or("mod");
    if project_type == "modpack" {
        return Err(AppError::new(
            "Les modpacks ne s’installent pas dans une instance",
        ));
    }
    Ok(
        compatible_versions(&state, &instance, &project_id, project_type)
            .await?
            .iter()
            .map(version_summary)
            .collect(),
    )
}

#[tauri::command]
pub async fn install_modrinth_project(
    project_id: String,
    instance_id: String,
    version_id: Option<String>,
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<ContentEntry> {
    let mut visited = std::collections::HashSet::new();
    let mut jobs = Vec::new();
    let existing = resolve_project_tree(
        &state,
        &instance_id,
        &project_id,
        version_id.as_deref(),
        &mut visited,
        &mut jobs,
    )
    .await?;
    if jobs.is_empty() {
        return existing.ok_or_else(|| AppError::new("Ce contenu est déjà installé"));
    }

    let total = jobs.len();
    let concurrency = state.store.settings()?.download_concurrency.clamp(1, 32);
    let completed = std::sync::Arc::new(AtomicUsize::new(0));
    let _ = app.emit(
        "download-progress",
        ProgressEvent {
            operation: "content",
            label: "Installation des contenus",
            current: 0,
            total: total as u64,
        },
    );
    let state_ref = &state;
    let instance_id_ref = instance_id.as_str();
    let entries = stream::iter(jobs.into_iter().map(|job| {
        let app = app.clone();
        let completed = completed.clone();
        async move {
            let entry = install_version(
                state_ref,
                instance_id_ref,
                &job.project,
                &job.version,
                &job.project_type,
            )
            .await?;
            let current = completed.fetch_add(1, Ordering::Relaxed) + 1;
            let _ = app.emit(
                "download-progress",
                ProgressEvent {
                    operation: "content",
                    label: "Installation des contenus",
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
    entries
        .into_iter()
        .find(|entry| entry.project_id.as_deref() == Some(project_id.as_str()))
        .or(existing)
        .ok_or_else(|| AppError::new("Contenu installé mais introuvable"))
}

fn resolve_project_tree<'a>(
    state: &'a crate::AppState,
    instance_id: &'a str,
    project_id: &'a str,
    version_id: Option<&'a str>,
    visited: &'a mut std::collections::HashSet<String>,
    jobs: &'a mut Vec<InstallJob>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<ContentEntry>>> + Send + 'a>>
{
    Box::pin(async move {
        if !visited.insert(project_id.to_string()) {
            return Ok(None);
        }
        if let Some(installed) =
            installed_content(&state.store.game_dir(instance_id), project_id, version_id)
        {
            return Ok(Some(installed));
        }
        let instance = state.store.instance(instance_id)?;
        let (project, version) = project_version(state, &instance, project_id, version_id).await?;
        let project_type = project["project_type"]
            .as_str()
            .unwrap_or("mod")
            .to_string();
        for dependency in version["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|dependency| dependency["dependency_type"] == "required")
        {
            let Some(dependency_id) = dependency["project_id"].as_str() else {
                continue;
            };
            resolve_project_tree(
                state,
                instance_id,
                dependency_id,
                dependency["version_id"].as_str(),
                visited,
                jobs,
            )
            .await?;
        }
        jobs.push(InstallJob {
            project,
            version,
            project_type,
        });
        Ok(None)
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
    let checksum = file["hashes"]["sha512"]
        .as_str()
        .map(|hash| Checksum::Sha512(hash.to_string()))
        .or_else(|| {
            file["hashes"]["sha1"]
                .as_str()
                .map(|hash| Checksum::Sha1(hash.to_string()))
        })
        .ok_or_else(|| AppError::new("Publication Modrinth sans checksum"))?;
    download_file_with_checksum(
        &state.client,
        url,
        &destination,
        Some(&checksum),
        None,
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

#[cfg(test)]
mod tests {
    use super::{installed_content, select_version, ContentMetadata};
    use serde_json::json;

    #[test]
    fn selects_latest_or_explicit_version() {
        let versions = || vec![json!({ "id": "new" }), json!({ "id": "old" })];
        assert_eq!(
            select_version(versions(), None, "Sodium").unwrap()["id"],
            "new"
        );
        assert_eq!(
            select_version(versions(), Some("old"), "Sodium").unwrap()["id"],
            "old"
        );
    }

    #[test]
    fn rejects_unavailable_explicit_version() {
        let error = select_version(vec![json!({ "id": "new" })], Some("old"), "Sodium")
            .unwrap_err()
            .to_string();
        assert!(error.contains("n’est pas compatible"));
    }

    #[test]
    fn recognizes_installed_and_disabled_versions() {
        let directory = tempfile::tempdir().unwrap();
        let mods = directory.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("sodium.jar.disabled"), b"mod").unwrap();
        let metadata = ContentMetadata {
            project_id: "sodium".into(),
            project_title: "Sodium".into(),
            version_id: "new".into(),
            icon_url: None,
        };
        std::fs::write(
            mods.join(".sodium.jar.xal.json"),
            serde_json::to_vec(&metadata).unwrap(),
        )
        .unwrap();

        let installed = installed_content(directory.path(), "sodium", Some("new")).unwrap();
        assert_eq!(installed.file_name, "sodium.jar.disabled");
        assert!(!installed.enabled);
        assert!(installed_content(directory.path(), "sodium", Some("old")).is_none());
    }

    #[test]
    fn finds_content_when_another_content_folder_is_missing() {
        let directory = tempfile::tempdir().unwrap();
        let packs = directory.path().join("resourcepacks");
        std::fs::create_dir_all(&packs).unwrap();
        std::fs::write(packs.join("pack.zip"), b"pack").unwrap();
        let metadata = ContentMetadata {
            project_id: "pack".into(),
            project_title: "Pack".into(),
            version_id: "v1".into(),
            icon_url: None,
        };
        std::fs::write(
            packs.join(".pack.zip.xal.json"),
            serde_json::to_vec(&metadata).unwrap(),
        )
        .unwrap();

        assert!(installed_content(directory.path(), "pack", Some("v1")).is_some());
    }
}
