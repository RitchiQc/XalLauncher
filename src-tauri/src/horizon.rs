use crate::download::{download_file_with_checksum, sha256_file, Checksum};
use crate::error::{AppError, Result, ResultExt};
use crate::http::{ensure_success, get_json};
use crate::models::{Account, Instance, LaunchResult, LoaderVersion};
use crate::store::Store;
use crate::versions::version_json_raw;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager};

pub const HORIZON_MINECRAFT: &str = "26.3";
const RELEASES: &str = "https://api.github.com/repos/horizon-rust/horizon/releases";
const WINDOWS_ASSET: &str = "horizon-windows-x64.zip";

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Clone)]
pub(crate) struct Build {
    pub root: PathBuf,
    pub tag: String,
    pub digest: String,
    pub client: PathBuf,
    pub prepare: PathBuf,
    pub client_api: Value,
    pub prepare_api: Value,
}

struct GameFiles {
    version_json: PathBuf,
    client_jar: PathBuf,
    server_jar: PathBuf,
    libraries_dir: PathBuf,
    assets_dir: PathBuf,
}

pub async fn list_builds(client: &reqwest::Client) -> Result<Vec<LoaderVersion>> {
    let releases: Vec<Release> = get_json(client, &format!("{RELEASES}?per_page=30")).await?;
    let mut seen = HashSet::new();
    Ok(releases
        .into_iter()
        .filter(|release| release.tag_name == "nightly" || release.tag_name.starts_with("nightly-"))
        .filter(|release| seen.insert(release.tag_name.clone()))
        .map(|release| LoaderVersion {
            version: release.tag_name,
            stable: false,
        })
        .collect())
}

async fn release(client: &reqwest::Client, tag: &str) -> Result<Release> {
    let tag = if tag.is_empty() { "nightly" } else { tag };
    get_json(client, &format!("{RELEASES}/tags/{}", urlencoding(tag))).await
}

fn urlencoding(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '~') {
                character.to_string()
            } else {
                character
                    .to_string()
                    .bytes()
                    .map(|byte| format!("%{byte:02X}"))
                    .collect()
            }
        })
        .collect()
}

fn safe_tag(tag: &str) -> String {
    tag.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn parse_sha256sums(text: &str, name: &str) -> Result<String> {
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let hash = parts.next().unwrap_or_default();
        let file = parts.next().unwrap_or_default().trim_start_matches('*');
        if file == name
            && hash.len() == 64
            && hash.chars().all(|character| character.is_ascii_hexdigit())
        {
            return Ok(hash.to_ascii_lowercase());
        }
    }
    Err(AppError::new(format!(
        "Horizon : {name} absent de SHA256SUMS"
    )))
}

fn find_file(root: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    let direct = root.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    if depth == 0 {
        return None;
    }
    for entry in fs::read_dir(root).ok()?.flatten() {
        if entry.file_type().ok()?.is_dir() {
            if let Some(path) = find_file(&entry.path(), name, depth - 1) {
                return Some(path);
            }
        }
    }
    None
}

fn extract_zip(archive: &Path, destination: &Path) -> Result<()> {
    let file = fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| AppError::new("Chemin dangereux dans l’archive Horizon"))?
            .to_path_buf();
        let target = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::File::create(&target)?;
        std::io::copy(&mut entry, &mut output)?;
    }
    Ok(())
}

fn command(program: &Path) -> Command {
    let mut command = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
}

fn run_launch_api(executable: &Path) -> Result<Value> {
    let output = command(executable)
        .arg("--launch-api")
        .current_dir(
            executable
                .parent()
                .ok_or_else(|| AppError::new("Build Horizon invalide"))?,
        )
        .output()
        .context("Lecture de l’API de lancement Horizon")?;
    if !output.status.success() {
        return Err(AppError::new(format!(
            "Horizon refuse --launch-api : {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    serde_json::from_slice(&output.stdout).context("Réponse --launch-api invalide")
}

fn validate_launch_api(api: &Value, name: &str) -> Result<()> {
    if api["launch_api"].as_str().is_none() {
        return Err(AppError::new(format!(
            "{name} ne fournit pas d’API de lancement"
        )));
    }
    if api["minecraft"].as_str() != Some(HORIZON_MINECRAFT) {
        return Err(AppError::new(format!(
            "{name} ne cible pas Minecraft {HORIZON_MINECRAFT}"
        )));
    }
    Ok(())
}

fn has_option(api: &Value, option: &str) -> bool {
    api["options"]
        .as_array()
        .map(|options| {
            options
                .iter()
                .any(|entry| entry["name"].as_str() == Some(option))
        })
        .unwrap_or(false)
}

fn hashes_match(marker: &Value, client: &Path, prepare: &Path) -> bool {
    marker["clientSha"].as_str().is_some_and(|expected| {
        sha256_file(client)
            .ok()
            .is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
    }) && marker["prepareSha"].as_str().is_some_and(|expected| {
        sha256_file(prepare)
            .ok()
            .is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
    })
}

fn cached_build(store: &Store, tag: &str) -> Option<Build> {
    let builds = store.root().join("horizon").join("builds");
    let mut candidates: Vec<PathBuf> = fs::read_dir(&builds)
        .ok()?
        .flatten()
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .map(|entry| entry.path())
        .collect();
    candidates.sort_by_key(|path| {
        std::cmp::Reverse(
            fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
        )
    });
    for build_dir in candidates {
        let Some(marker) = fs::read_to_string(build_dir.join("build.json"))
            .ok()
            .and_then(|content| serde_json::from_str::<Value>(&content).ok())
        else {
            continue;
        };
        let Some(cached_tag) = marker["tag"].as_str() else {
            continue;
        };
        if cached_tag != tag && !(tag == "nightly" && cached_tag.starts_with("nightly")) {
            continue;
        }
        let (Some(client_exe), Some(prepare_exe)) = (
            find_file(&build_dir, "horizon-client.exe", 4),
            find_file(&build_dir, "horizon-prepare.exe", 4),
        ) else {
            continue;
        };
        if !hashes_match(&marker, &client_exe, &prepare_exe) {
            continue;
        }
        let (Ok(client_api), Ok(prepare_api)) =
            (run_launch_api(&client_exe), run_launch_api(&prepare_exe))
        else {
            continue;
        };
        if validate_launch_api(&client_api, "horizon-client").is_err()
            || validate_launch_api(&prepare_api, "horizon-prepare").is_err()
        {
            continue;
        }
        let Some(digest) = marker["digest"].as_str() else {
            continue;
        };
        let root = find_file(&build_dir, "research-lock.json", 4)
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .or_else(|| client_exe.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| build_dir.clone());
        return Some(Build {
            root,
            tag: cached_tag.to_string(),
            digest: digest.to_string(),
            client: client_exe,
            prepare: prepare_exe,
            client_api,
            prepare_api,
        });
    }
    None
}

pub(crate) async fn resolve_build(
    client: &reqwest::Client,
    store: &Store,
    tag: &str,
    app: Option<&AppHandle>,
) -> Result<Build> {
    let release = match release(client, tag).await {
        Ok(release) => release,
        Err(error) => return cached_build(store, tag).ok_or(error),
    };
    let archive_asset = release
        .assets
        .iter()
        .find(|asset| asset.name == WINDOWS_ASSET)
        .ok_or_else(|| AppError::new(format!("Horizon : {WINDOWS_ASSET} absent de la release")))?;
    let sums_asset = release
        .assets
        .iter()
        .find(|asset| asset.name == "SHA256SUMS")
        .ok_or_else(|| AppError::new("Horizon : SHA256SUMS absent de la release"))?;
    let sums = ensure_success(
        client.get(&sums_asset.browser_download_url).send().await?,
        "Téléchargement de SHA256SUMS Horizon",
    )
    .await?
    .text()
    .await?;
    let digest = parse_sha256sums(&sums, WINDOWS_ASSET)?;
    let short = &digest[..12];
    let build_dir = store
        .root()
        .join("horizon")
        .join("builds")
        .join(format!("{}-{short}", safe_tag(&release.tag_name)));
    let marker = build_dir.join("build.json");
    let cached = fs::read_to_string(&marker)
        .ok()
        .and_then(|content| serde_json::from_str::<Value>(&content).ok())
        .filter(|value| value["digest"].as_str() == Some(digest.as_str()));
    if let Some(cached) = cached {
        if let (Some(client_exe), Some(prepare_exe)) = (
            find_file(&build_dir, "horizon-client.exe", 4),
            find_file(&build_dir, "horizon-prepare.exe", 4),
        ) {
            if hashes_match(&cached, &client_exe, &prepare_exe) {
                if let (Ok(client_api), Ok(prepare_api)) =
                    (run_launch_api(&client_exe), run_launch_api(&prepare_exe))
                {
                    if validate_launch_api(&client_api, "horizon-client").is_ok()
                        && validate_launch_api(&prepare_api, "horizon-prepare").is_ok()
                    {
                        let root = find_file(&build_dir, "research-lock.json", 4)
                            .and_then(|path| path.parent().map(Path::to_path_buf))
                            .or_else(|| client_exe.parent().map(Path::to_path_buf))
                            .unwrap_or_else(|| build_dir.clone());
                        return Ok(Build {
                            root,
                            tag: release.tag_name,
                            digest,
                            client: client_exe,
                            prepare: prepare_exe,
                            client_api,
                            prepare_api,
                        });
                    }
                }
            }
        }
    }

    let archive = store
        .root()
        .join("cache")
        .join(format!("horizon-{short}.zip"));
    let checksum = Checksum::Sha256(digest.clone());
    download_file_with_checksum(
        client,
        &archive_asset.browser_download_url,
        &archive,
        Some(&checksum),
        app,
        "launch",
        "Horizon",
    )
    .await?;

    let parent = build_dir
        .parent()
        .ok_or_else(|| AppError::new("Dossier de build Horizon invalide"))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".install-{short}-{}", uuid::Uuid::new_v4()));
    if temporary.exists() {
        fs::remove_dir_all(&temporary)?;
    }
    fs::create_dir_all(&temporary)?;
    if let Err(error) = extract_zip(&archive, &temporary) {
        let _ = fs::remove_dir_all(&temporary);
        return Err(error);
    }
    if build_dir.exists() {
        fs::remove_dir_all(&build_dir)?;
    }
    fs::rename(&temporary, &build_dir).context("Installation du build Horizon")?;

    let client_exe = find_file(&build_dir, "horizon-client.exe", 4)
        .ok_or_else(|| AppError::new("Build Horizon sans horizon-client.exe"))?;
    let prepare_exe = find_file(&build_dir, "horizon-prepare.exe", 4)
        .ok_or_else(|| AppError::new("Build Horizon sans horizon-prepare.exe"))?;
    let root = find_file(&build_dir, "research-lock.json", 4)
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .or_else(|| client_exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| build_dir.clone());
    let client_sha = sha256_file(&client_exe)?;
    let prepare_sha = sha256_file(&prepare_exe)?;
    let client_api = run_launch_api(&client_exe)?;
    let prepare_api = run_launch_api(&prepare_exe)?;
    validate_launch_api(&client_api, "horizon-client")?;
    validate_launch_api(&prepare_api, "horizon-prepare")?;
    fs::write(
        build_dir.join("build.json"),
        serde_json::to_vec_pretty(&json!({
            "tag": release.tag_name,
            "digest": digest,
            "clientSha": client_sha,
            "prepareSha": prepare_sha,
            "resolvedAt": Utc::now(),
            "clientApi": client_api,
            "prepareApi": prepare_api,
        }))?,
    )?;
    Ok(Build {
        root,
        tag: release.tag_name,
        digest,
        client: client_exe,
        prepare: prepare_exe,
        client_api,
        prepare_api,
    })
}

fn sha1_hex(bytes: &[u8]) -> String {
    hex::encode(Sha1::digest(bytes))
}

fn manifest_accepted(lock: &Value, checksum: &str) -> bool {
    let minecraft = &lock["minecraft"];
    if minecraft["manifest_sha1"].as_str() == Some(checksum) {
        return true;
    }
    minecraft["accepted_manifest_sha1"]
        .as_array()
        .is_some_and(|accepted| {
            accepted
                .iter()
                .any(|entry| entry.as_str() == Some(checksum))
        })
}

async fn horizon_version_manifest(
    client: &reqwest::Client,
    build: &Build,
) -> Result<(Vec<u8>, Value)> {
    let lock: Value = serde_json::from_slice(&fs::read(build.root.join("research-lock.json"))?)?;
    let (bytes, value) = version_json_raw(client, HORIZON_MINECRAFT).await?;
    if manifest_accepted(&lock, &sha1_hex(&bytes)) {
        return Ok((bytes, value));
    }
    let pinned = lock["minecraft"]["manifest_sha1"]
        .as_str()
        .ok_or_else(|| AppError::new("research-lock.json sans manifest_sha1"))?;
    let url =
        format!("https://piston-meta.mojang.com/v1/packages/{pinned}/{HORIZON_MINECRAFT}.json");
    let response = ensure_success(
        client.get(&url).send().await?,
        "Téléchargement du manifeste 26.3 épinglé",
    )
    .await?;
    let bytes = response.bytes().await?;
    let actual = sha1_hex(&bytes);
    if actual != pinned {
        return Err(AppError::new(format!(
            "Le manifeste 26.3 épinglé a un SHA-1 inattendu : {actual} au lieu de {pinned}"
        )));
    }
    let value = serde_json::from_slice(&bytes)?;
    Ok((bytes.to_vec(), value))
}

async fn ensure_game_files(
    client: &reqwest::Client,
    store: &Store,
    app: &AppHandle,
    build: &Build,
) -> Result<GameFiles> {
    let (raw_profile, profile) = horizon_version_manifest(client, build).await?;
    let profiles = crate::launch::profile_chain(client, profile.clone()).await?;
    let base = profiles
        .last()
        .cloned()
        .ok_or_else(|| AppError::new("Profil Minecraft 26.3 vide"))?;
    crate::launch::download_assets(client, store, &base, app).await?;
    let natives = store
        .root()
        .join("versions")
        .join(HORIZON_MINECRAFT)
        .join("natives");
    crate::launch::download_libraries(client, store, &profiles, &natives, app).await?;
    for profile in &profiles {
        crate::launch::download_logging(client, store, profile).await?;
    }

    let version_dir = store.root().join("versions").join(HORIZON_MINECRAFT);
    fs::create_dir_all(&version_dir)?;
    let version_json = version_dir.join(format!("{HORIZON_MINECRAFT}.json"));
    fs::write(&version_json, &raw_profile)?;

    let horizon_manifest = build
        .root
        .join(".local")
        .join("minecraft")
        .join(HORIZON_MINECRAFT)
        .join("manifest.json");
    if let Some(parent) = horizon_manifest.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&horizon_manifest, &raw_profile)?;

    let client_info = &base["downloads"]["client"];
    let client_sha = client_info["sha1"]
        .as_str()
        .ok_or_else(|| AppError::new("Minecraft 26.3 sans checksum client"))?;
    let client_jar = version_dir.join(format!("{HORIZON_MINECRAFT}.jar"));
    download_file_with_checksum(
        client,
        client_info["url"].as_str().unwrap_or_default(),
        &client_jar,
        Some(&Checksum::Sha1(client_sha.to_string())),
        Some(app),
        "launch",
        "Client Minecraft 26.3",
    )
    .await?;

    let server_info = &base["downloads"]["server"];
    let server_sha = server_info["sha1"]
        .as_str()
        .ok_or_else(|| AppError::new("Minecraft 26.3 sans checksum serveur"))?;
    let server_jar = store
        .root()
        .join("cache")
        .join(format!("server-{HORIZON_MINECRAFT}.jar"));
    download_file_with_checksum(
        client,
        server_info["url"].as_str().unwrap_or_default(),
        &server_jar,
        Some(&Checksum::Sha1(server_sha.to_string())),
        Some(app),
        "launch",
        "Serveur Minecraft 26.3",
    )
    .await?;
    Ok(GameFiles {
        version_json,
        client_jar,
        server_jar,
        libraries_dir: store.root().join("libraries"),
        assets_dir: store.root().join("assets"),
    })
}

fn check_resources(build: &Build, resources: &Path) -> bool {
    let mut command = command(&build.prepare);
    command
        .arg("--check")
        .arg("--root")
        .arg(&build.root)
        .arg("--output")
        .arg(resources)
        .current_dir(&build.root)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn run_prepare(
    app: &AppHandle,
    build: &Build,
    java_home: &Path,
    game_files: &GameFiles,
    minecraft_dir: &Path,
    output: &Path,
) -> Result<()> {
    crate::launch::emit(app, "Préparation des ressources Horizon", 0, 1);
    let mut command = command(&build.prepare);
    command
        .arg("--prepare")
        .arg("--server-jar")
        .arg(&game_files.server_jar)
        .arg("--minecraft-dir")
        .arg(minecraft_dir)
        .arg("--version-json")
        .arg(&game_files.version_json)
        .arg("--client-jar")
        .arg(&game_files.client_jar)
        .arg("--libraries-dir")
        .arg(&game_files.libraries_dir)
        .arg("--assets-dir")
        .arg(&game_files.assets_dir)
        .arg("--output")
        .arg(output)
        .arg("--complete")
        .arg("--progress")
        .arg("json")
        .arg("--root")
        .arg(&build.root)
        .arg("--java-home")
        .arg(java_home)
        .current_dir(&build.root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.env("JAVA_HOME", java_home);
    if let Some(path) = std::env::var_os("PATH") {
        if let Ok(joined) = std::env::join_paths(
            std::iter::once(java_home.join("bin")).chain(std::env::split_paths(&path)),
        ) {
            command.env("PATH", joined);
        }
    }

    let mut child = command
        .spawn()
        .context("Démarrage de la préparation Horizon")?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::new("Sortie de horizon-prepare indisponible"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::new("Erreur de horizon-prepare indisponible"))?;
    let stderr_thread = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let mut stderr = stderr;
        let _ = stderr.read_to_end(&mut buffer);
        buffer
    });
    let mut reported_error = None;
    for line in BufReader::new(stdout)
        .lines()
        .map_while(std::result::Result::ok)
    {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(fraction) = value["fraction"].as_f64() {
            let current = (fraction.clamp(0.0, 1.0) * 1000.0) as u64;
            crate::launch::emit(app, "Préparation des ressources Horizon", current, 1000);
        } else if let Some(message) = value["message"].as_str() {
            crate::launch::emit(app, message, 0, 0);
        } else if let Some(error) = value["error"].as_str() {
            reported_error = Some(error.to_string());
        }
    }
    let status = child.wait()?;
    let stderr = stderr_thread
        .join()
        .map(|buffer| String::from_utf8_lossy(&buffer).into_owned())
        .unwrap_or_default();
    if !status.success() {
        let detail = reported_error.unwrap_or_else(|| {
            stderr
                .lines()
                .rev()
                .take(8)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n")
        });
        return Err(AppError::new(format!(
            "Échec de la préparation Horizon : {detail}"
        )));
    }
    Ok(())
}

pub(crate) async fn prepare(
    state: &crate::AppState,
    app: &AppHandle,
    instance: &Instance,
) -> Result<(Build, PathBuf)> {
    let tag = instance.loader_version.as_deref().unwrap_or("nightly");
    crate::launch::emit(app, "Préparation de Horizon", 0, 1);
    let build = resolve_build(&state.client, &state.store, tag, Some(app)).await?;
    crate::launch::emit(app, &format!("Horizon {}", build.tag), 0, 1);
    let resources = state
        .store
        .root()
        .join("horizon")
        .join("resources")
        .join(&build.digest);
    if resources.is_dir() && check_resources(&build, &resources) {
        return Ok((build, resources));
    }
    let game_files = ensure_game_files(&state.client, &state.store, app, &build).await?;
    let java_major = build.prepare_api["java_major"].as_u64().unwrap_or(25);
    let java_home =
        crate::java::ensure_jdk(&state.client, &state.store, java_major, Some(app)).await?;
    let app = app.clone();
    let build_for_prepare = build.clone();
    let java_home_for_prepare = java_home.clone();
    let minecraft_dir = state.store.root().to_path_buf();
    let output = resources.clone();
    tokio::task::spawn_blocking(move || {
        run_prepare(
            &app,
            &build_for_prepare,
            &java_home_for_prepare,
            &game_files,
            &minecraft_dir,
            &output,
        )
    })
    .await
    .map_err(|error| AppError::new(error.to_string()))??;
    if !check_resources(&build, &resources) {
        return Err(AppError::new(
            "La préparation Horizon est incomplète : relancez le jeu",
        ));
    }
    Ok((build, resources))
}

pub(crate) async fn launch(
    state: &crate::AppState,
    app: &AppHandle,
    instance: &Instance,
    account: &Account,
    token: &str,
    server_override: Option<&str>,
) -> Result<LaunchResult> {
    let (build, resources) = prepare(state, app, instance).await?;
    if !has_option(&build.client_api, "--session-stdin") {
        return Err(AppError::new(
            "Ce build Horizon ne permet pas le lancement par session stdin",
        ));
    }
    let game_dir = state.store.game_dir(&instance.id);
    fs::create_dir_all(&game_dir)?;
    let home = state.store.instance_dir(&instance.id).join("horizon");
    fs::create_dir_all(&home)?;

    let mut command = command(&build.client);
    command
        .arg("--resources")
        .arg(&resources)
        .arg("--game-dir")
        .arg(&game_dir)
        .arg("--home")
        .arg(&home)
        .arg("--session-stdin");
    if instance.fullscreen {
        if has_option(&build.client_api, "--fullscreen") {
            command.arg("--fullscreen");
        }
    } else {
        if has_option(&build.client_api, "--width") {
            command
                .arg("--width")
                .arg(instance.resolution_width.to_string());
        }
        if has_option(&build.client_api, "--height") {
            command
                .arg("--height")
                .arg(instance.resolution_height.to_string());
        }
    }
    if let Some(server) = server_override.or(instance.server_address.as_deref()) {
        if !has_option(&build.client_api, "--quickPlayMultiplayer") {
            return Err(AppError::new(
                "Ce build Horizon ne permet pas le lancement direct sur un serveur",
            ));
        }
        let (host, port) = crate::launch::normalize_server(server)?;
        command
            .arg("--quickPlayMultiplayer")
            .arg(format!("{host}:{port}"));
    }
    command
        .current_dir(&game_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().context("Impossible de démarrer Horizon")?;
    let pid = child.id();
    let session = json!({
        "name": account.name,
        "uuid": account.id,
        "access_token": token,
        "user_type": "msa",
    });
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(serde_json::to_string(&session)?.as_bytes())?;
    }
    let started_at = Utc::now();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::new("Sortie Horizon indisponible"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::new("Erreur Horizon indisponible"))?;
    let child = Arc::new(Mutex::new(child));
    state
        .processes
        .lock()
        .map_err(|_| AppError::new("Verrou du processus invalide"))?
        .insert(instance.id.clone(), child.clone());
    let log_file = Arc::new(Mutex::new(fs::File::create(
        state
            .store
            .root()
            .join("logs")
            .join(format!("{}.log", instance.id)),
    )?));
    crate::launch::forward_logs(
        stdout,
        app.clone(),
        instance.id.clone(),
        log_file.clone(),
        "stdout",
    );
    crate::launch::forward_logs(
        stderr,
        app.clone(),
        instance.id.clone(),
        log_file.clone(),
        "stderr",
    );
    crate::launch::monitor_process(
        child,
        app.clone(),
        instance.id.clone(),
        state.store.clone(),
        state.processes.clone(),
        None,
    );
    let mut instances = state.store.instances()?;
    if let Some(stored) = instances.iter_mut().find(|item| item.id == instance.id) {
        stored.last_played_at = Some(started_at);
    }
    state.store.save_instances(&instances)?;
    if instance.close_on_launch {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
        }
    }
    crate::launch::emit(app, "Démarrage de Horizon", 1, 1);
    Ok(LaunchResult { pid, started_at })
}

#[cfg(test)]
mod tests {
    use super::{manifest_accepted, parse_sha256sums, safe_tag, sha1_hex};

    #[test]
    fn reads_the_windows_checksum() {
        let sums = "\
83d36c5a2f8b6690d4d77411dfc3f85753319184fffe738ee5f0a4c552a0eabe  horizon-linux-x86_64.tar.gz
f47f9f988b7c30b727dfb35aed2b74f01428925b2eccc9bafff2352f4847f7d6  horizon-windows-x64.zip
";
        assert_eq!(
            parse_sha256sums(sums, "horizon-windows-x64.zip").unwrap(),
            "f47f9f988b7c30b727dfb35aed2b74f01428925b2eccc9bafff2352f4847f7d6"
        );
        assert!(parse_sha256sums(sums, "missing.zip").is_err());
    }

    #[test]
    fn keeps_release_tags_safe_as_folder_names() {
        assert_eq!(safe_tag("nightly"), "nightly");
        assert_eq!(safe_tag("nightly-2026-11-13"), "nightly-2026-11-13");
        assert_eq!(safe_tag("a/b:c"), "a_b_c");
    }

    #[test]
    fn accepts_only_horizon_manifest_hashes() {
        let lock = serde_json::json!({
            "minecraft": {
                "manifest_sha1": "11f1aefe473eefba0af62e3286c33b692807b2eb",
                "accepted_manifest_sha1": [
                    "9f67e10ed013deeaad20ac3c800652765d493408"
                ]
            }
        });
        assert!(manifest_accepted(
            &lock,
            "11f1aefe473eefba0af62e3286c33b692807b2eb"
        ));
        assert!(manifest_accepted(
            &lock,
            "9f67e10ed013deeaad20ac3c800652765d493408"
        ));
        assert!(!manifest_accepted(
            &lock,
            "3267fc08b80e13529ead2f913fc7807d19aec710"
        ));
    }

    #[test]
    fn pretty_printing_changes_the_manifest_sha1() {
        let raw = br#"{"id":"26.3"}"#;
        let value: serde_json::Value = serde_json::from_slice(raw).unwrap();
        let pretty = serde_json::to_vec_pretty(&value).unwrap();
        assert_eq!(sha1_hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_ne!(sha1_hex(raw), sha1_hex(&pretty));
    }
}
