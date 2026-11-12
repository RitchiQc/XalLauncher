use crate::error::Result;
use std::fs;
use std::path::{Path, PathBuf};

pub fn backup_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.xal-backup", path.display()))
}

pub fn backup_once(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let backup = backup_path(path);
    if !backup.exists() {
        fs::copy(path, backup)?;
    }
    Ok(())
}

pub fn restore(path: &Path) -> Result<bool> {
    let backup = backup_path(path);
    if !backup.exists() {
        return Ok(false);
    }
    fs::copy(&backup, path)?;
    fs::remove_file(backup)?;
    Ok(true)
}

pub fn read(path: &Path) -> Result<Vec<(String, String)>> {
    let Ok(content) = fs::read_to_string(path) else {
        return Ok(Vec::new());
    };
    Ok(content
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            Some((key.trim().to_string(), value.to_string()))
        })
        .collect())
}

pub fn write(path: &Path, entries: &[(String, String)]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = entries
        .iter()
        .map(|(key, value)| format!("{key}:{value}"))
        .collect::<Vec<_>>()
        .join("\n");
    let temp = path.with_extension("txt.xal-tmp");
    fs::write(&temp, content)?;
    fs::rename(&temp, path)?;
    Ok(())
}

pub fn get<'a>(entries: &'a [(String, String)], key: &str) -> Option<&'a str> {
    entries
        .iter()
        .find(|(entry_key, _)| entry_key == key)
        .map(|(_, value)| value.as_str())
}

pub fn set(entries: &mut Vec<(String, String)>, key: &str, value: String) {
    if let Some(entry) = entries.iter_mut().find(|(entry_key, _)| entry_key == key) {
        entry.1 = value;
    } else {
        entries.push((key.to_string(), value));
    }
}

fn list(entries: &[(String, String)], key: &str) -> Vec<String> {
    get(entries, key)
        .and_then(|value| serde_json::from_str::<Vec<String>>(value).ok())
        .unwrap_or_default()
}

pub fn ensure_list(entries: &mut Vec<(String, String)>, key: &str, item: &str) -> Result<()> {
    let mut items = list(entries, key);
    if !items.iter().any(|existing| existing == item) {
        items.push(item.to_string());
    }
    set(entries, key, serde_json::to_string(&items)?);
    Ok(())
}

pub fn remove_from_list(entries: &mut Vec<(String, String)>, key: &str, item: &str) -> Result<()> {
    let items = list(entries, key);
    if items.is_empty() {
        return Ok(());
    }
    let filtered = items
        .into_iter()
        .filter(|existing| existing != item)
        .collect::<Vec<_>>();
    set(entries, key, serde_json::to_string(&filtered)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_list, read, remove_from_list, set, write};

    #[test]
    fn updates_only_known_keys() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("options.txt");
        std::fs::write(&path, "renderDistance:12\nfov:0.5\n").unwrap();
        let mut entries = read(&path).unwrap();
        set(&mut entries, "renderDistance", "8".into());
        assert_eq!(
            entries
                .iter()
                .find(|(key, _)| key == "renderDistance")
                .unwrap()
                .1,
            "8"
        );
        assert_eq!(
            entries.iter().find(|(key, _)| key == "fov").unwrap().1,
            "0.5"
        );
    }

    #[test]
    fn merges_resource_pack_lists() {
        let mut entries = vec![("resourcePacks".to_string(), "[\"vanilla\"]".to_string())];
        ensure_list(
            &mut entries,
            "resourcePacks",
            "file/XalLauncher-Branding.zip",
        )
        .unwrap();
        ensure_list(
            &mut entries,
            "resourcePacks",
            "file/XalLauncher-Branding.zip",
        )
        .unwrap();
        assert_eq!(
            entries
                .iter()
                .find(|(key, _)| key == "resourcePacks")
                .unwrap()
                .1,
            "[\"vanilla\",\"file/XalLauncher-Branding.zip\"]"
        );
        remove_from_list(
            &mut entries,
            "resourcePacks",
            "file/XalLauncher-Branding.zip",
        )
        .unwrap();
        assert_eq!(
            entries
                .iter()
                .find(|(key, _)| key == "resourcePacks")
                .unwrap()
                .1,
            "[\"vanilla\"]"
        );
    }

    #[test]
    fn writes_round_trip() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("options.txt");
        let entries = vec![("maxFps".to_string(), "260".to_string())];
        write(&path, &entries).unwrap();
        assert_eq!(read(&path).unwrap(), entries);
    }
}
