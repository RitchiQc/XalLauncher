use crate::auth::valid_token;
use crate::download::{download_file, download_file_with_checksum, maven_checksum, Checksum};
use crate::error::{AppError, Result, ResultExt};
use crate::loaders::maven_path;
use crate::models::{GameLog, LaunchResult, ProgressEvent};
use crate::store::Store;
use crate::versions::version_json;
use chrono::Utc;
use futures_util::{stream, StreamExt, TryStreamExt};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

fn emit(app: &AppHandle, label: &str, current: u64, total: u64) {
    let _ = app.emit(
        "download-progress",
        ProgressEvent {
            operation: "launch",
            label,
            current,
            total,
        },
    );
}

fn rules_allow(rules: Option<&Vec<Value>>) -> bool {
    let Some(rules) = rules else {
        return true;
    };
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        let os_matches = rule
            .get("os")
            .map(|os| {
                if os["name"] == "windows" && !cfg!(windows) {
                    return false;
                }
                if os["name"] == "osx" && !cfg!(target_os = "macos") {
                    return false;
                }
                if os["name"] == "linux" && !cfg!(target_os = "linux") {
                    return false;
                }
                if os["arch"] == "x86" && !cfg!(target_arch = "x86") {
                    return false;
                }
                true
            })
            .unwrap_or(true);
        let features_match = rule
            .get("features")
            .map(|features| {
                features
                    .as_object()
                    .map(|values| values.values().all(|value| value == &Value::Bool(false)))
                    .unwrap_or(true)
            })
            .unwrap_or(true);
        if os_matches && features_match {
            allowed = rule["action"] == "allow";
        }
    }
    allowed
}

fn validate_artifact_path(path: &str) -> Result<()> {
    let candidate = Path::new(path);
    if candidate.as_os_str().is_empty()
        || candidate.is_absolute()
        || candidate
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(AppError::new("Chemin de bibliothèque distant dangereux"));
    }
    Ok(())
}

fn value_arguments(values: Option<&Vec<Value>>) -> Vec<String> {
    values
        .map(|arguments| {
            arguments
                .iter()
                .filter_map(|argument| {
                    if let Some(value) = argument.as_str() {
                        return Some(value.to_string());
                    }
                    if rules_allow(argument.get("rules").and_then(Value::as_array)) {
                        match argument.get("value") {
                            Some(Value::String(value)) => Some(value.clone()),
                            Some(Value::Array(values)) => Some(
                                values
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .collect::<Vec<_>>()
                                    .join(" "),
                            ),
                            _ => None,
                        }
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn profile_chain(client: &reqwest::Client, profile: Value) -> Result<Vec<Value>> {
    let mut chain = vec![profile.clone()];
    if let Some(parent) = profile["inheritsFrom"].as_str() {
        let mut parents =
            Box::pin(profile_chain(client, version_json(client, parent).await?)).await?;
        chain.append(&mut parents);
    }
    Ok(chain)
}

#[derive(Clone)]
struct AssetJob {
    url: String,
    path: PathBuf,
    sha1: String,
}

async fn download_assets(
    client: &reqwest::Client,
    store: &Store,
    base: &Value,
    app: &AppHandle,
) -> Result<()> {
    let Some(index) = base.get("assetIndex") else {
        return Ok(());
    };
    let id = index["id"].as_str().unwrap_or("legacy");
    let index_path = store
        .root()
        .join("assets")
        .join("indexes")
        .join(format!("{id}.json"));
    download_file(
        client,
        index["url"].as_str().unwrap_or(""),
        &index_path,
        index["sha1"].as_str(),
        None,
        "launch",
        "Index des assets",
    )
    .await?;
    let content = fs::read_to_string(&index_path)?;
    let objects: Value = serde_json::from_str(&content)?;
    let mut jobs = Vec::new();
    for (_, object) in objects["objects"].as_object().into_iter().flatten() {
        let Some(hash) = object["hash"].as_str() else {
            continue;
        };
        if hash.len() < 2 {
            continue;
        }
        jobs.push(AssetJob {
            url: format!(
                "https://resources.download.minecraft.net/{}/{hash}",
                &hash[..2]
            ),
            path: store
                .root()
                .join("assets")
                .join("objects")
                .join(&hash[..2])
                .join(hash),
            sha1: hash.to_string(),
        });
    }
    let total = jobs.len();
    let concurrency = store.settings()?.download_concurrency.clamp(1, 32);
    let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    emit(app, "Assets Minecraft", 0, total as u64);
    stream::iter(jobs.into_iter().map(|job| {
        let client = client.clone();
        let app = app.clone();
        let completed = completed.clone();
        async move {
            download_file(
                &client,
                &job.url,
                &job.path,
                Some(&job.sha1),
                None,
                "launch",
                "Asset",
            )
            .await?;
            let current = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if current.is_multiple_of(25) || current == total {
                emit(&app, "Assets Minecraft", current as u64, total as u64);
            }
            Ok::<(), AppError>(())
        }
    }))
    .buffer_unordered(concurrency)
    .try_collect::<Vec<()>>()
    .await?;
    Ok(())
}

async fn download_libraries(
    client: &reqwest::Client,
    store: &Store,
    profiles: &[Value],
    natives_dir: &Path,
    app: &AppHandle,
) -> Result<Vec<PathBuf>> {
    let libraries: Vec<&Value> = profiles
        .iter()
        .filter_map(|profile| profile["libraries"].as_array())
        .flatten()
        .collect();
    let total = libraries.len();
    let mut classpath = Vec::new();
    let mut seen = HashSet::new();
    let mut natives = Vec::new();
    fs::create_dir_all(natives_dir)?;

    for (index, library) in libraries.into_iter().enumerate() {
        if !rules_allow(library.get("rules").and_then(Value::as_array)) {
            continue;
        }
        let name = library["name"].as_str().unwrap_or("");
        let coordinate_path = maven_path(name)?;
        let base_url = library["url"]
            .as_str()
            .unwrap_or("https://libraries.minecraft.net/");
        let artifact = library["downloads"]["artifact"].clone();
        let artifact_path = artifact["path"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| coordinate_path.clone());
        validate_artifact_path(&artifact_path)?;
        if artifact.is_object() || library.get("url").is_some() {
            let path = store.root().join("libraries").join(&artifact_path);
            let url = artifact["url"]
                .as_str()
                .filter(|url| !url.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| format!("{}/{artifact_path}", base_url.trim_end_matches('/')));
            let checksum = match artifact["sha1"].as_str() {
                Some(hash) => Checksum::Sha1(hash.to_string()),
                None => maven_checksum(client, &url).await?,
            };
            download_file_with_checksum(client, &url, &path, Some(&checksum), None, "launch", name)
                .await?;
            if seen.insert(path.clone()) {
                classpath.push(path);
            }
        }

        if let Some(classifier) = library["natives"]
            .get(if cfg!(windows) {
                "windows"
            } else if cfg!(target_os = "macos") {
                "osx"
            } else {
                "linux"
            })
            .and_then(Value::as_str)
        {
            let classifier = classifier.replace(
                "${arch}",
                if cfg!(target_arch = "x86_64") {
                    "64"
                } else {
                    "32"
                },
            );
            if let Some(native) = library["downloads"]["classifiers"].get(&classifier) {
                let path = if let Some(relative) = native["path"].as_str() {
                    validate_artifact_path(relative)?;
                    store.root().join("libraries").join(relative)
                } else {
                    store.root().join("libraries").join(format!(
                        "{}-{}.jar",
                        coordinate_path.trim_end_matches(".jar"),
                        classifier
                    ))
                };
                let url = native["url"]
                    .as_str()
                    .filter(|url| !url.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        format!(
                            "{}/{}",
                            base_url.trim_end_matches('/'),
                            path.file_name().unwrap_or_default().to_string_lossy()
                        )
                    });
                let checksum = match native["sha1"].as_str() {
                    Some(hash) => Checksum::Sha1(hash.to_string()),
                    None => maven_checksum(client, &url).await?,
                };
                download_file_with_checksum(
                    client,
                    &url,
                    &path,
                    Some(&checksum),
                    None,
                    "launch",
                    name,
                )
                .await?;
                natives.push(path);
            }
        }
        if index % 5 == 0 || index + 1 == total {
            emit(
                app,
                "Bibliothèques Minecraft",
                (index + 1) as u64,
                total as u64,
            );
        }
    }
    for native in natives {
        extract_native(&native, natives_dir)?;
    }
    Ok(classpath)
}

fn extract_native(archive: &Path, destination: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(fs::File::open(archive)?)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        if relative
            .components()
            .next()
            .map(|part| part.as_os_str() == "META-INF")
            .unwrap_or(false)
            || entry.is_dir()
        {
            continue;
        }
        let target = destination.join(relative.file_name().unwrap_or_default());
        let mut output = fs::File::create(target)?;
        std::io::copy(&mut entry, &mut output)?;
    }
    Ok(())
}

async fn download_logging(client: &reqwest::Client, store: &Store, profile: &Value) -> Result<()> {
    let file = &profile["logging"]["client"]["file"];
    if let (Some(url), Some(id)) = (file["url"].as_str(), file["id"].as_str()) {
        let path = store.root().join("assets").join("log_configs").join(id);
        download_file(
            client,
            url,
            &path,
            file["sha1"].as_str(),
            None,
            "launch",
            "Configuration des logs",
        )
        .await?;
    }
    Ok(())
}

fn substitute(template: &str, values: &[(String, String)]) -> String {
    values
        .iter()
        .fold(template.to_string(), |result, (key, value)| {
            result.replace(&format!("${{{key}}}"), value)
        })
}

fn resolve_main_class(profiles: &[Value]) -> Result<String> {
    profiles
        .iter()
        .find_map(|profile| profile["mainClass"].as_str())
        .map(str::to_string)
        .ok_or_else(|| AppError::new("Classe principale Minecraft introuvable"))
}

fn collect_arguments(profiles: &[Value], section: &str) -> Vec<String> {
    let modern: Vec<String> = profiles
        .iter()
        .flat_map(|profile| value_arguments(profile["arguments"][section].as_array()))
        .collect();
    if !modern.is_empty() {
        return modern;
    }
    profiles
        .iter()
        .find_map(|profile| profile["minecraftArguments"].as_str())
        .map(|arguments| arguments.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default()
}

#[tauri::command]
pub async fn launch_instance(
    id: String,
    account_id: String,
    server_override: Option<String>,
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<LaunchResult> {
    if state
        .processes
        .lock()
        .map_err(|_| AppError::new("Verrou du processus invalide"))?
        .contains_key(&id)
    {
        return Err(AppError::new(
            "Cette instance est déjà en cours d’exécution",
        ));
    }
    let instance = state.store.instance(&id)?;
    emit(&app, "Vérification de la session", 0, 1);
    let (account, token) = valid_token(&state, &account_id).await?;
    let base_profile = version_json(&state.client, &instance.version_id).await?;
    let java_major = base_profile["javaVersion"]["majorVersion"]
        .as_u64()
        .unwrap_or(8);
    let java = match &instance.java_path {
        Some(path) if path.is_file() => path.clone(),
        Some(_) => {
            return Err(AppError::new(
                "Le Java personnalisé de cette instance est introuvable",
            ))
        }
        None => {
            crate::java::ensure_java(&state.client, &state.store, java_major, Some(&app)).await?
        }
    };
    emit(&app, "Préparation du modloader", 0, 1);
    let (profile_id, profile) =
        crate::loaders::prepare(&state.client, &state.store, &instance, &java, Some(&app)).await?;
    let profiles = profile_chain(&state.client, profile).await?;
    let base = profiles
        .last()
        .ok_or_else(|| AppError::new("Profil Minecraft vide"))?
        .clone();
    let game_dir = state.store.game_dir(&id);
    fs::create_dir_all(&game_dir)?;
    let natives_dir = state
        .store
        .root()
        .join("versions")
        .join(&instance.version_id)
        .join("natives");
    download_assets(&state.client, &state.store, &base, &app).await?;
    let mut libraries =
        download_libraries(&state.client, &state.store, &profiles, &natives_dir, &app).await?;
    for profile in &profiles {
        download_logging(&state.client, &state.store, profile).await?;
    }

    let base_id = base["id"].as_str().unwrap_or(&instance.version_id);
    let client_download = &base["downloads"]["client"];
    let game_jar = state
        .store
        .root()
        .join("versions")
        .join(base_id)
        .join(format!("{base_id}.jar"));
    let game_checksum = Checksum::Sha1(
        client_download["sha1"]
            .as_str()
            .ok_or_else(|| AppError::new("Jeu Minecraft sans checksum SHA-1"))?
            .to_string(),
    );
    download_file_with_checksum(
        &state.client,
        client_download["url"].as_str().unwrap_or(""),
        &game_jar,
        Some(&game_checksum),
        None,
        "launch",
        "Jeu Minecraft",
    )
    .await?;
    libraries.push(game_jar);
    let main_class = resolve_main_class(&profiles)?;
    let assets_index = base["assetIndex"]["id"].as_str().unwrap_or("legacy");
    let classpath = libraries
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(";");
    let values = vec![
        ("auth_player_name".into(), account.name.clone()),
        ("auth_session".into(), token.clone()),
        ("auth_uuid".into(), account.id.clone()),
        ("auth_access_token".into(), token),
        ("user_type".into(), "msa".into()),
        ("user_properties".into(), "{}".into()),
        ("version_name".into(), profile_id.clone()),
        ("game_directory".into(), game_dir.to_string_lossy().into()),
        (
            "assets_root".into(),
            state.store.root().join("assets").to_string_lossy().into(),
        ),
        (
            "game_assets".into(),
            state.store.root().join("assets").to_string_lossy().into(),
        ),
        ("assets_index_name".into(), assets_index.into()),
        (
            "version_type".into(),
            base["type"].as_str().unwrap_or("release").into(),
        ),
        ("classpath".into(), classpath),
        (
            "natives_directory".into(),
            natives_dir.to_string_lossy().into(),
        ),
        (
            "library_directory".into(),
            state
                .store
                .root()
                .join("libraries")
                .to_string_lossy()
                .into(),
        ),
        ("launcher_name".into(), "XalLauncher".into()),
        ("launcher_version".into(), env!("CARGO_PKG_VERSION").into()),
        ("classpath_separator".into(), ";".into()),
        ("auth_xuid".into(), String::new()),
        ("clientid".into(), String::new()),
    ];
    let mut jvm = vec![
        format!("-Xms{}M", instance.memory_min),
        format!("-Xmx{}M", instance.memory_max),
    ];
    jvm.extend(
        collect_arguments(&profiles, "jvm")
            .iter()
            .map(|argument| substitute(argument, &values)),
    );
    if !instance.jvm_arguments.trim().is_empty() {
        let custom = shell_words::split(&instance.jvm_arguments)
            .map_err(|error| AppError::new(format!("Arguments JVM invalides : {error}")))?;
        jvm.extend(custom);
    }
    if !jvm
        .iter()
        .any(|argument| argument == "-cp" || argument == "-classpath")
    {
        jvm.push("-cp".into());
        jvm.push(
            values
                .iter()
                .find(|(key, _)| key == "classpath")
                .map(|(_, value)| value.clone())
                .unwrap_or_default(),
        );
    }
    let mut game = collect_arguments(&profiles, "game")
        .iter()
        .map(|argument| substitute(argument, &values))
        .collect::<Vec<_>>();
    if !instance.fullscreen {
        game.extend([
            "--width".into(),
            instance.resolution_width.to_string(),
            "--height".into(),
            instance.resolution_height.to_string(),
        ]);
    } else {
        game.push("--fullscreen".into());
    }
    if let Some(server) = server_override
        .as_ref()
        .or(instance.server_address.as_ref())
    {
        let (host, port) = server
            .rsplit_once(':')
            .and_then(|(host, port)| port.parse::<u16>().ok().map(|port| (host, port)))
            .unwrap_or((server.as_str(), 25565));
        game.extend([
            "--server".into(),
            host.into(),
            "--port".into(),
            port.to_string(),
        ]);
    }

    let argument_file = state.store.instance_dir(&id).join("jvm-args.txt");
    let estimated_length: usize = jvm.iter().map(|argument| argument.len() + 3).sum();
    let use_argument_file = java_major >= 9 && estimated_length > 12_000;
    if use_argument_file {
        let content = jvm
            .iter()
            .map(|argument| {
                format!(
                    "\"{}\"",
                    argument.replace('\\', "\\\\").replace('"', "\\\"")
                )
            })
            .collect::<Vec<_>>()
            .join("\r\n");
        fs::write(&argument_file, content)?;
    }

    let mut command = Command::new(&java);
    if use_argument_file {
        command.arg(format!("@{}", argument_file.to_string_lossy()));
    } else {
        command.args(&jvm);
    }
    command
        .arg(main_class)
        .args(game)
        .current_dir(&game_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    emit(&app, "Démarrage de Minecraft", 1, 1);
    let mut child = command
        .spawn()
        .context("Impossible de démarrer le processus Minecraft")?;
    let pid = child.id();
    let started_at = Utc::now();
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::new("Sortie Minecraft indisponible"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::new("Erreur Minecraft indisponible"))?;
    let child = Arc::new(Mutex::new(child));
    state
        .processes
        .lock()
        .map_err(|_| AppError::new("Verrou du processus invalide"))?
        .insert(id.clone(), child.clone());
    let log_file = Arc::new(Mutex::new(fs::File::create(
        state.store.root().join("logs").join(format!("{id}.log")),
    )?));
    forward_logs(stdout, app.clone(), id.clone(), log_file.clone(), "stdout");
    forward_logs(stderr, app.clone(), id.clone(), log_file.clone(), "stderr");
    monitor_process(
        child,
        app.clone(),
        id.clone(),
        state.store.clone(),
        state.processes.clone(),
        argument_file,
    );
    let mut instances = state.store.instances()?;
    if let Some(stored) = instances.iter_mut().find(|item| item.id == id) {
        stored.last_played_at = Some(started_at);
    }
    state.store.save_instances(&instances)?;
    if instance.close_on_launch {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
        }
    }
    Ok(LaunchResult { pid, started_at })
}

fn forward_logs(
    reader: impl Read + Send + 'static,
    app: AppHandle,
    id: String,
    file: Arc<Mutex<fs::File>>,
    stream: &'static str,
) {
    std::thread::spawn(move || {
        for line in BufReader::new(reader)
            .lines()
            .map_while(std::result::Result::ok)
        {
            if let Ok(mut file) = file.lock() {
                let _ = writeln!(file, "[{stream}] {line}");
            }
            let _ = app.emit(
                "game-log",
                GameLog {
                    instance_id: &id,
                    line: &line,
                    stream,
                },
            );
        }
    });
}

fn monitor_process(
    child: Arc<Mutex<Child>>,
    app: AppHandle,
    id: String,
    store: Store,
    processes: Arc<Mutex<std::collections::HashMap<String, Arc<Mutex<Child>>>>>,
    argument_file: PathBuf,
) {
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let status = child
            .lock()
            .map_err(|_| AppError::new("Verrou du processus invalide"))
            .and_then(|mut child| Ok(child.wait()?));
        if let Ok(mut instances) = store.instances() {
            if let Some(instance) = instances.iter_mut().find(|instance| instance.id == id) {
                instance.total_play_seconds += started.elapsed().as_secs();
            }
            let _ = store.save_instances(&instances);
        }
        if let Ok(mut processes) = processes.lock() {
            processes.remove(&id);
        }
        let _ = fs::remove_file(argument_file);
        let _ = app.emit(
            "game-exit",
            serde_json::json!({
                "instanceId": id,
                "code": status.as_ref().ok().and_then(|status| status.code()),
                "error": status.err().map(|error| error.to_string()),
            }),
        );
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
        }
    });
}

#[tauri::command]
pub fn stop_game(id: String, state: tauri::State<'_, crate::AppState>) -> Result<()> {
    let processes = state
        .processes
        .lock()
        .map_err(|_| AppError::new("Verrou du processus invalide"))?;
    let child = processes
        .get(&id)
        .ok_or_else(|| AppError::new("Cette instance n’est pas en cours d’exécution"))?;
    child
        .lock()
        .map_err(|_| AppError::new("Verrou du processus invalide"))?
        .kill()?;
    Ok(())
}

#[tauri::command]
pub fn running_games(state: tauri::State<'_, crate::AppState>) -> Result<Vec<String>> {
    Ok(state
        .processes
        .lock()
        .map_err(|_| AppError::new("Verrou du processus invalide"))?
        .keys()
        .cloned()
        .collect())
}

#[tauri::command]
pub fn read_instance_log(id: String, state: tauri::State<'_, crate::AppState>) -> Result<String> {
    let path = state.store.root().join("logs").join(format!("{id}.log"));
    if !path.exists() {
        return Ok(String::new());
    }
    let content = fs::read_to_string(path)?;
    Ok(content
        .chars()
        .rev()
        .take(250_000)
        .collect::<String>()
        .chars()
        .rev()
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{rules_allow, substitute, validate_artifact_path};
    use serde_json::json;

    #[test]
    fn evaluates_platform_rules() {
        assert!(rules_allow(None));
        let windows = vec![json!({"action":"allow","os":{"name":"windows"}})];
        assert_eq!(rules_allow(Some(&windows)), cfg!(windows));
        let forbidden = vec![json!({"action":"disallow","os":{"name":"windows"}})];
        assert_eq!(rules_allow(Some(&forbidden)), !cfg!(windows));
    }

    #[test]
    fn rejects_dangerous_artifact_paths() {
        assert!(validate_artifact_path("org/example/lib/1.0/lib.jar").is_ok());
        assert!(validate_artifact_path("../outside.jar").is_err());
        assert!(validate_artifact_path("org/../../outside.jar").is_err());
        assert!(validate_artifact_path("C:/outside.jar").is_err());
        assert!(validate_artifact_path("/outside.jar").is_err());
        assert!(validate_artifact_path("").is_err());
    }

    #[test]
    fn substitutes_launcher_placeholders() {
        let values = vec![("player".to_string(), "Xaltar".to_string())];
        assert_eq!(
            substitute("--username ${player}", &values),
            "--username Xaltar"
        );
    }
}
