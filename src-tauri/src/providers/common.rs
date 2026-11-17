use crate::error::Result;
use crate::models::ContentEntry;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContentMetadata {
    pub project_id: String,
    pub project_title: String,
    pub version_id: String,
    pub icon_url: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
}

fn provider_matches(stored: Option<&str>, expected: &str) -> bool {
    stored == Some(expected) || (expected == "modrinth" && stored.is_none())
}

pub(crate) fn installed_content(
    game_dir: &Path,
    provider: &str,
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
                || !provider_matches(metadata.provider.as_deref(), provider)
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
                provider: metadata.provider,
            });
        }
    }
    None
}

pub(crate) fn remove_previous_versions(
    directory: &Path,
    provider: &str,
    project_id: &str,
    keep_file: &str,
) {
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
        if metadata.project_id != project_id
            || !provider_matches(metadata.provider.as_deref(), provider)
        {
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

pub(crate) fn write_sidecar(
    directory: &Path,
    file_name: &str,
    metadata: &ContentMetadata,
) -> Result<()> {
    let sidecar = directory.join(format!(".{file_name}.xal.json"));
    std::fs::write(sidecar, serde_json::to_vec_pretty(metadata)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{installed_content, remove_previous_versions, ContentMetadata};
    use std::fs;

    fn metadata(project_id: &str, version_id: &str, provider: Option<&str>) -> ContentMetadata {
        ContentMetadata {
            project_id: project_id.into(),
            project_title: "Project".into(),
            version_id: version_id.into(),
            icon_url: None,
            provider: provider.map(str::to_string),
        }
    }

    #[test]
    fn treats_legacy_sidecars_as_modrinth_only() {
        let directory = tempfile::tempdir().unwrap();
        let mods = directory.path().join("mods");
        fs::create_dir_all(&mods).unwrap();
        fs::write(mods.join("legacy.jar"), b"mod").unwrap();
        fs::write(
            mods.join(".legacy.jar.xal.json"),
            serde_json::to_vec(&metadata("42", "1", None)).unwrap(),
        )
        .unwrap();

        assert!(installed_content(directory.path(), "modrinth", "42", None).is_some());
        assert!(installed_content(directory.path(), "curseforge", "42", None).is_none());
    }

    #[test]
    fn removes_only_matching_provider_versions() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join(".old.jar.xal.json"),
            serde_json::to_vec(&metadata("42", "1", Some("curseforge"))).unwrap(),
        )
        .unwrap();
        fs::write(directory.path().join("old.jar"), b"old").unwrap();
        fs::write(
            directory.path().join(".keep.jar.xal.json"),
            serde_json::to_vec(&metadata("42", "2", Some("modrinth"))).unwrap(),
        )
        .unwrap();
        fs::write(directory.path().join("keep.jar"), b"keep").unwrap();

        remove_previous_versions(directory.path(), "curseforge", "42", "new.jar");

        assert!(!directory.path().join("old.jar").exists());
        assert!(directory.path().join("keep.jar").exists());
    }
}
