use crate::content_meta;
use crate::models::ContentEntry;
use std::path::Path;

fn provider_matches(stored: Option<&str>, expected: &str) -> bool {
    stored == Some(expected) || (expected == "modrinth" && stored.is_none())
}

pub(crate) fn installed_content(
    store: &crate::store::Store,
    instance_id: &str,
    provider: &str,
    project_id: &str,
    version_id: Option<&str>,
) -> Option<ContentEntry> {
    let instance_dir = store.instance_dir(instance_id);
    let game_dir = store.game_dir(instance_id);
    for (kind, folder) in [
        ("mod", "mods"),
        ("resourcepack", "resourcepacks"),
        ("shader", "shaderpacks"),
    ] {
        content_meta::migrate_folder(&instance_dir, folder);
        for (file_name, metadata) in content_meta::entries(&instance_dir, folder) {
            if metadata.project_id != project_id
                || !provider_matches(metadata.provider.as_deref(), provider)
                || version_id.is_some_and(|id| metadata.version_id != id)
            {
                continue;
            }
            let directory = game_dir.join(folder);
            let enabled_path = directory.join(&file_name);
            let disabled_path = directory.join(format!("{file_name}.disabled"));
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
    instance_dir: &Path,
    folder: &str,
    provider: &str,
    project_id: &str,
    keep_file: &str,
) {
    let directory = instance_dir.join("game").join(folder);
    let keep_file = keep_file.strip_suffix(".disabled").unwrap_or(keep_file);
    content_meta::migrate_folder(instance_dir, folder);
    for (file_name, metadata) in content_meta::entries(instance_dir, folder) {
        if metadata.project_id != project_id
            || !provider_matches(metadata.provider.as_deref(), provider)
            || file_name == keep_file
        {
            continue;
        }
        content_meta::remove(instance_dir, folder, &file_name);
        let _ = std::fs::remove_file(directory.join(&file_name));
        let _ = std::fs::remove_file(directory.join(format!("{file_name}.disabled")));
    }
}

#[cfg(test)]
mod tests {
    use super::{installed_content, remove_previous_versions};
    use crate::content_meta::ContentMetadata;
    use crate::store::Store;
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
        let store = Store::temporary(directory.path());
        let mods = store.game_dir("instance").join("mods");
        fs::create_dir_all(&mods).unwrap();
        fs::write(mods.join("legacy.jar"), b"mod").unwrap();
        fs::write(
            mods.join(".legacy.jar.xal.json"),
            serde_json::to_vec(&metadata("42", "1", None)).unwrap(),
        )
        .unwrap();

        assert!(installed_content(&store, "instance", "modrinth", "42", None).is_some());
        assert!(installed_content(&store, "instance", "curseforge", "42", None).is_none());
        assert!(!mods.join(".legacy.jar.xal.json").exists());
    }

    #[test]
    fn removes_only_matching_provider_versions() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::temporary(directory.path());
        let mods = store.game_dir("instance").join("mods");
        fs::create_dir_all(&mods).unwrap();
        fs::write(
            mods.join(".old.jar.xal.json"),
            serde_json::to_vec(&metadata("42", "1", Some("curseforge"))).unwrap(),
        )
        .unwrap();
        fs::write(mods.join("old.jar"), b"old").unwrap();
        fs::write(
            mods.join(".keep.jar.xal.json"),
            serde_json::to_vec(&metadata("42", "2", Some("modrinth"))).unwrap(),
        )
        .unwrap();
        fs::write(mods.join("keep.jar"), b"keep").unwrap();

        remove_previous_versions(
            &store.instance_dir("instance"),
            "mods",
            "curseforge",
            "42",
            "new.jar",
        );

        assert!(!mods.join("old.jar").exists());
        assert!(mods.join("keep.jar").exists());
        assert!(!store
            .instance_dir("instance")
            .join("metadata/mods/old.jar.xal.json")
            .exists());
    }
}
