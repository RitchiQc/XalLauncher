pub(crate) mod export;
pub(crate) mod import;

use crate::error::Result;
use crate::models::Instance;
use crate::store::Store;
use serde::Serialize;
use std::fs;
use std::path::Path;

pub(crate) const MAX_ENTRIES: usize = 200_000;
pub(crate) const MAX_TOTAL_BYTES: u64 = 32 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Modrinth,
    XalLauncher,
    Prism,
    CurseForge,
    Unknown,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub instance: Instance,
    pub format: String,
    pub imported_files: u64,
    pub warnings: Vec<String>,
}

pub(crate) fn register_instance(store: &Store, instance: Instance) -> Result<Instance> {
    fs::create_dir_all(store.game_dir(&instance.id))?;
    let mut instances = store.instances()?;
    instances.push(instance.clone());
    store.save_instances(&instances)?;
    Ok(instance)
}

pub(crate) fn sanitize_name(name: &str, fallback: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .filter(|value| !value.is_control())
        .collect();
    let cleaned = if cleaned.chars().count() > 48 {
        cleaned.chars().take(48).collect()
    } else {
        cleaned
    };
    if cleaned.chars().count() < 2 {
        fallback.to_string()
    } else {
        cleaned
    }
}

pub(crate) fn clamp_memory(min: u32, max: u32) -> (u32, u32) {
    let min = min.clamp(256, 65536);
    let max = max.clamp(1024, 65536).max(min);
    (min, max)
}

pub(crate) fn valid_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.chars().skip(1).all(|char| char.is_ascii_hexdigit())
}

pub(crate) fn count_files(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                count_files(&path)
            } else {
                1
            }
        })
        .sum()
}

pub(crate) fn curseforge_error() -> crate::error::AppError {
    crate::error::AppError::new(
        "Les packs CurseForge ne sont pas pris en charge : leurs mods nécessitent une clé API CurseForge. \
         Importez d’abord le pack dans Prism Launcher, puis importez le dossier de l’instance Prism dans XalLauncher.",
    )
}

pub(crate) fn unknown_error() -> crate::error::AppError {
    crate::error::AppError::new(
        "Format non reconnu. Formats supportés : Modrinth (.mrpack) et Prism/MultiMC (.zip).",
    )
}
