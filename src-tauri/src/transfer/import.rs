use crate::error::{AppError, Result};
use crate::models::{Instance, Loader, ModuleState};
use crate::store::Store;
use crate::transfer::{
    clamp_memory, count_files, curseforge_error, register_instance, sanitize_name, unknown_error,
    valid_color, Format, ImportReport, MAX_ENTRIES, MAX_TOTAL_BYTES,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use zip::ZipArchive;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct XalSettings {
    name: Option<String>,
    color: Option<String>,
    memory_min: Option<u32>,
    memory_max: Option<u32>,
    resolution_width: Option<u32>,
    resolution_height: Option<u32>,
    fullscreen: Option<bool>,
    close_on_launch: Option<bool>,
    jvm_arguments: Option<String>,
    server_address: Option<String>,
    modules: Option<ModuleState>,
}

#[derive(Debug, Clone, Deserialize)]
struct PrismComponent {
    uid: String,
    version: String,
}

#[derive(Debug, Default, Deserialize)]
struct PrismPack {
    #[serde(default)]
    components: Vec<PrismComponent>,
}

#[derive(Debug, Default, Clone, PartialEq)]
struct PrismConfig {
    name: Option<String>,
    min_memory: Option<u32>,
    max_memory: Option<u32>,
    server_address: Option<String>,
}

fn archive_names(archive: &mut ZipArchive<fs::File>) -> Result<Vec<String>> {
    let mut names = Vec::with_capacity(archive.len());
    for index in 0..archive.len() {
        names.push(archive.by_index(index)?.name()?.to_string());
    }
    Ok(names)
}

fn find_marker<'a>(names: &'a [String], marker: &str) -> Option<(&'a str, String)> {
    names
        .iter()
        .filter(|name| name.as_str() == marker || name.ends_with(&format!("/{marker}")))
        .min_by_key(|name| name.matches('/').count())
        .map(|name| {
            let prefix = name.strip_suffix(marker).unwrap_or_default().to_string();
            (name.as_str(), prefix)
        })
}

fn read_entry(archive: &mut ZipArchive<fs::File>, name: &str) -> Result<String> {
    let mut file = archive.by_name(name)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    Ok(content)
}

fn detect(archive: &mut ZipArchive<fs::File>) -> Result<(Format, String)> {
    let names = archive_names(archive)?;
    if let Some((_, prefix)) = find_marker(&names, "modrinth.index.json") {
        let format = if find_marker(&names, "xallauncher.json").is_some() {
            Format::XalLauncher
        } else {
            Format::Modrinth
        };
        return Ok((format, prefix));
    }
    if let Some((manifest, prefix)) = find_marker(&names, "manifest.json") {
        if let Ok(content) = read_entry(archive, manifest) {
            if serde_json::from_str::<Value>(&content)
                .map(|value| value["manifestType"] == "minecraftModpack")
                .unwrap_or(false)
            {
                return Ok((Format::CurseForge, prefix));
            }
        }
    }
    if let Some((_, prefix)) = find_marker(&names, "mmc-pack.json") {
        return Ok((Format::Prism, prefix));
    }
    if let Some((_, prefix)) = find_marker(&names, "xallauncher.json") {
        return Ok((Format::XalLauncher, prefix));
    }
    Ok((Format::Unknown, String::new()))
}

fn read_xal_settings(archive: &mut ZipArchive<fs::File>, prefix: &str) -> Option<XalSettings> {
    let mut candidates = vec!["xallauncher.json".to_string()];
    if !prefix.is_empty() {
        candidates.push(format!("{prefix}xallauncher.json"));
    }
    candidates.into_iter().find_map(|name| {
        read_entry(archive, &name)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
    })
}

fn apply_settings(store: &Store, instance: &mut Instance, settings: &XalSettings) -> Result<()> {
    if let Some(color) = settings.color.as_deref().filter(|value| valid_color(value)) {
        instance.color = color.to_string();
    }
    if settings.memory_min.is_some() || settings.memory_max.is_some() {
        let (min, max) = clamp_memory(
            settings.memory_min.unwrap_or(instance.memory_min),
            settings.memory_max.unwrap_or(instance.memory_max),
        );
        instance.memory_min = min;
        instance.memory_max = max;
    }
    if let (Some(width), Some(height)) = (settings.resolution_width, settings.resolution_height) {
        instance.resolution_width = width.max(640);
        instance.resolution_height = height.max(360);
    }
    if let Some(value) = settings.fullscreen {
        instance.fullscreen = value;
    }
    if let Some(value) = settings.close_on_launch {
        instance.close_on_launch = value;
    }
    if let Some(value) = settings.jvm_arguments.as_deref() {
        if value.len() <= 4096 && !value.chars().any(char::is_control) {
            instance.jvm_arguments = value.trim().to_string();
        }
    }
    if let Some(value) = settings.server_address.as_deref() {
        let value = value.trim();
        instance.server_address =
            if value.is_empty() || value.chars().any(char::is_control) || value.len() > 255 {
                None
            } else {
                Some(value.to_string())
            };
    }
    if let Some(modules) = &settings.modules {
        instance.modules = modules.clone();
    }
    let mut instances = store.instances()?;
    if let Some(existing) = instances.iter_mut().find(|item| item.id == instance.id) {
        *existing = instance.clone();
    } else {
        instances.push(instance.clone());
    }
    store.save_instances(&instances)?;
    Ok(())
}

#[tauri::command]
pub async fn import_instance(
    path: PathBuf,
    name: Option<String>,
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<ImportReport> {
    if !path.is_file() {
        return Err(AppError::new("L’archive à importer est introuvable"));
    }
    let (format, prefix, settings) = {
        let file = fs::File::open(&path)?;
        let mut archive =
            ZipArchive::new(file).map_err(|_| AppError::new("Fichier ZIP invalide ou corrompu"))?;
        if archive.len() > MAX_ENTRIES {
            return Err(AppError::new("Cette archive contient trop de fichiers"));
        }
        let (format, prefix) = detect(&mut archive)?;
        let settings = matches!(format, Format::Modrinth | Format::XalLauncher)
            .then(|| read_xal_settings(&mut archive, &prefix))
            .flatten();
        (format, prefix, settings)
    };
    match format {
        Format::Modrinth | Format::XalLauncher => {
            let custom_name = name
                .filter(|value| !value.trim().is_empty())
                .or_else(|| settings.as_ref().and_then(|value| value.name.clone()))
                .map(|value| sanitize_name(&value, "Instance importée"));
            let mut instance = crate::modpacks::import_pack(
                state.client.clone(),
                state.store.clone(),
                Some(app),
                path,
                custom_name,
                &prefix,
            )
            .await?;
            if let Some(settings) = &settings {
                apply_settings(&state.store, &mut instance, settings)?;
            }
            let imported_files = count_files(&state.store.game_dir(&instance.id));
            Ok(ImportReport {
                instance,
                format: if settings.is_some() || format == Format::XalLauncher {
                    "XalLauncher"
                } else {
                    "Modrinth"
                }
                .into(),
                imported_files,
                warnings: Vec::new(),
            })
        }
        Format::Prism => {
            let store = state.store.clone();
            tokio::task::spawn_blocking(move || import_prism_archive(&store, &path, &prefix, name))
                .await
                .map_err(|error| AppError::new(error.to_string()))?
        }
        Format::CurseForge => Err(curseforge_error()),
        Format::Unknown => Err(unknown_error()),
    }
}

#[tauri::command]
pub async fn import_instance_folder(
    path: PathBuf,
    name: Option<String>,
    state: tauri::State<'_, crate::AppState>,
) -> Result<ImportReport> {
    if !path.is_dir() {
        return Err(AppError::new("Le dossier à importer est introuvable"));
    }
    let store = state.store.clone();
    tokio::task::spawn_blocking(move || import_prism_folder(&store, &path, name))
        .await
        .map_err(|error| AppError::new(error.to_string()))?
}

fn import_prism_archive(
    store: &Store,
    path: &Path,
    prefix: &str,
    custom_name: Option<String>,
) -> Result<ImportReport> {
    let file = fs::File::open(path)?;
    let mut archive =
        ZipArchive::new(file).map_err(|_| AppError::new("Fichier ZIP invalide ou corrompu"))?;
    if archive.len() > MAX_ENTRIES {
        return Err(AppError::new("Cette archive contient trop de fichiers"));
    }
    let pack_content =
        read_entry(&mut archive, &format!("{prefix}mmc-pack.json")).map_err(|_| {
            AppError::new(
                "mmc-pack.json introuvable : cette archive n’est pas une instance Prism/MultiMC",
            )
        })?;
    let pack: PrismPack =
        serde_json::from_str(&pack_content).map_err(|_| AppError::new("mmc-pack.json invalide"))?;
    let (version_id, loader, loader_version) = prism_target(&pack.components)?;
    let config = read_entry(&mut archive, &format!("{prefix}instance.cfg"))
        .map(|content| parse_instance_cfg(&content))
        .unwrap_or_default();
    let name = custom_name
        .filter(|value| !value.trim().is_empty())
        .or_else(|| config.name.clone())
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("Instance importée")
                .to_string()
        });
    let instance = build_instance(name, version_id, loader, loader_version, config);
    let game_dir = store.game_dir(&instance.id);
    fs::create_dir_all(&game_dir)?;
    let game_prefix = prism_game_prefix(&mut archive, prefix);
    let root_layout = game_prefix == prefix;
    let imported_files =
        match extract_prism_archive(&mut archive, &game_prefix, root_layout, &game_dir) {
            Ok(files) => files,
            Err(error) => {
                let _ = fs::remove_dir_all(store.instance_dir(&instance.id));
                return Err(error);
            }
        };
    let instance = register_instance(store, instance)?;
    let mut warnings = Vec::new();
    if imported_files == 0 {
        warnings.push("Aucun fichier de jeu n’a été trouvé dans l’archive".to_string());
    }
    Ok(ImportReport {
        instance,
        format: "Prism / MultiMC".into(),
        imported_files,
        warnings,
    })
}

fn prism_game_prefix(archive: &mut ZipArchive<fs::File>, prefix: &str) -> String {
    for candidate in [
        format!("{prefix}.minecraft/"),
        format!("{prefix}minecraft/"),
    ] {
        for index in 0..archive.len() {
            if let Ok(entry) = archive.by_index(index) {
                if entry
                    .name()
                    .map(|name| name.starts_with(&candidate))
                    .unwrap_or(false)
                {
                    return candidate;
                }
            }
        }
    }
    prefix.to_string()
}

fn extract_prism_archive(
    archive: &mut ZipArchive<fs::File>,
    game_prefix: &str,
    root_layout: bool,
    game_dir: &Path,
) -> Result<u64> {
    let mut imported = 0_u64;
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        if entry.is_symlink() || entry.is_dir() {
            continue;
        }
        let Some(entry_path) = entry.enclosed_name() else {
            continue;
        };
        let entry_name = entry_path.to_string_lossy().replace('\\', "/");
        let Some(relative) = entry_name.strip_prefix(game_prefix) else {
            continue;
        };
        if relative.is_empty() || should_skip_entry(relative, root_layout) {
            continue;
        }
        total = total.saturating_add(entry.size());
        if total > MAX_TOTAL_BYTES {
            return Err(AppError::new(
                "Cette instance dépasse la taille maximale autorisée (32 Go)",
            ));
        }
        let target = game_dir.join(crate::modpacks::safe_relative(relative)?);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::File::create(target)?;
        std::io::copy(&mut entry, &mut output)?;
        imported += 1;
    }
    Ok(imported)
}

fn should_skip_entry(relative: &str, root_layout: bool) -> bool {
    let first = relative.split('/').next().unwrap_or("");
    if matches!(
        first,
        "logs"
            | "crash-reports"
            | "usercache.json"
            | "launcher_profiles.json"
            | "launcher_accounts.json"
            | "launcher_log.txt"
    ) {
        return true;
    }
    if relative.ends_with(".part")
        || relative.ends_with(".xal-tmp")
        || relative.ends_with(".xal-backup")
    {
        return true;
    }
    root_layout
        && matches!(
            first,
            "instance.cfg" | "mmc-pack.json" | "patches" | "icons" | "jarmods" | "natives"
        )
}

fn import_prism_folder(
    store: &Store,
    path: &Path,
    custom_name: Option<String>,
) -> Result<ImportReport> {
    if path.join("manifest.json").is_file() {
        let content = fs::read_to_string(path.join("manifest.json")).unwrap_or_default();
        if serde_json::from_str::<Value>(&content)
            .map(|value| value["manifestType"] == "minecraftModpack")
            .unwrap_or(false)
        {
            return Err(curseforge_error());
        }
    }
    let pack_path = path.join("mmc-pack.json");
    if !pack_path.is_file() {
        return Err(AppError::new(
            "Dossier Prism/MultiMC non reconnu : mmc-pack.json est absent. \
             Sélectionnez le dossier de l’instance, pas le dossier .minecraft.",
        ));
    }
    let pack: PrismPack = serde_json::from_str(&fs::read_to_string(&pack_path)?)
        .map_err(|_| AppError::new("mmc-pack.json invalide"))?;
    let (version_id, loader, loader_version) = prism_target(&pack.components)?;
    let config = fs::read_to_string(path.join("instance.cfg"))
        .map(|content| parse_instance_cfg(&content))
        .unwrap_or_default();
    let name = custom_name
        .filter(|value| !value.trim().is_empty())
        .or_else(|| config.name.clone())
        .unwrap_or_else(|| {
            path.file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("Instance importée")
                .to_string()
        });
    let instance = build_instance(name, version_id, loader, loader_version, config);
    let (game_root, root_layout) = if path.join(".minecraft").is_dir() {
        (path.join(".minecraft"), false)
    } else if path.join("minecraft").is_dir() {
        (path.join("minecraft"), false)
    } else {
        (path.to_path_buf(), true)
    };
    let game_dir = store.game_dir(&instance.id);
    fs::create_dir_all(&game_dir)?;
    let mut imported_files = 0_u64;
    let mut total_bytes = 0_u64;
    let result = copy_folder_files(
        &game_root,
        &game_root,
        &game_dir,
        root_layout,
        &mut imported_files,
        &mut total_bytes,
    );
    if let Err(error) = result {
        let _ = fs::remove_dir_all(store.instance_dir(&instance.id));
        return Err(error);
    }
    let instance = register_instance(store, instance)?;
    let mut warnings = Vec::new();
    if imported_files == 0 {
        warnings.push("Aucun fichier de jeu n’a été trouvé dans le dossier".to_string());
    }
    Ok(ImportReport {
        instance,
        format: "Prism / MultiMC".into(),
        imported_files,
        warnings,
    })
}

fn copy_folder_files(
    source_root: &Path,
    current: &Path,
    destination: &Path,
    root_layout: bool,
    imported_files: &mut u64,
    total_bytes: &mut u64,
) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        let relative_path = entry
            .path()
            .strip_prefix(source_root)
            .map_err(|_| AppError::new("Chemin d’import invalide"))?
            .to_path_buf();
        let relative = relative_path.to_string_lossy().replace('\\', "/");
        if should_skip_entry(&relative, root_layout) {
            continue;
        }
        if file_type.is_dir() {
            copy_folder_files(
                source_root,
                &entry.path(),
                destination,
                root_layout,
                imported_files,
                total_bytes,
            )?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let size = entry.metadata()?.len();
        *total_bytes = total_bytes.saturating_add(size);
        if *total_bytes > MAX_TOTAL_BYTES {
            return Err(AppError::new(
                "Cette instance dépasse la taille maximale autorisée (32 Go)",
            ));
        }
        let target = destination.join(&relative_path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(entry.path(), target)?;
        *imported_files += 1;
    }
    Ok(())
}

fn prism_target(components: &[PrismComponent]) -> Result<(String, Loader, Option<String>)> {
    let minecraft = components
        .iter()
        .find(|component| component.uid == "net.minecraft")
        .map(|component| component.version.trim().to_string())
        .filter(|value| crate::instances::valid_version(value))
        .ok_or_else(|| AppError::new("Version Minecraft absente ou invalide dans mmc-pack.json"))?;
    for (uid, loader) in [
        ("net.fabricmc.fabric-loader", Loader::Fabric),
        ("org.quiltmc.quilt-loader", Loader::Quilt),
        ("net.minecraftforge", Loader::Forge),
        ("net.neoforged", Loader::Neoforge),
    ] {
        if let Some(component) = components.iter().find(|component| component.uid == uid) {
            let mut version = component.version.trim().to_string();
            if loader == Loader::Forge && !version.starts_with(&format!("{minecraft}-")) {
                version = format!("{minecraft}-{version}");
            }
            if !crate::instances::valid_version(&version) {
                return Err(AppError::new(
                    "Version du modloader invalide dans mmc-pack.json",
                ));
            }
            return Ok((minecraft, loader, Some(version)));
        }
    }
    Ok((minecraft, Loader::Vanilla, None))
}

fn parse_instance_cfg(content: &str) -> PrismConfig {
    let mut config = PrismConfig::default();
    for line in content.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "name" if !value.is_empty() => config.name = Some(value.to_string()),
            "MinMemAlloc" => config.min_memory = value.parse().ok(),
            "MaxMemAlloc" => config.max_memory = value.parse().ok(),
            "JoinServerOnLaunchAddress" if !value.is_empty() => {
                config.server_address = Some(value.to_string());
            }
            _ => {}
        }
    }
    config
}

fn build_instance(
    name: String,
    version_id: String,
    loader: Loader,
    loader_version: Option<String>,
    config: PrismConfig,
) -> Instance {
    let (memory_min, memory_max) = clamp_memory(
        config.min_memory.unwrap_or(512),
        config.max_memory.unwrap_or(4096),
    );
    Instance {
        id: uuid::Uuid::new_v4().to_string(),
        name: sanitize_name(&name, "Instance importée"),
        version_id,
        loader,
        loader_version,
        color: "#8b5cf6".into(),
        memory_min,
        memory_max,
        resolution_width: 1280,
        resolution_height: 720,
        fullscreen: false,
        close_on_launch: false,
        java_path: None,
        jvm_arguments: String::new(),
        server_address: config.server_address.filter(|value| {
            !value.is_empty() && value.len() <= 255 && !value.chars().any(char::is_control)
        }),
        icon_url: None,
        icon_file: None,
        modules: ModuleState::default(),
        created_at: Utc::now(),
        last_played_at: None,
        total_play_seconds: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_zip(path: &Path, entries: &[(&str, &str)]) {
        let file = fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (name, content) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    fn format_of(path: &Path) -> Format {
        let file = fs::File::open(path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        detect(&mut archive).unwrap().0
    }

    #[test]
    fn detects_supported_formats() {
        let directory = tempfile::tempdir().unwrap();
        let modrinth = directory.path().join("pack.mrpack");
        write_zip(&modrinth, &[("modrinth.index.json", "{}")]);
        assert_eq!(format_of(&modrinth), Format::Modrinth);

        let curseforge = directory.path().join("curseforge.zip");
        write_zip(
            &curseforge,
            &[("manifest.json", "{\"manifestType\":\"minecraftModpack\"}")],
        );
        assert_eq!(format_of(&curseforge), Format::CurseForge);

        let prism = directory.path().join("prism.zip");
        write_zip(&prism, &[("mmc-pack.json", "{\"components\":[]}")]);
        assert_eq!(format_of(&prism), Format::Prism);

        let nested = directory.path().join("nested.zip");
        write_zip(&nested, &[("MonPack/mmc-pack.json", "{\"components\":[]}")]);
        assert_eq!(format_of(&nested), Format::Prism);
    }

    #[test]
    fn maps_prism_components() {
        let forge = vec![
            PrismComponent {
                uid: "net.minecraft".into(),
                version: "1.20.1".into(),
            },
            PrismComponent {
                uid: "net.minecraftforge".into(),
                version: "47.2.0".into(),
            },
        ];
        let (minecraft, loader, version) = prism_target(&forge).unwrap();
        assert_eq!(minecraft, "1.20.1");
        assert_eq!(loader, Loader::Forge);
        assert_eq!(version.as_deref(), Some("1.20.1-47.2.0"));

        let vanilla = vec![PrismComponent {
            uid: "net.minecraft".into(),
            version: "1.21.1".into(),
        }];
        let (_, loader, version) = prism_target(&vanilla).unwrap();
        assert_eq!(loader, Loader::Vanilla);
        assert!(version.is_none());
    }

    #[test]
    fn parses_instance_cfg() {
        let config = parse_instance_cfg(
            "[General]\nname=Mon pack\nMaxMemAlloc=6144\nMinMemAlloc=1024\n\
             JoinServerOnLaunchAddress=play.example.fr\n",
        );
        assert_eq!(config.name.as_deref(), Some("Mon pack"));
        assert_eq!(config.max_memory, Some(6144));
        assert_eq!(config.min_memory, Some(1024));
        assert_eq!(config.server_address.as_deref(), Some("play.example.fr"));
    }

    #[test]
    fn imports_prism_archive() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::temporary(directory.path());
        let archive_path = directory.path().join("prism.zip");
        write_zip(
            &archive_path,
            &[
                (
                    "mmc-pack.json",
                    "{\"formatVersion\":1,\"components\":[{\"uid\":\"net.minecraft\",\"version\":\"1.20.1\"},{\"uid\":\"net.fabricmc.fabric-loader\",\"version\":\"0.15.11\"}]}",
                ),
                (
                    "instance.cfg",
                    "name=Pack Prism\nMaxMemAlloc=6144\nMinMemAlloc=1024\n",
                ),
                (".minecraft/mods/example.jar", "jar"),
                (".minecraft/saves/world/level.dat", "level"),
                (".minecraft/logs/latest.log", "log"),
            ],
        );

        let report = import_prism_archive(&store, &archive_path, "", None).unwrap();

        assert_eq!(report.format, "Prism / MultiMC");
        assert_eq!(report.instance.name, "Pack Prism");
        assert_eq!(report.instance.version_id, "1.20.1");
        assert_eq!(report.instance.loader, Loader::Fabric);
        assert_eq!(report.instance.loader_version.as_deref(), Some("0.15.11"));
        assert_eq!(report.instance.memory_max, 6144);
        assert_eq!(report.imported_files, 2);
        let game_dir = store.game_dir(&report.instance.id);
        assert!(game_dir.join("mods/example.jar").is_file());
        assert!(game_dir.join("saves/world/level.dat").is_file());
        assert!(!game_dir.join("logs").exists());
    }

    #[test]
    fn imports_nested_prism_archive() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::temporary(directory.path());
        let archive_path = directory.path().join("nested.zip");
        write_zip(
            &archive_path,
            &[
                (
                    "MonPack/mmc-pack.json",
                    "{\"formatVersion\":1,\"components\":[{\"uid\":\"net.minecraft\",\"version\":\"1.21.1\"}]}",
                ),
                ("MonPack/.minecraft/mods/a.jar", "jar"),
            ],
        );

        let report = import_prism_archive(&store, &archive_path, "MonPack/", None).unwrap();

        assert_eq!(report.instance.version_id, "1.21.1");
        assert_eq!(report.imported_files, 1);
        assert!(store
            .game_dir(&report.instance.id)
            .join("mods/a.jar")
            .is_file());
    }

    #[test]
    fn rejects_curseforge_folder() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::temporary(directory.path());
        fs::write(
            directory.path().join("manifest.json"),
            "{\"manifestType\":\"minecraftModpack\"}",
        )
        .unwrap();
        let error = import_prism_folder(&store, directory.path(), None).unwrap_err();
        assert!(error.to_string().contains("CurseForge"));
    }
}
