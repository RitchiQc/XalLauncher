use crate::download::download_file;
use crate::error::{AppError, Result, ResultExt};
use crate::http::get_json;
use crate::models::{Instance, Loader};
use crate::store::Store;
use crate::versions::{forge_installer_url, neoforge_installer_url, version_json};
use serde_json::Value;
use std::path::Path;
use tauri::AppHandle;

pub async fn prepare(
    client: &reqwest::Client,
    store: &Store,
    instance: &Instance,
    java: &Path,
    app: Option<&AppHandle>,
) -> Result<(String, Value)> {
    let loader_version = instance.loader_version.as_deref().unwrap_or("");
    match instance.loader {
        Loader::Vanilla => Ok((
            instance.version_id.clone(),
            version_json(client, &instance.version_id).await?,
        )),
        Loader::Fabric | Loader::Quilt => {
            let base = if instance.loader == Loader::Fabric {
                "https://meta.fabricmc.net/v2"
            } else {
                "https://meta.quiltmc.org/v3"
            };
            let url = format!(
                "{base}/versions/loader/{}/{loader_version}/profile/json",
                instance.version_id
            );
            let profile: Value = get_json(client, &url).await?;
            let id = profile["id"]
                .as_str()
                .unwrap_or(&instance.version_id)
                .to_string();
            Ok((id, profile))
        }
        Loader::Forge | Loader::Neoforge => {
            if let Ok(id) = find_installed_profile(store, instance) {
                let path = store
                    .root()
                    .join("versions")
                    .join(&id)
                    .join(format!("{id}.json"));
                if let Ok(content) = std::fs::read_to_string(path) {
                    if let Ok(profile) = serde_json::from_str(&content) {
                        return Ok((id, profile));
                    }
                }
            }
            let is_forge = instance.loader == Loader::Forge;
            let url = if is_forge {
                forge_installer_url(loader_version)
            } else {
                neoforge_installer_url(loader_version)
            };
            let installer = store.root().join("cache").join(format!(
                "{}-{loader_version}-installer.jar",
                if is_forge { "forge" } else { "neoforge" }
            ));
            download_file(
                client,
                &url,
                &installer,
                None,
                app,
                "launch",
                "Installateur du modloader",
            )
            .await?;
            let output = tokio::process::Command::new(java)
                .arg("-jar")
                .arg(&installer)
                .arg("--installClient")
                .arg(store.root())
                .current_dir(store.root())
                .output()
                .await
                .context("Exécution de l’installateur du modloader")?;
            if !output.status.success() {
                let log = String::from_utf8_lossy(&output.stderr);
                let log = if log.trim().is_empty() {
                    String::from_utf8_lossy(&output.stdout)
                } else {
                    log
                };
                return Err(AppError::new(format!(
                    "L’installateur du modloader a échoué : {}",
                    log.chars().take(1200).collect::<String>()
                )));
            }
            let id = find_installed_profile(store, instance)?;
            let path = store
                .root()
                .join("versions")
                .join(&id)
                .join(format!("{id}.json"));
            let profile = serde_json::from_str(&std::fs::read_to_string(path)?)?;
            Ok((id, profile))
        }
    }
}

fn find_installed_profile(store: &Store, instance: &Instance) -> Result<String> {
    let needle = if instance.loader == Loader::Forge {
        "forge"
    } else {
        "neoforge"
    };
    let loader_version = instance.loader_version.as_deref().unwrap_or("");
    let short_version = loader_version
        .strip_prefix(&format!("{}-", instance.version_id))
        .unwrap_or(loader_version);
    let versions = store.root().join("versions");
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(&versions)?.flatten() {
        let id = entry.file_name().to_string_lossy().to_string();
        if !id.to_lowercase().contains(needle) || !id.contains(short_version) {
            continue;
        }
        let json_path = entry.path().join(format!("{id}.json"));
        if !json_path.is_file() {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&json_path) {
            if let Ok(profile) = serde_json::from_str::<Value>(&content) {
                if profile["inheritsFrom"]
                    .as_str()
                    .unwrap_or(&instance.version_id)
                    == instance.version_id
                {
                    let modified = entry
                        .metadata()
                        .and_then(|meta| meta.modified())
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                    candidates.push((modified, id));
                }
            }
        }
    }
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.0));
    candidates
        .into_iter()
        .next()
        .map(|(_, id)| id)
        .ok_or_else(|| AppError::new("Le profil installé par le modloader est introuvable"))
}

pub fn maven_path(coordinate: &str) -> Result<String> {
    let (coordinate, extension) = coordinate.split_once('@').unwrap_or((coordinate, "jar"));
    let parts: Vec<&str> = coordinate.split(':').collect();
    if parts.len() < 3 || parts.len() > 4 {
        return Err(AppError::new(format!(
            "Coordonnée Maven invalide : {coordinate}"
        )));
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts
        .get(3)
        .map(|value| format!("-{value}"))
        .unwrap_or_default();
    Ok(format!(
        "{group}/{artifact}/{version}/{artifact}-{version}{classifier}.{extension}"
    ))
}

#[cfg(test)]
mod tests {
    use super::maven_path;

    #[test]
    fn converts_maven_coordinates() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.16.9").unwrap(),
            "net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar"
        );
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-windows").unwrap(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar"
        );
        assert_eq!(
            maven_path("com.example:lib:1.0@zip").unwrap(),
            "com/example/lib/1.0/lib-1.0.zip"
        );
        assert!(maven_path("invalid").is_err());
    }
}
