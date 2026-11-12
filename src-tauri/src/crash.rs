use crate::error::Result;
use crate::models::{CrashCause, CrashReport};
use crate::store::Store;
use chrono::Utc;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

pub fn diagnose(store: &Store, instance_id: &str) -> Result<CrashReport> {
    let logs_dir = store.root().join("logs");
    let game_dir = store.game_dir(instance_id);
    let mut text = String::new();
    if let Ok(content) = fs::read_to_string(logs_dir.join(format!("{instance_id}.log"))) {
        text.push_str(&content);
    }
    if let Ok(content) = fs::read_to_string(game_dir.join("logs").join("latest.log")) {
        text.push_str(&content);
    }
    let crash_file = newest_crash_report(&game_dir);
    if let Some(path) = &crash_file {
        if let Ok(content) = fs::read_to_string(path) {
            text.push_str(&content);
        }
    }
    let report = analyze(
        instance_id,
        &text,
        crash_file
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
    );
    let _ = fs::create_dir_all(&logs_dir);
    if let Ok(json) = serde_json::to_vec_pretty(&report) {
        let _ = fs::write(logs_dir.join(format!("{instance_id}-diagnosis.json")), json);
    }
    Ok(report)
}

#[tauri::command]
pub fn diagnose_crash(
    instance_id: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<CrashReport> {
    diagnose(&state.store, &instance_id)
}

fn newest_crash_report(game_dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(game_dir.join("crash-reports")).ok()?;
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("crash-"))
        .map(|entry| entry.path())
        .collect();
    files.sort_by_key(|path| path.metadata().and_then(|meta| meta.modified()).ok());
    files.pop()
}

pub fn analyze(instance_id: &str, text: &str, crash_file: Option<String>) -> CrashReport {
    let lower = text.to_lowercase();
    let mut causes = Vec::new();

    if lower.contains("outofmemoryerror") || lower.contains("java heap space") {
        causes.push(CrashCause {
            id: "memory".into(),
            title: "Mémoire insuffisante".into(),
            detail: "La JVM a manqué de mémoire pendant le chargement du jeu ou des mods.".into(),
            action: "Augmentez la mémoire maximale de l’instance (4 Go minimum, 6 à 8 Go avec des mods).".into(),
        });
    }
    if lower.contains("unsupportedclassversionerror")
        || lower.contains("requires java")
        || lower.contains("compiled by a more recent version")
    {
        causes.push(CrashCause {
            id: "java".into(),
            title: "Version de Java incompatible".into(),
            detail: "Le jeu ou un mod a été compilé pour une version de Java plus récente.".into(),
            action: "Vérifiez la version Java détectée pour cette instance ou laissez XalLauncher installer Temurin.".into(),
        });
    }
    if lower.contains("missing or unsupported mandatory dependencies")
        || lower.contains("requires mod")
        || lower.contains("mod resolution encountered an incompatible mod set")
    {
        causes.push(CrashCause {
            id: "dependency".into(),
            title: "Dépendance de mod manquante".into(),
            detail: "Un mod installé réclame une dépendance absente ou incompatible.".into(),
            action: "Installez les dépendances demandées depuis l’onglet Mods ou retirez le mod concerné.".into(),
        });
    }
    if lower.contains("duplicatemodsfoundexception") || lower.contains("duplicate mods") {
        causes.push(CrashCause {
            id: "duplicate".into(),
            title: "Mods en double".into(),
            detail: "Plusieurs fichiers fournissent le même mod.".into(),
            action: "Supprimez les versions en double dans le dossier mods de l’instance.".into(),
        });
    }
    if lower.contains("mixin apply failed")
        || lower.contains("mixinapplyerror")
        || lower.contains("mixin transformation")
    {
        causes.push(CrashCause {
            id: "mixin".into(),
            title: "Conflit Mixin".into(),
            detail: "Un mod tente de modifier une classe déjà transformée par un autre mod.".into(),
            action: "Désactivez les mods les plus récemment ajoutés ou mettez-les à jour.".into(),
        });
    }
    if lower.contains("glfw error")
        || lower.contains("failed to create window")
        || lower.contains("opengl")
        || lower.contains("pixel format")
    {
        causes.push(CrashCause {
            id: "graphics".into(),
            title: "Problème graphique".into(),
            detail: "La création de la fenêtre ou du contexte OpenGL a échoué.".into(),
            action: "Mettez à jour le pilote de votre carte graphique et évitez les surcouches d’affichage.".into(),
        });
    }
    if lower.contains("could not find or load main class")
        || lower.contains("invalid or corrupt jarfile")
        || lower.contains("unable to access jarfile")
    {
        causes.push(CrashCause {
            id: "install".into(),
            title: "Installation corrompue".into(),
            detail: "Le classpath ou un fichier du jeu est introuvable ou corrompu.".into(),
            action: "Relancez l’instance pour retélécharger les fichiers, ou recréez l’instance."
                .into(),
        });
    }
    if lower.contains("address already in use") {
        causes.push(CrashCause {
            id: "port".into(),
            title: "Port déjà utilisé".into(),
            detail: "Un autre processus utilise le port réseau demandé.".into(),
            action: "Fermez l’autre instance de Minecraft ou choisissez un autre port.".into(),
        });
    }
    if causes.is_empty() {
        causes.push(CrashCause {
            id: "unknown".into(),
            title: "Cause non identifiée".into(),
            detail: "Aucun motif connu n’a été détecté dans les journaux.".into(),
            action: "Consultez l’extrait de journal ci-dessous et le rapport de crash complet."
                .into(),
        });
    }

    let summary = first_line(text, "description:")
        .or_else(|| first_line(text, "exception in thread"))
        .unwrap_or_else(|| "Minecraft s’est arrêté de façon inattendue".into());
    let severity = if crash_file.is_some() {
        "fatal"
    } else if causes.iter().any(|cause| cause.id != "unknown") {
        "error"
    } else {
        "warning"
    };

    CrashReport {
        instance_id: instance_id.into(),
        summary,
        severity: severity.into(),
        causes,
        mods: extract_mods(text),
        crash_file,
        log_excerpt: excerpt(text),
        generated_at: Utc::now(),
    }
}

fn first_line(text: &str, needle: &str) -> Option<String> {
    text.lines()
        .find(|line| line.to_lowercase().contains(needle))
        .map(|line| line.trim().chars().take(220).collect())
}

fn extract_mods(text: &str) -> Vec<String> {
    let Ok(pattern) = Regex::new(r#"(?i)(?:mod|modid)[\s:'"]+([a-z0-9_.-]{3,40})"#) else {
        return Vec::new();
    };
    let mut mods = Vec::new();
    for capture in pattern.captures_iter(text) {
        let value = capture[1].to_lowercase();
        if !mods.contains(&value) {
            mods.push(value);
        }
        if mods.len() >= 10 {
            break;
        }
    }
    mods
}

fn excerpt(text: &str) -> String {
    let keywords = [
        "error",
        "exception",
        "caused by",
        "crash",
        "failed",
        "mixin",
        "outofmemory",
    ];
    let mut lines: Vec<&str> = text
        .lines()
        .filter(|line| {
            let lower = line.to_lowercase();
            keywords.iter().any(|keyword| lower.contains(keyword))
        })
        .collect();
    if lines.is_empty() {
        lines = text.lines().collect();
    }
    let start = lines.len().saturating_sub(25);
    lines[start..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::analyze;

    #[test]
    fn detects_memory_crash() {
        let report = analyze(
            "instance",
            "java.lang.OutOfMemoryError: Java heap space",
            None,
        );
        assert!(report.causes.iter().any(|cause| cause.id == "memory"));
        assert_eq!(report.severity, "error");
    }

    #[test]
    fn detects_missing_dependencies() {
        let report = analyze(
            "instance",
            "Mod 'sodium' requires mod 'fabric-api' which is missing",
            None,
        );
        assert!(report.causes.iter().any(|cause| cause.id == "dependency"));
    }

    #[test]
    fn detects_java_version() {
        let report = analyze(
            "instance",
            "java.lang.UnsupportedClassVersionError: compiled by a more recent version",
            None,
        );
        assert!(report.causes.iter().any(|cause| cause.id == "java"));
    }

    #[test]
    fn falls_back_to_unknown_cause() {
        let report = analyze("instance", "Tout va bien", None);
        assert!(report.causes.iter().any(|cause| cause.id == "unknown"));
        assert_eq!(report.severity, "warning");
    }
}
