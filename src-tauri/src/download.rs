use crate::error::{AppError, Result, ResultExt};
use crate::http::ensure_success;
use crate::models::ProgressEvent;
use futures_util::StreamExt;
use reqwest::{Client, StatusCode};
use sha1::{Digest, Sha1};
use sha2::{Sha256, Sha512};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Debug)]
pub enum Checksum {
    Sha1(String),
    Sha256(String),
    Sha512(String),
}

impl Checksum {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Sha1(_) => "SHA-1",
            Self::Sha256(_) => "SHA-256",
            Self::Sha512(_) => "SHA-512",
        }
    }

    fn value(&self) -> &str {
        match self {
            Self::Sha1(value) | Self::Sha256(value) | Self::Sha512(value) => value,
        }
    }
}

fn hash_file<D: Digest>(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = D::new();
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

pub fn sha256_file(path: &Path) -> Result<String> {
    hash_file::<Sha256>(path)
}

pub fn checksum_file(path: &Path, expected: &Checksum) -> Result<String> {
    match expected {
        Checksum::Sha1(_) => hash_file::<Sha1>(path),
        Checksum::Sha256(_) => hash_file::<Sha256>(path),
        Checksum::Sha512(_) => hash_file::<Sha512>(path),
    }
}

pub fn valid_checksum(path: &Path, expected: Option<&Checksum>) -> bool {
    match expected {
        Some(hash) if path.is_file() => checksum_file(path, hash)
            .map(|actual| actual.eq_ignore_ascii_case(hash.value()))
            .unwrap_or(false),
        _ => false,
    }
}

pub async fn maven_checksum(client: &Client, url: &str) -> Result<Checksum> {
    let mut last_error = "Aucun checksum Maven disponible".to_string();
    for (extension, length) in [("sha256", 64), ("sha1", 40)] {
        let checksum_url = format!("{url}.{extension}");
        match client.get(&checksum_url).send().await {
            Ok(response) if response.status().is_success() => match response.text().await {
                Ok(text) => {
                    let value = text.split_whitespace().next().unwrap_or("");
                    if value.len() == length && value.chars().all(|char| char.is_ascii_hexdigit()) {
                        return Ok(if length == 64 {
                            Checksum::Sha256(value.to_string())
                        } else {
                            Checksum::Sha1(value.to_string())
                        });
                    }
                    last_error = format!("Checksum {extension} invalide pour {url}");
                }
                Err(error) => last_error = error.to_string(),
            },
            Ok(response) => {
                last_error = format!("Checksum {extension} absent ({})", response.status())
            }
            Err(error) => last_error = error.to_string(),
        }
    }
    Err(AppError::new(last_error))
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
    let expected = expected_sha1.map(|hash| Checksum::Sha1(hash.to_string()));
    download_file_with_checksum(
        client,
        url,
        destination,
        expected.as_ref(),
        app,
        operation,
        label,
    )
    .await
}

pub async fn download_file_with_checksum(
    client: &Client,
    url: &str,
    destination: &Path,
    expected: Option<&Checksum>,
    app: Option<&AppHandle>,
    operation: &str,
    label: &str,
) -> Result<()> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|_| AppError::new(format!("URL de téléchargement invalide pour {label}")))?;
    if parsed.scheme() != "https" {
        return Err(AppError::new(format!(
            "Téléchargement refusé pour {label} : HTTPS obligatoire"
        )));
    }
    if valid_checksum(destination, expected)
        || (expected.is_none() && destination.is_file() && destination.metadata()?.len() > 0)
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
                if let Some(expected) = expected {
                    if !valid_checksum(&partial, Some(expected)) {
                        last_error = format!("Somme {} incorrecte pour {label}", expected.name());
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

#[cfg(test)]
mod tests {
    use super::{valid_checksum, Checksum};
    use std::fs;

    #[test]
    fn verifies_supported_checksums() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("artifact.jar");
        fs::write(&path, b"abc").unwrap();
        let checksums = [
            Checksum::Sha1("a9993e364706816aba3e25717850c26c9cd0d89d".into()),
            Checksum::Sha256(
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".into(),
            ),
            Checksum::Sha512(
                concat!(
                    "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a",
                    "2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
                )
                .into(),
            ),
        ];
        for checksum in &checksums {
            assert!(valid_checksum(&path, Some(checksum)));
        }
        let wrong = Checksum::Sha256("00".repeat(32));
        assert!(!valid_checksum(&path, Some(&wrong)));
    }

    #[tokio::test]
    async fn refuses_insecure_downloads() {
        let directory = tempfile::tempdir().unwrap();
        let error = super::download_file_with_checksum(
            &reqwest::Client::new(),
            "http://example.com/artifact.jar",
            &directory.path().join("artifact.jar"),
            None,
            None,
            "test",
            "test",
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("HTTPS obligatoire"));
    }
}
