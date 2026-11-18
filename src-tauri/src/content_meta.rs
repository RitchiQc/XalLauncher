use crate::error::{AppError, Result};
use crate::models::Loader;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

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

pub(crate) const CONTENT_FOLDERS: [&str; 5] = [
    "mods",
    "resourcepacks",
    "shaderpacks",
    "horizon-mods",
    "horizon-native-mods",
];

fn content_folders(loader: Loader) -> &'static [&'static str] {
    if loader == Loader::Horizon {
        &[
            "horizon-mods",
            "horizon-native-mods",
            "resourcepacks",
            "shaderpacks",
        ]
    } else {
        &["mods", "resourcepacks", "shaderpacks"]
    }
}

fn base_name(file_name: &str) -> &str {
    file_name.strip_suffix(".disabled").unwrap_or(file_name)
}

fn folder_dir(instance_dir: &Path, folder: &str) -> PathBuf {
    instance_dir.join("metadata").join(folder)
}

pub(crate) fn meta_path(instance_dir: &Path, folder: &str, file_name: &str) -> PathBuf {
    folder_dir(instance_dir, folder).join(format!("{}.xal.json", base_name(file_name)))
}

fn write_json(path: &Path, value: &ContentMetadata) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::new("Dossier de métadonnées invalide"))?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(&serde_json::to_vec_pretty(value)?)?;
    temp.persist(path).map_err(|error| {
        AppError::new(format!(
            "Remplacement de {} : {}",
            path.display(),
            error.error
        ))
    })?;
    Ok(())
}

pub(crate) fn read(
    instance_dir: &Path,
    folder: &str,
    file_name: &str,
) -> Option<ContentMetadata> {
    let direct = folder_dir(instance_dir, folder).join(format!("{file_name}.xal.json"));
    let path = meta_path(instance_dir, folder, file_name);
    for candidate in [path, direct] {
        if let Ok(bytes) = fs::read(candidate) {
            if let Ok(metadata) = serde_json::from_slice(&bytes) {
                return Some(metadata);
            }
        }
    }
    None
}

pub(crate) fn write(
    instance_dir: &Path,
    folder: &str,
    file_name: &str,
    metadata: &ContentMetadata,
) -> Result<()> {
    write_json(&meta_path(instance_dir, folder, file_name), metadata)
}

pub(crate) fn remove(instance_dir: &Path, folder: &str, file_name: &str) {
    let _ = fs::remove_file(meta_path(instance_dir, folder, file_name));
    let _ = fs::remove_file(
        folder_dir(instance_dir, folder).join(format!("{file_name}.xal.json")),
    );
}

pub(crate) fn entries(instance_dir: &Path, folder: &str) -> Vec<(String, ContentMetadata)> {
    let directory = folder_dir(instance_dir, folder);
    let Ok(items) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for item in items.flatten() {
        let name = item.file_name().to_string_lossy().to_string();
        let Some(name) = name.strip_suffix(".xal.json") else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let Ok(bytes) = fs::read(item.path()) else {
            continue;
        };
        let Ok(metadata) = serde_json::from_slice::<ContentMetadata>(&bytes) else {
            continue;
        };
        entries.push((base_name(name).to_string(), metadata));
    }
    entries
}

fn legacy_sidecars(directory: &Path, name: &str) -> Vec<PathBuf> {
    let mut candidates = vec![directory.join(format!(".{name}.xal.json"))];
    if let Some(base) = name.strip_suffix(".disabled") {
        candidates.push(directory.join(format!(".{base}.xal.json")));
    }
    candidates
}

pub(crate) fn migrate_folder(instance_dir: &Path, folder: &str) {
    let directory = instance_dir.join("game").join(folder);
    let Ok(items) = fs::read_dir(&directory) else {
        return;
    };
    for item in items.flatten() {
        let name = item.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let candidates = legacy_sidecars(&directory, &name);
        if let Some(legacy) = candidates.iter().find(|path| path.is_file()) {
            if let Ok(bytes) = fs::read(legacy) {
                if let Ok(metadata) = serde_json::from_slice::<ContentMetadata>(&bytes) {
                    let target = meta_path(instance_dir, folder, &name);
                    if !target.exists() {
                        let _ = write_json(&target, &metadata);
                    }
                }
            }
        }
        for candidate in candidates {
            let _ = fs::remove_file(candidate);
        }
    }
    if let Ok(items) = fs::read_dir(&directory) {
        for item in items.flatten() {
            let name = item.file_name().to_string_lossy().to_string();
            if name.starts_with('.') && name.ends_with(".xal.json") {
                let _ = fs::remove_file(item.path());
            }
        }
    }
}

pub(crate) fn migrate_instance(store: &crate::store::Store, instance_id: &str) -> Result<()> {
    let loader = store
        .instance(instance_id)
        .map(|instance| instance.loader)
        .unwrap_or(Loader::Vanilla);
    let instance_dir = store.instance_dir(instance_id);
    for folder in content_folders(loader) {
        migrate_folder(&instance_dir, folder);
    }
    Ok(())
}

pub(crate) fn migrate_all(store: &crate::store::Store) {
    if let Ok(instances) = store.instances() {
        for instance in instances {
            if let Err(error) = migrate_instance(store, &instance.id) {
                tracing::warn!(
                    "Migration des métadonnées de l’instance {} ignorée : {error}",
                    instance.id
                );
            }
        }
    }
}

pub(crate) fn exists(instance_dir: &Path, folder: &str, file_name: &str) -> bool {
    let base = base_name(file_name);
    let directory = instance_dir.join("game").join(folder);
    directory.join(base).exists() || directory.join(format!("{base}.disabled")).exists()
}

pub(crate) fn export_entries(instance_dir: &Path) -> BTreeMap<String, ContentMetadata> {
    let mut exported = BTreeMap::new();
    for folder in CONTENT_FOLDERS {
        for (file_name, metadata) in entries(instance_dir, folder) {
            if exists(instance_dir, folder, &file_name) {
                exported.insert(format!("{folder}/{file_name}"), metadata);
            }
        }
    }
    exported
}

fn safe_entry(key: &str) -> Result<(&str, &str)> {
    let mut parts = key.split('/');
    let (Some(folder), Some(file_name), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(AppError::new("Chemin de métadonnées invalide"));
    };
    if !CONTENT_FOLDERS.contains(&folder) {
        return Err(AppError::new("Dossier de métadonnées inconnu"));
    }
    let path = Path::new(file_name);
    let mut components = path.components();
    if components.next() != Some(Component::Normal(std::ffi::OsStr::new(file_name)))
        || components.next().is_some()
        || file_name.is_empty()
        || file_name == "."
        || file_name == ".."
        || file_name.contains(['/', '\\'])
    {
        return Err(AppError::new("Nom de fichier de métadonnées invalide"));
    }
    Ok((folder, file_name))
}

pub(crate) fn import_entries(
    instance_dir: &Path,
    entries: &BTreeMap<String, ContentMetadata>,
) -> Result<()> {
    for (key, metadata) in entries {
        let (folder, file_name) = safe_entry(key)?;
        if exists(instance_dir, folder, file_name) {
            write(instance_dir, folder, file_name, metadata)?;
        }
    }
    Ok(())
}

pub(crate) fn export_document(instance_dir: &Path) -> serde_json::Value {
    serde_json::json!({
        "schema": 1,
        "files": export_entries(instance_dir),
    })
}

pub(crate) fn import_document(instance_dir: &Path, value: &serde_json::Value) -> Result<()> {
    let files = value.get("files").cloned().unwrap_or(serde_json::Value::Null);
    let entries: BTreeMap<String, ContentMetadata> = serde_json::from_value(files)?;
    import_entries(instance_dir, &entries)
}

#[cfg(test)]
mod tests {
    use super::{
        entries, exists, export_entries, import_entries, migrate_folder, read, remove, write,
        ContentMetadata,
    };
    use std::collections::BTreeMap;
    use std::fs;

    fn metadata(project_id: &str, provider: Option<&str>) -> ContentMetadata {
        ContentMetadata {
            project_id: project_id.into(),
            project_title: "Project".into(),
            version_id: "1".into(),
            icon_url: None,
            provider: provider.map(str::to_string),
        }
    }

    #[test]
    fn writes_reads_and_removes_metadata_outside_game_folder() {
        let directory = tempfile::tempdir().unwrap();
        let instance = directory.path();
        write(
            instance,
            "mods",
            "sodium.jar.disabled",
            &metadata("sodium", Some("modrinth")),
        )
        .unwrap();

        let path = instance.join("metadata/mods/sodium.jar.xal.json");
        assert!(path.is_file());
        assert!(!instance.join("game/mods").exists());
        assert_eq!(
            read(instance, "mods", "sodium.jar").unwrap().project_id,
            "sodium"
        );
        assert_eq!(
            read(instance, "mods", "sodium.jar.disabled")
                .unwrap()
                .project_id,
            "sodium"
        );

        remove(instance, "mods", "sodium.jar.disabled");
        assert!(!path.exists());
    }

    #[test]
    fn migrates_legacy_sidecars_and_cleans_game_folder() {
        let directory = tempfile::tempdir().unwrap();
        let instance = directory.path();
        let mods = instance.join("game/mods");
        fs::create_dir_all(&mods).unwrap();
        fs::write(mods.join("sodium.jar"), b"jar").unwrap();
        fs::write(
            mods.join(".sodium.jar.xal.json"),
            serde_json::to_vec(&metadata("sodium", Some("modrinth"))).unwrap(),
        )
        .unwrap();
        fs::write(mods.join("other.jar.disabled"), b"jar").unwrap();
        fs::write(
            mods.join(".other.jar.xal.json"),
            serde_json::to_vec(&metadata("other", Some("curseforge"))).unwrap(),
        )
        .unwrap();
        fs::write(mods.join(".orphan.jar.xal.json"), b"{}").unwrap();

        migrate_folder(instance, "mods");

        assert!(!mods.join(".sodium.jar.xal.json").exists());
        assert!(!mods.join(".other.jar.xal.json").exists());
        assert!(!mods.join(".orphan.jar.xal.json").exists());
        assert_eq!(
            read(instance, "mods", "other.jar.disabled")
                .unwrap()
                .provider
                .as_deref(),
            Some("curseforge")
        );
        assert_eq!(entries(instance, "mods").len(), 2);
    }

    #[test]
    fn exports_and_reimports_metadata_for_existing_files() {
        let source = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        fs::create_dir_all(source.path().join("game/mods")).unwrap();
        fs::write(source.path().join("game/mods/sodium.jar"), b"jar").unwrap();
        write(
            source.path(),
            "mods",
            "sodium.jar",
            &metadata("sodium", Some("modrinth")),
        )
        .unwrap();
        write(
            source.path(),
            "mods",
            "missing.jar",
            &metadata("missing", Some("modrinth")),
        )
        .unwrap();

        let exported = export_entries(source.path());
        assert_eq!(exported.len(), 1);

        fs::create_dir_all(destination.path().join("game/mods")).unwrap();
        fs::write(destination.path().join("game/mods/sodium.jar"), b"jar").unwrap();
        import_entries(destination.path(), &exported).unwrap();
        assert_eq!(
            read(destination.path(), "mods", "sodium.jar")
                .unwrap()
                .project_id,
            "sodium"
        );
        assert!(exists(destination.path(), "mods", "sodium.jar"));
    }

    #[test]
    fn rejects_unsafe_metadata_paths() {
        let directory = tempfile::tempdir().unwrap();
        let mut exported = BTreeMap::new();
        exported.insert("../evil.jar".to_string(), metadata("evil", None));
        assert!(import_entries(directory.path(), &exported).is_err());

        let mut wrong_folder = BTreeMap::new();
        wrong_folder.insert("config/a.json".to_string(), metadata("x", None));
        assert!(import_entries(directory.path(), &wrong_folder).is_err());
    }
}
