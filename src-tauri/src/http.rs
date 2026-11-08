use crate::error::{AppError, Result, ResultExt};
use reqwest::{Client, Response};
use serde::de::DeserializeOwned;
use std::time::Duration;

pub const USER_AGENT: &str = concat!(
    "XaltarStudio/XalLauncher/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/RitchiQc/XalLauncher)"
);

pub fn client() -> Result<Client> {
    Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::limited(8))
        .https_only(true)
        .build()
        .context("Initialisation du client réseau")
}

pub async fn ensure_success(response: Response, action: &str) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().await.unwrap_or_default();
    let detail: String = body.chars().take(700).collect();
    Err(AppError::new(format!("{action} ({status}) : {detail}")))
}

pub async fn get_json<T: DeserializeOwned>(client: &Client, url: &str) -> Result<T> {
    let response = client
        .get(url)
        .send()
        .await
        .context(format!("Connexion à {url}"))?;
    ensure_success(response, "Requête réseau")
        .await?
        .json::<T>()
        .await
        .context("Réponse JSON invalide")
}
