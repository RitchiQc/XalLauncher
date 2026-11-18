use crate::content_meta;
use crate::error::Result;
use crate::http::ensure_success;
use crate::models::Loader;
use crate::providers::fingerprint::{self, FileHashes};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub(crate) struct CachedHashes {
    size: u64,
    modified: Option<SystemTime>,
    hashes: FileHashes,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossProviderMatch {
    pub project_id: String,
    pub file_name: String,
    pub installed_provider: Option<String>,
}

#[derive(Debug, Clone)]
struct Candidate {
    file_name: String,
    provider: Option<String>,
    hashes: FileHashes,
}

#[derive(Debug, Clone)]
struct ResolvedMatch {
    project_id: String,
    file_name: String,
    provider: Option<String>,
}

fn hashes_for(state: &crate::AppState, path: &Path) -> Result<FileHashes> {
    let metadata = std::fs::metadata(path)?;
    if let Ok(cache) = state.hash_cache.lock() {
        if let Some(entry) = cache.get(path) {
            if entry.size == metadata.len() && entry.modified == metadata.modified().ok() {
                return Ok(entry.hashes.clone());
            }
        }
    }
    let hashes = fingerprint::file_hashes(path)?;
    if let Ok(mut cache) = state.hash_cache.lock() {
        cache.insert(
            path.to_path_buf(),
            CachedHashes {
                size: metadata.len(),
                modified: metadata.modified().ok(),
                hashes: hashes.clone(),
            },
        );
    }
    Ok(hashes)
}

fn candidates(
    state: &crate::AppState,
    instance_id: &str,
    kind: &str,
    target: &str,
) -> Result<Vec<Candidate>> {
    let loader = state.store.instance(instance_id)?.loader;
    if loader == Loader::Horizon {
        return Ok(Vec::new());
    }
    let folder = crate::content::folder_for_kind(loader, kind)?;
    if !matches!(folder, "mods" | "resourcepacks" | "shaderpacks") {
        return Ok(Vec::new());
    }
    let instance_dir = state.store.instance_dir(instance_id);
    content_meta::migrate_folder(&instance_dir, folder);
    let directory = state.store.game_dir(instance_id).join(folder);
    let Ok(items) = std::fs::read_dir(directory) else {
        return Ok(Vec::new());
    };
    let mut candidates = Vec::new();
    for item in items.flatten() {
        let name = item.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || item.file_type().map(|kind| kind.is_dir()).unwrap_or(true) {
            continue;
        }
        let provider = content_meta::read(&instance_dir, folder, &name).and_then(|meta| meta.provider);
        if provider.as_deref() == Some(target) {
            continue;
        }
        if let Ok(hashes) = hashes_for(state, &item.path()) {
            candidates.push(Candidate {
                file_name: name,
                provider,
                hashes,
            });
        }
    }
    Ok(candidates)
}

#[derive(Deserialize)]
struct FingerprintResponse {
    data: FingerprintData,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FingerprintData {
    #[serde(default)]
    exact_matches: Vec<FingerprintMatch>,
}

#[derive(Deserialize)]
struct FingerprintMatch {
    #[serde(default)]
    file: Option<FingerprintFile>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FingerprintFile {
    #[serde(default)]
    mod_id: u64,
    #[serde(default)]
    file_fingerprint: u64,
    #[serde(default)]
    file_name: String,
}

async fn curseforge_matches(
    state: &crate::AppState,
    candidates: &[Candidate],
) -> Result<Vec<ResolvedMatch>> {
    let Some(key) = crate::secrets::curseforge_key()? else {
        return Ok(Vec::new());
    };
    let mut by_fingerprint: HashMap<u32, Vec<usize>> = HashMap::new();
    let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, candidate) in candidates.iter().enumerate() {
        by_fingerprint
            .entry(candidate.hashes.curseforge)
            .or_default()
            .push(index);
        by_name
            .entry(candidate.file_name.to_lowercase())
            .or_default()
            .push(index);
    }
    let fingerprints: Vec<u32> = candidates
        .iter()
        .map(|candidate| candidate.hashes.curseforge)
        .collect();
    let mut resolved = Vec::new();
    for chunk in fingerprints.chunks(1000) {
        let response: FingerprintResponse = crate::providers::curseforge::cf_post_json(
            &state.client,
            &key,
            "/fingerprints/432",
            &json!({ "fingerprints": chunk }),
            "Identification des contenus CurseForge",
        )
        .await?;
        for item in response.data.exact_matches {
            let Some(file) = item.file else {
                continue;
            };
            let mut indexes = by_fingerprint
                .get(&(file.file_fingerprint as u32))
                .cloned()
                .unwrap_or_default();
            if indexes.is_empty() {
                indexes = by_name
                    .get(&file.file_name.to_lowercase())
                    .cloned()
                    .unwrap_or_default();
            }
            for index in indexes {
                let candidate = &candidates[index];
                resolved.push(ResolvedMatch {
                    project_id: file.mod_id.to_string(),
                    file_name: candidate.file_name.clone(),
                    provider: candidate.provider.clone(),
                });
            }
        }
    }
    Ok(resolved)
}

async fn modrinth_matches(
    state: &crate::AppState,
    candidates: &[Candidate],
) -> Result<Vec<ResolvedMatch>> {
    let mut by_hash: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, candidate) in candidates.iter().enumerate() {
        by_hash
            .entry(candidate.hashes.sha1.clone())
            .or_default()
            .push(index);
    }
    let hashes: Vec<String> = candidates
        .iter()
        .map(|candidate| candidate.hashes.sha1.clone())
        .collect();
    let mut resolved = Vec::new();
    for chunk in hashes.chunks(500) {
        let response = state
            .client
            .post("https://api.modrinth.com/v2/version_files")
            .json(&json!({ "hashes": chunk, "algorithm": "sha1" }))
            .send()
            .await?;
        let versions: HashMap<String, Value> =
            ensure_success(response, "Identification des contenus Modrinth")
                .await?
                .json()
                .await?;
        for (hash, version) in versions {
            let Some(project_id) = version["project_id"].as_str() else {
                continue;
            };
            if let Some(indexes) = by_hash.get(&hash) {
                for index in indexes {
                    let candidate = &candidates[*index];
                    resolved.push(ResolvedMatch {
                        project_id: project_id.to_string(),
                        file_name: candidate.file_name.clone(),
                        provider: candidate.provider.clone(),
                    });
                }
            }
        }
    }
    Ok(resolved)
}


fn dedup(matches: Vec<ResolvedMatch>) -> Vec<CrossProviderMatch> {
    let mut seen = HashSet::new();
    matches
        .into_iter()
        .filter_map(|item| {
            seen.insert((item.project_id.clone(), item.file_name.clone()))
                .then(|| CrossProviderMatch {
                    project_id: item.project_id,
                    file_name: item.file_name,
                    installed_provider: item.provider,
                })
        })
        .collect()
}

#[tauri::command]
pub async fn cross_provider_matches(
    instance_id: String,
    kind: String,
    provider: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Vec<CrossProviderMatch>> {
    if !matches!(provider.as_str(), "modrinth" | "curseforge") {
        return Ok(Vec::new());
    }
    let candidates = candidates(&state, &instance_id, &kind, &provider)?;
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let matches = if provider == "curseforge" {
        curseforge_matches(&state, &candidates).await?
    } else {
        modrinth_matches(&state, &candidates).await?
    };
    Ok(dedup(matches))
}

pub(crate) async fn remove_duplicates(
    state: &crate::AppState,
    instance_id: &str,
    provider: &str,
    targets: &[(String, String)],
) -> Result<Vec<String>> {
    if !matches!(provider, "modrinth" | "curseforge") {
        return Ok(Vec::new());
    }
    let mut by_kind: HashMap<String, HashSet<String>> = HashMap::new();
    for (kind, project_id) in targets {
        by_kind
            .entry(kind.clone())
            .or_default()
            .insert(project_id.clone());
    }
    let loader = state.store.instance(instance_id)?.loader;
    let instance_dir = state.store.instance_dir(instance_id);
    let game_dir = state.store.game_dir(instance_id);
    let mut removed = Vec::new();
    let mut seen = HashSet::new();
    for (kind, project_ids) in by_kind {
        let folder = crate::content::folder_for_kind(loader, &kind)?;
        if !matches!(folder, "mods" | "resourcepacks" | "shaderpacks") {
            continue;
        }
        let candidates = candidates(state, instance_id, &kind, provider)?;
        if candidates.is_empty() {
            continue;
        }
        let matches = if provider == "curseforge" {
            curseforge_matches(state, &candidates).await?
        } else {
            modrinth_matches(state, &candidates).await?
        };
        for item in matches {
            if !project_ids.contains(&item.project_id)
                || !seen.insert(format!("{folder}/{}", item.file_name))
            {
                continue;
            }
            content_meta::remove(&instance_dir, folder, &item.file_name);
            let directory = game_dir.join(folder);
            let _ = std::fs::remove_file(directory.join(&item.file_name));
            let _ = std::fs::remove_file(directory.join(format!("{}.disabled", item.file_name)));
            removed.push(item.file_name);
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::{dedup, ResolvedMatch};
    use crate::providers::fingerprint::curseforge_fingerprint;

    #[test]
    fn deduplicates_resolved_matches() {
        let matches = vec![
            ResolvedMatch {
                project_id: "42".into(),
                file_name: "mod.jar".into(),
                provider: Some("modrinth".into()),
            },
            ResolvedMatch {
                project_id: "42".into(),
                file_name: "mod.jar".into(),
                provider: Some("modrinth".into()),
            },
        ];
        let deduped = dedup(matches);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].project_id, "42");
    }

    #[test]
    fn computes_curseforge_fingerprints() {
        assert_eq!(curseforge_fingerprint(&[]), 0x5bd1_5e36);
        assert_eq!(
            curseforge_fingerprint(b"abc"),
            curseforge_fingerprint(b" a\tb\nc\r ")
        );
    }
}

