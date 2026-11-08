use crate::error::{AppError, Result};
use crate::http::get_json;
use crate::models::{Loader, LoaderVersion, MinecraftVersion};
use serde::Deserialize;
use serde_json::Value;

pub const VERSION_MANIFEST: &str =
    "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Deserialize)]
struct VersionManifest {
    versions: Vec<ManifestVersion>,
}
#[derive(Deserialize)]
struct ManifestVersion {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "releaseTime")]
    release_time: String,
    url: String,
}

async fn manifest(client: &reqwest::Client) -> Result<VersionManifest> {
    get_json(client, VERSION_MANIFEST).await
}

pub async fn version_json(client: &reqwest::Client, id: &str) -> Result<Value> {
    let manifest = manifest(client).await?;
    let entry = manifest
        .versions
        .into_iter()
        .find(|version| version.id == id)
        .ok_or_else(|| AppError::new(format!("Version Minecraft inconnue : {id}")))?;
    get_json(client, &entry.url).await
}

#[tauri::command]
pub async fn list_versions(
    state: tauri::State<'_, crate::AppState>,
) -> Result<Vec<MinecraftVersion>> {
    let manifest = manifest(&state.client).await?;
    Ok(manifest
        .versions
        .into_iter()
        .filter(|version| matches!(version.kind.as_str(), "release" | "snapshot"))
        .map(|version| MinecraftVersion {
            id: version.id,
            kind: version.kind,
            release_time: version.release_time,
        })
        .collect())
}

#[derive(Deserialize)]
struct FabricEntry {
    loader: FabricLoader,
}
#[derive(Deserialize)]
struct FabricLoader {
    version: String,
    #[serde(default)]
    stable: bool,
}

#[tauri::command]
pub async fn list_loader_versions(
    loader: Loader,
    version_id: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Vec<LoaderVersion>> {
    match loader {
        Loader::Vanilla => Ok(Vec::new()),
        Loader::Fabric | Loader::Quilt => {
            let base = if loader == Loader::Fabric {
                "https://meta.fabricmc.net/v2"
            } else {
                "https://meta.quiltmc.org/v3"
            };
            let entries: Vec<FabricEntry> = get_json(
                &state.client,
                &format!("{base}/versions/loader/{version_id}"),
            )
            .await?;
            Ok(entries
                .into_iter()
                .map(|entry| LoaderVersion {
                    version: entry.loader.version,
                    stable: entry.loader.stable,
                })
                .collect())
        }
        Loader::Forge => forge_versions(&state.client, &version_id).await,
        Loader::Neoforge => neoforge_versions(&state.client, &version_id).await,
    }
}

async fn forge_versions(
    client: &reqwest::Client,
    game_version: &str,
) -> Result<Vec<LoaderVersion>> {
    let text = client
        .get("https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml")
        .send()
        .await?
        .text()
        .await?;
    let expression = regex::Regex::new(&format!(
        r"<version>({}-[0-9][^<]+)</version>",
        regex::escape(game_version)
    ))?;
    let mut versions: Vec<LoaderVersion> = expression
        .captures_iter(&text)
        .filter_map(|capture| {
            capture.get(1).map(|value| LoaderVersion {
                version: value.as_str().to_string(),
                stable: !value.as_str().contains("beta"),
            })
        })
        .collect();
    versions.reverse();
    Ok(versions)
}

async fn neoforge_versions(
    client: &reqwest::Client,
    game_version: &str,
) -> Result<Vec<LoaderVersion>> {
    let prefix = game_version.strip_prefix("1.").unwrap_or(game_version);
    let expected = if prefix.matches('.').count() == 0 {
        format!("{prefix}.0")
    } else {
        prefix.to_string()
    };
    let text = client
        .get("https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml")
        .send()
        .await?
        .text()
        .await?;
    let expression = regex::Regex::new(&format!(
        r"<version>({}\.[0-9][^<]*)</version>",
        regex::escape(&expected)
    ))?;
    let mut versions: Vec<LoaderVersion> = expression
        .captures_iter(&text)
        .filter_map(|capture| {
            capture.get(1).map(|value| LoaderVersion {
                version: value.as_str().to_string(),
                stable: !["beta", "alpha", "rc"]
                    .iter()
                    .any(|part| value.as_str().contains(part)),
            })
        })
        .collect();
    versions.reverse();
    Ok(versions)
}

pub fn forge_installer_url(version: &str) -> String {
    format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{version}/forge-{version}-installer.jar")
}

pub fn neoforge_installer_url(version: &str) -> String {
    format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{version}/neoforge-{version}-installer.jar")
}
