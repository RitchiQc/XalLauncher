use crate::download::download_file;
use crate::error::{AppError, Result, ResultExt};
use crate::models::ProgressEvent;
use crate::store::Store;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

fn java_executable(home: &Path) -> PathBuf {
    home.join("bin")
        .join(if cfg!(windows) { "java.exe" } else { "java" })
}

fn find_java(directory: &Path, depth: usize) -> Option<PathBuf> {
    let direct = java_executable(directory);
    if direct.is_file() {
        return Some(direct);
    }
    if depth == 0 {
        return None;
    }
    for entry in fs::read_dir(directory).ok()?.flatten() {
        if entry.file_type().ok()?.is_dir() {
            if let Some(path) = find_java(&entry.path(), depth - 1) {
                return Some(path);
            }
        }
    }
    None
}

pub async fn ensure_java(
    client: &reqwest::Client,
    store: &Store,
    major: u64,
    app: Option<&AppHandle>,
) -> Result<PathBuf> {
    let runtime_root = store.root().join("runtimes");
    let destination = runtime_root.join(major.to_string());
    if let Some(java) = find_java(&destination, 3) {
        return Ok(java);
    }
    if let Some(app) = app {
        let _ = app.emit(
            "download-progress",
            ProgressEvent {
                operation: "launch",
                label: "Installation du runtime Java",
                current: 0,
                total: 1,
            },
        );
    }
    fs::create_dir_all(&runtime_root)?;
    let archive = store
        .root()
        .join("cache")
        .join(format!("temurin-{major}.zip"));
    let (os, arch) = if cfg!(windows) {
        ("windows", "x64")
    } else if cfg!(target_os = "macos") {
        ("mac", "x64")
    } else {
        ("linux", "x64")
    };
    let url = format!("https://api.adoptium.net/v3/binary/latest/{major}/ga/{os}/{arch}/jre/hotspot/normal/eclipse");
    if let Err(error) = download_file(
        client,
        &url,
        &archive,
        None,
        app,
        "launch",
        "Runtime Java Temurin",
    )
    .await
    {
        let _ = fs::remove_file(&archive);
        let fallback = format!("https://api.adoptium.net/v3/binary/latest/{major}/ga/{os}/{arch}/jdk/hotspot/normal/eclipse");
        download_file(
            client,
            &fallback,
            &archive,
            None,
            app,
            "launch",
            "JDK Java Temurin",
        )
        .await
        .map_err(|_| AppError::new(format!("Impossible de télécharger Java {major} : {error}")))?;
    }

    let temporary = runtime_root.join(format!(".install-{major}"));
    if temporary.exists() {
        fs::remove_dir_all(&temporary)?;
    }
    fs::create_dir_all(&temporary)?;
    let extraction = extract_zip(&archive, &temporary);
    if let Err(error) = extraction {
        let _ = fs::remove_file(&archive);
        let _ = fs::remove_dir_all(&temporary);
        return Err(error);
    }
    let java = find_java(&temporary, 4)
        .ok_or_else(|| AppError::new("L’archive Java ne contient pas d’exécutable"))?;
    let home = java
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| AppError::new("Installation Java invalide"))?;
    if destination.exists() {
        fs::remove_dir_all(&destination)?;
    }
    fs::rename(home, &destination).context("Finalisation de l’installation Java")?;
    let _ = fs::remove_dir_all(&temporary);
    Ok(java_executable(&destination))
}

fn extract_zip(archive: &Path, destination: &Path) -> Result<()> {
    let file = fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| AppError::new("Chemin dangereux dans l’archive Java"))?
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
        io::copy(&mut entry, &mut output)?;
    }
    Ok(())
}
