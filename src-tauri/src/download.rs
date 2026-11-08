use crate::error::{AppError, Result, ResultExt};
use crate::http::ensure_success;
use crate::models::ProgressEvent;
use futures_util::StreamExt;
use reqwest::{Client, StatusCode};
use sha1::{Digest, Sha1};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use tauri::{AppHandle, Emitter};

pub fn sha1_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn valid_sha1(path: &Path, expected: Option<&str>) -> bool {
    match expected {
        Some(hash) if path.is_file() => sha1_file(path)
            .map(|actual| actual.eq_ignore_ascii_case(hash))
            .unwrap_or(false),
        _ => false,
    }
}

fn emit(app: Option<&AppHandle>, operation: &str, label: &str, current: u64, total: u64) {
    if let Some(app) = app {
        let _ = app.emit(
            "download-progress",
            ProgressEvent {
                operation,
                label,
                current,
                total,
            },
        );
    }
}

pub async fn download_file(
    client: &Client,
    url: &str,
    destination: &Path,
    expected_sha1: Option<&str>,
    app: Option<&AppHandle>,
    operation: &str,
    label: &str,
) -> Result<()> {
    if valid_sha1(destination, expected_sha1)
        || (expected_sha1.is_none() && destination.is_file() && destination.metadata()?.len() > 0)
    {
        return Ok(());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let partial = destination.with_extension("part");
    let mut last_error = String::new();

    for attempt in 0..4 {
        let offset = if partial.is_file() {
            partial.metadata()?.len()
        } else {
            0
        };
        let request = if offset > 0 {
            client
                .get(url)
                .header(reqwest::header::RANGE, format!("bytes={offset}-"))
        } else {
            client.get(url)
        };
        match request.send().await {
            Ok(response) => {
                let supports_resume = response.status() == StatusCode::PARTIAL_CONTENT;
                let response =
                    match ensure_success(response, &format!("Téléchargement de {label}")).await {
                        Ok(value) => value,
                        Err(error) => {
                            last_error = error.to_string();
                            continue;
                        }
                    };
                let resume = offset > 0 && supports_resume;
                let start = if resume { offset } else { 0 };
                let total = response
                    .content_length()
                    .map(|length| length + start)
                    .unwrap_or(0);
                let mut file = if resume {
                    fs::OpenOptions::new().append(true).open(&partial)?
                } else {
                    fs::File::create(&partial)?
                };
                let mut stream = response.bytes_stream();
                let mut current = start;
                let mut last_emit = 0_u64;
                let mut failed = None;
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(bytes) => {
                            if let Err(error) = file.write_all(&bytes) {
                                failed = Some(error.to_string());
                                break;
                            }
                            current += bytes.len() as u64;
                            if current.saturating_sub(last_emit) >= 512 * 1024 || current == total {
                                emit(app, operation, label, current, total);
                                last_emit = current;
                            }
                        }
                        Err(error) => {
                            failed = Some(error.to_string());
                            break;
                        }
                    }
                }
                if let Some(error) = failed {
                    last_error = error;
                    continue;
                }
                if let Some(expected) = expected_sha1 {
                    let actual = sha1_file(&partial)?;
                    if !actual.eq_ignore_ascii_case(expected) {
                        last_error = format!("Somme SHA-1 incorrecte pour {label}");
                        let _ = fs::remove_file(&partial);
                        continue;
                    }
                }
                if destination.exists() {
                    fs::remove_file(destination)?;
                }
                fs::rename(&partial, destination)
                    .context(format!("Installation de {}", destination.display()))?;
                emit(
                    app,
                    operation,
                    label,
                    current.max(1),
                    total.max(current).max(1),
                );
                return Ok(());
            }
            Err(error) => last_error = error.to_string(),
        }
        if attempt < 3 {
            tokio::time::sleep(std::time::Duration::from_millis(400 * (attempt + 1) as u64)).await;
        }
    }
    Err(AppError::new(format!(
        "Échec après plusieurs tentatives : {last_error}"
    )))
}
