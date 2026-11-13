use crate::error::{AppError, Result, ResultExt};
use crate::models::{Account, Cape, DeviceCodeEvent};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{Duration, Utc};
use keyring::Entry;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};
use tauri_plugin_opener::OpenerExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;

const SERVICE: &str = "studio.xaltar.xallauncher";
const DEVICE_CODE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const SCOPE: &str = "XboxLive.signin offline_access";
const DEVICE_CODE_ENDPOINT: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN_ENDPOINT: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const AUTHORIZE_ENDPOINT: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize";
const CHUNK_SIZE: usize = 1000;
const BROWSER_TIMEOUT_SECONDS: u64 = 180;

async fn ensure_auth_success(
    response: reqwest::Response,
    action: &str,
) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    Err(AppError::new(format!("{action} ({})", response.status())))
}

fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn pkce_pair() -> (String, String) {
    let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let challenge = pkce_challenge(&verifier);
    (verifier, challenge)
}

fn authorize_url(
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(AUTHORIZE_ENDPOINT)
        .map_err(|error| AppError::new(format!("URL Microsoft invalide : {error}")))?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_mode", "query")
        .append_pair("scope", SCOPE)
        .append_pair("prompt", "select_account")
        .append_pair("state", state)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(url)
}

fn parse_oauth_callback(request_target: &str, expected_state: &str) -> Result<String> {
    let url = reqwest::Url::parse(&format!("http://localhost{request_target}"))
        .map_err(|_| AppError::new("Réponse Microsoft illisible"))?;
    let mut code = None;
    let mut state = None;
    let mut error = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            _ => {}
        }
    }
    if let Some(error) = error {
        return Err(AppError::new(if error == "access_denied" {
            "Connexion annulée dans le navigateur".to_string()
        } else {
            format!("Connexion Microsoft refusée : {error}")
        }));
    }
    if state.as_deref() != Some(expected_state) {
        return Err(AppError::new(
            "Réponse Microsoft invalide : état de sécurité incorrect",
        ));
    }
    code.ok_or_else(|| AppError::new("Réponse Microsoft incomplète : code absent"))
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

async fn respond_html(
    stream: &mut tokio::net::TcpStream,
    status: &str,
    title: &str,
    message: &str,
) {
    let title = escape_html(title);
    let message = escape_html(message);
    let body = format!(
        "<!doctype html><html lang=\"fr\"><head><meta charset=\"utf-8\"><title>{title}</title></head><body style=\"margin:0;height:100vh;display:grid;place-items:center;background:#090b12;color:#ececf5;font-family:system-ui,sans-serif\"><main style=\"text-align:center;padding:32px\"><h1 style=\"font-size:22px\">{title}</h1><p style=\"color:#8d94a9\">{message}</p></main></body></html>"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;
}

async fn wait_until_cancelled(flag: &AtomicBool) {
    while !flag.load(Ordering::SeqCst) {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

async fn accept_any(
    ipv4: &TcpListener,
    ipv6: Option<&TcpListener>,
) -> std::io::Result<(tokio::net::TcpStream, std::net::SocketAddr)> {
    match ipv6 {
        Some(ipv6) => tokio::select! {
            result = ipv4.accept() => result,
            result = ipv6.accept() => result,
        },
        None => ipv4.accept().await,
    }
}

async fn wait_for_oauth_code(
    listener: TcpListener,
    ipv6_listener: Option<TcpListener>,
    expected_state: &str,
    cancel: &AtomicBool,
) -> Result<String> {
    let deadline =
        tokio::time::Instant::now() + std::time::Duration::from_secs(BROWSER_TIMEOUT_SECONDS);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(AppError::new(
                "Délai dépassé : aucune réponse du navigateur. Vérifiez que l’URI http://localhost est autorisée dans l’application Entra, ou utilisez le code d’appareil.",
            ));
        }
        tokio::select! {
            accepted = tokio::time::timeout(remaining, accept_any(&listener, ipv6_listener.as_ref())) => {
                let (mut stream, _) = match accepted {
                    Ok(Ok(value)) => value,
                    Ok(Err(error)) => return Err(AppError::new(format!("Réception de la réponse Microsoft impossible : {error}"))),
                    Err(_) => continue,
                };
                let mut buffer = vec![0u8; 8192];
                let read = tokio::time::timeout(std::time::Duration::from_secs(5), stream.read(&mut buffer)).await;
                let size = match read {
                    Ok(Ok(size)) if size > 0 => size,
                    _ => continue,
                };
                let request = String::from_utf8_lossy(&buffer[..size]);
                let target = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                match parse_oauth_callback(target, expected_state) {
                    Ok(code) => {
                        respond_html(&mut stream, "200 OK", "Connexion réussie", "Vous pouvez fermer cette fenêtre et revenir à XalLauncher.").await;
                        return Ok(code);
                    }
                    Err(error) => {
                        respond_html(&mut stream, "400 Bad Request", "Connexion impossible", &error.to_string()).await;
                        return Err(error);
                    }
                }
            }
            _ = wait_until_cancelled(cancel) => {
                return Err(AppError::new("Connexion annulée"));
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Credentials {
    refresh_token: String,
    minecraft_token: String,
    expires_at: chrono::DateTime<Utc>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

fn credential_entry(account_id: &str) -> Result<Entry> {
    Entry::new(SERVICE, account_id)
        .map_err(|error| AppError::new(format!("Coffre d’identification : {error}")))
}

fn meta_key(account_id: &str) -> String {
    format!("{account_id}:meta")
}

fn chunk_key(account_id: &str, index: usize) -> String {
    format!("{account_id}:{index}")
}

fn split_secret(secret: &str) -> Vec<String> {
    let characters: Vec<char> = secret.chars().collect();
    characters
        .chunks(CHUNK_SIZE)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

fn join_secret(chunks: &[String]) -> String {
    chunks.concat()
}

fn vault_error(error: keyring::Error) -> AppError {
    AppError::new(format!(
        "Coffre d’identification Windows inaccessible : {error}"
    ))
}

fn save_credentials(account_id: &str, credentials: &Credentials) -> Result<()> {
    let payload = serde_json::to_string(credentials)?;
    let chunks = split_secret(&payload);
    let previous = credential_entry(&meta_key(account_id))?
        .get_password()
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    for (index, chunk) in chunks.iter().enumerate() {
        credential_entry(&chunk_key(account_id, index))?.set_password(chunk)?;
    }
    for index in chunks.len()..previous {
        if let Ok(entry) = credential_entry(&chunk_key(account_id, index)) {
            let _ = entry.delete_credential();
        }
    }
    credential_entry(&meta_key(account_id))?.set_password(&chunks.len().to_string())?;
    Ok(())
}

fn read_secret(account_id: &str) -> Result<Option<String>> {
    let meta = credential_entry(&meta_key(account_id))?;
    match meta.get_password() {
        Ok(value) => {
            let count = value.parse::<usize>().map_err(|_| {
                AppError::new("Coffre d’identification : entrée de session corrompue")
            })?;
            let mut chunks = Vec::with_capacity(count);
            for index in 0..count {
                chunks.push(credential_entry(&chunk_key(account_id, index))?.get_password()?);
            }
            Ok(Some(join_secret(&chunks)))
        }
        Err(keyring::Error::NoEntry) => match credential_entry(account_id)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(vault_error(error)),
        },
        Err(error) => Err(vault_error(error)),
    }
}

fn read_credentials(account_id: &str) -> Result<Credentials> {
    let payload = read_secret(account_id)?
        .ok_or_else(|| AppError::new("Session absente : reconnectez ce compte Microsoft"))?;
    serde_json::from_str(&payload)
        .map_err(|_| AppError::new("Session illisible : reconnectez ce compte Microsoft"))
}

fn delete_credentials(account_id: &str) {
    let count = credential_entry(&meta_key(account_id))
        .ok()
        .and_then(|entry| entry.get_password().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    for index in 0..count {
        if let Ok(entry) = credential_entry(&chunk_key(account_id, index)) {
            let _ = entry.delete_credential();
        }
    }
    for key in [meta_key(account_id), account_id.to_string()] {
        if let Ok(entry) = credential_entry(&key) {
            let _ = entry.delete_credential();
        }
    }
}

#[tauri::command]
pub fn list_accounts(state: tauri::State<'_, crate::AppState>) -> Result<Vec<Account>> {
    state.store.accounts()
}

#[tauri::command]
pub fn remove_account(
    account_id: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Vec<Account>> {
    let mut accounts = state.store.accounts()?;
    accounts.retain(|account| account.id != account_id);
    state.store.save_accounts(&accounts)?;
    delete_credentials(&account_id);
    Ok(accounts)
}

async fn device_code_login(
    client_id: &str,
    app: &AppHandle,
    state: &crate::AppState,
) -> Result<TokenResponse> {
    let device = state
        .client
        .post(DEVICE_CODE_ENDPOINT)
        .form(&[("client_id", client_id), ("scope", SCOPE)])
        .send()
        .await
        .context("Ouverture de la connexion Microsoft")?;
    let device = ensure_auth_success(device, "Demande du code Microsoft").await?;
    let device: Value = device.json().await?;
    let device_code = device["device_code"]
        .as_str()
        .ok_or_else(|| AppError::new("Réponse Microsoft incomplète"))?
        .to_string();
    let event = DeviceCodeEvent {
        user_code: device["user_code"].as_str().unwrap_or("").to_string(),
        verification_uri: device["verification_uri"]
            .as_str()
            .unwrap_or("https://microsoft.com/link")
            .to_string(),
        expires_in: device["expires_in"].as_u64().unwrap_or(900),
        message: device["message"].as_str().unwrap_or("").to_string(),
    };
    app.emit("auth-device-code", event)
        .map_err(|error| AppError::new(error.to_string()))?;
    let mut interval = device["interval"].as_u64().unwrap_or(5).max(1);

    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(interval)) => {}
            _ = wait_until_cancelled(&state.auth_cancel) => {
                return Err(AppError::new("Connexion annulée"));
            }
        }
        let response = state
            .client
            .post(TOKEN_ENDPOINT)
            .form(&[
                ("grant_type", DEVICE_CODE_GRANT),
                ("client_id", client_id),
                ("device_code", device_code.as_str()),
            ])
            .send()
            .await?;
        let status = response.status();
        let body: Value = response.json().await?;
        if status.is_success() {
            return Ok(serde_json::from_value::<TokenResponse>(body)?);
        }
        match body["error"].as_str().unwrap_or("") {
            "authorization_pending" => continue,
            "slow_down" => {
                interval += 5;
                continue;
            }
            "authorization_declined" => {
                return Err(AppError::new("Connexion refusée dans le navigateur"))
            }
            "expired_token" => return Err(AppError::new("Le code de connexion a expiré")),
            other => {
                return Err(AppError::new(format!(
                    "Connexion Microsoft impossible : {other}"
                )))
            }
        }
    }
}

async fn browser_login(
    client_id: &str,
    app: &AppHandle,
    state: &crate::AppState,
) -> Result<TokenResponse> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|error| AppError::new(format!("Ouverture du port local impossible : {error}")))?;
    let port = listener
        .local_addr()
        .map_err(|error| AppError::new(format!("Port local introuvable : {error}")))?
        .port();
    let ipv6_listener = TcpListener::bind(("::1", port)).await.ok();
    let redirect_uri = format!("http://localhost:{port}");
    let (verifier, challenge) = pkce_pair();
    let csrf_state = Uuid::new_v4().to_string();
    let url = authorize_url(client_id, &redirect_uri, &csrf_state, &challenge)?;
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|error| AppError::new(format!("Ouverture du navigateur impossible : {error}")))?;
    let code =
        wait_for_oauth_code(listener, ipv6_listener, &csrf_state, &state.auth_cancel).await?;
    let response = state
        .client
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("client_id", client_id),
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("code_verifier", verifier.as_str()),
            ("scope", SCOPE),
        ])
        .send()
        .await
        .context("Échange du code Microsoft")?;
    let response = ensure_auth_success(response, "Connexion Microsoft dans le navigateur").await?;
    Ok(response.json().await?)
}

fn save_account(
    state: &crate::AppState,
    account: &Account,
    credentials: &Credentials,
) -> Result<()> {
    save_credentials(&account.id, credentials)?;
    let mut accounts = state.store.accounts()?;
    accounts.retain(|item| item.id != account.id);
    accounts.insert(0, account.clone());
    state.store.save_accounts(&accounts)?;
    Ok(())
}

#[tauri::command]
pub async fn begin_microsoft_login(
    client_id: String,
    mode: Option<String>,
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Account> {
    if client_id.trim().is_empty() {
        return Err(AppError::new(
            "Renseignez d’abord l’identifiant de l’application Microsoft Entra dans les paramètres",
        ));
    }
    let client_id = client_id.trim().to_string();
    state.auth_cancel.store(false, Ordering::SeqCst);
    let token = if mode.as_deref() == Some("browser") {
        browser_login(&client_id, &app, &state).await?
    } else {
        device_code_login(&client_id, &app, &state).await?
    };
    let (account, credentials) = minecraft_account(&state.client, token).await?;
    save_account(&state, &account, &credentials)?;
    Ok(account)
}

#[tauri::command]
pub fn cancel_microsoft_login(state: tauri::State<'_, crate::AppState>) {
    state.auth_cancel.store(true, Ordering::SeqCst);
}

async fn minecraft_account(
    client: &reqwest::Client,
    token: TokenResponse,
) -> Result<(Account, Credentials)> {
    let xbl_response = client.post("https://user.auth.xboxlive.com/user/authenticate")
        .json(&json!({
            "Properties": { "AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com", "RpsTicket": format!("d={}", token.access_token) },
            "RelyingParty": "http://auth.xboxlive.com", "TokenType": "JWT"
        })).send().await?;
    let xbl: Value = ensure_auth_success(xbl_response, "Authentification Xbox Live")
        .await?
        .json()
        .await?;
    let xbl_token = required_string(&xbl, "Token")?;
    let user_hash = required_string(&xbl, "/DisplayClaims/xui/0/uhs")?;

    let xsts_response = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .json(
            &json!({ "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
            "RelyingParty": "rp://api.minecraftservices.com/", "TokenType": "JWT" }),
        )
        .send()
        .await?;
    let status = xsts_response.status();
    let xsts: Value = xsts_response.json().await?;
    if !status.is_success() {
        let code = xsts["XErr"].as_i64().unwrap_or_default();
        let message = match code {
            2148916233 => "Ce compte Microsoft n’a pas de profil Xbox",
            2148916235 => "Xbox Live n’est pas disponible dans cette région",
            2148916236 | 2148916237 => {
                "Le compte Xbox nécessite une vérification ou l’accord d’un adulte"
            }
            2148916238 => "Ce compte Xbox est restreint",
            _ => "Échec de l’autorisation Xbox XSTS",
        };
        return Err(AppError::new(format!("{message} (code {code})")));
    }
    let xsts_token = required_string(&xsts, "Token")?;
    let identity = format!("XBL3.0 x={user_hash};{xsts_token}");
    let minecraft_response = client
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&json!({ "identityToken": identity }))
        .send()
        .await?;
    let minecraft: Value = ensure_auth_success(minecraft_response, "Authentification Minecraft")
        .await?
        .json()
        .await?;
    let minecraft_token = required_string(&minecraft, "access_token")?;
    let expires_in = minecraft["expires_in"]
        .as_i64()
        .or(token.expires_in)
        .unwrap_or(3600);

    let entitlements_response = client
        .get("https://api.minecraftservices.com/entitlements/mcstore")
        .bearer_auth(&minecraft_token)
        .send()
        .await?;
    let entitlements: Value = ensure_auth_success(
        entitlements_response,
        "Vérification de la licence Minecraft",
    )
    .await?
    .json()
    .await?;
    if entitlements["items"]
        .as_array()
        .map(|items| items.is_empty())
        .unwrap_or(true)
    {
        return Err(AppError::new(
            "Ce compte Microsoft ne possède pas Minecraft: Java Edition. Utilisez « Utiliser un autre compte Microsoft » pour choisir le compte qui possède le jeu.",
        ));
    }
    let mut account = fetch_profile(client, &minecraft_token).await?;
    account.expires_at = Utc::now() + Duration::seconds(expires_in);
    let credentials = Credentials {
        refresh_token: token
            .refresh_token
            .ok_or_else(|| AppError::new("Microsoft n’a pas fourni de jeton de renouvellement"))?,
        minecraft_token,
        expires_at: account.expires_at,
    };
    Ok((account, credentials))
}

fn required_string(value: &Value, pointer: &str) -> Result<String> {
    let value = if pointer.starts_with('/') {
        value.pointer(pointer)
    } else {
        value.get(pointer)
    };
    value
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| AppError::new(format!("Réponse d’authentification incomplète : {pointer}")))
}

async fn fetch_profile(client: &reqwest::Client, token: &str) -> Result<Account> {
    let response = client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(token)
        .send()
        .await?;
    let profile: Value = ensure_auth_success(response, "Lecture du profil Minecraft")
        .await?
        .json()
        .await?;
    let skin = profile["skins"].as_array().and_then(|skins| {
        skins
            .iter()
            .find(|skin| skin["state"] == "ACTIVE")
            .or_else(|| skins.first())
    });
    let skin_url = skin
        .and_then(|skin| skin["url"].as_str())
        .map(secure_texture_url);
    let skin_variant = skin
        .and_then(|skin| skin["variant"].as_str())
        .map(|variant| variant.to_lowercase());
    let capes = profile["capes"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value::<Cape>(item.clone()).ok())
                .map(|cape| Cape {
                    url: secure_texture_url(&cape.url),
                    ..cape
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Account {
        id: required_string(&profile, "id")?,
        name: required_string(&profile, "name")?,
        skin_url,
        skin_variant,
        capes,
        expires_at: Utc::now(),
    })
}

fn secure_texture_url(url: &str) -> String {
    url.replacen(
        "http://textures.minecraft.net",
        "https://textures.minecraft.net",
        1,
    )
}

pub async fn valid_token(state: &crate::AppState, account_id: &str) -> Result<(Account, String)> {
    let account = state
        .store
        .accounts()?
        .into_iter()
        .find(|item| item.id == account_id)
        .ok_or_else(|| AppError::new("Compte Minecraft introuvable"))?;
    let credentials = read_credentials(account_id)?;
    if credentials.expires_at > Utc::now() + Duration::seconds(60) {
        return Ok((account, credentials.minecraft_token));
    }
    let client_id = state.store.settings()?.microsoft_client_id;
    if client_id.is_empty() {
        return Err(AppError::new("Identifiant Microsoft Entra absent"));
    }
    let response = state
        .client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&[
            ("client_id", client_id.as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", credentials.refresh_token.as_str()),
            ("scope", "XboxLive.signin offline_access"),
        ])
        .send()
        .await?;
    let token: TokenResponse =
        ensure_auth_success(response, "Renouvellement de la session Microsoft")
            .await?
            .json()
            .await?;
    let (account, credentials) = minecraft_account(&state.client, token).await?;
    save_credentials(account_id, &credentials)?;
    let mut accounts = state.store.accounts()?;
    if let Some(stored) = accounts.iter_mut().find(|item| item.id == account_id) {
        *stored = account.clone();
    }
    state.store.save_accounts(&accounts)?;
    Ok((account, credentials.minecraft_token))
}

#[tauri::command]
pub async fn refresh_account(
    account_id: String,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Account> {
    let (_, token) = valid_token(&state, &account_id).await?;
    let account = fetch_profile(&state.client, &token).await?;
    let mut accounts = state.store.accounts()?;
    let stored = accounts
        .iter_mut()
        .find(|item| item.id == account_id)
        .ok_or_else(|| AppError::new("Compte introuvable"))?;
    let expires_at = stored.expires_at;
    *stored = Account {
        expires_at,
        ..account.clone()
    };
    state.store.save_accounts(&accounts)?;
    Ok(Account {
        expires_at,
        ..account
    })
}

#[tauri::command]
pub async fn set_active_cape(
    account_id: String,
    cape_id: Option<String>,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Account> {
    let (_, token) = valid_token(&state, &account_id).await?;
    let request = state
        .client
        .request(
            if cape_id.is_some() {
                reqwest::Method::PUT
            } else {
                reqwest::Method::DELETE
            },
            "https://api.minecraftservices.com/minecraft/profile/capes/active",
        )
        .bearer_auth(&token);
    let request = if let Some(cape_id) = cape_id {
        request.json(&json!({ "capeId": cape_id }))
    } else {
        request
    };
    let response = request.send().await?;
    ensure_auth_success(response, "Modification de la cape officielle").await?;
    let account = fetch_profile(&state.client, &token).await?;
    let mut accounts = state.store.accounts()?;
    let stored = accounts
        .iter_mut()
        .find(|item| item.id == account_id)
        .ok_or_else(|| AppError::new("Compte introuvable"))?;
    let expires_at = stored.expires_at;
    *stored = Account {
        expires_at,
        ..account.clone()
    };
    state.store.save_accounts(&accounts)?;
    Ok(Account {
        expires_at,
        ..account
    })
}

#[cfg(test)]
mod tests {
    use super::{
        authorize_url, delete_credentials, escape_html, join_secret, parse_oauth_callback,
        pkce_challenge, pkce_pair, read_credentials, read_secret, save_credentials,
        secure_texture_url, split_secret, Credentials, Duration, Utc, DEVICE_CODE_GRANT,
    };

    #[test]
    fn escapes_callback_page_content() {
        assert_eq!(
            escape_html("<script>alert('x')</script>"),
            "&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;"
        );
    }

    #[test]
    fn uses_microsoft_device_code_grant() {
        assert_eq!(
            DEVICE_CODE_GRANT,
            "urn:ietf:params:oauth:grant-type:device_code"
        );
    }

    #[test]
    fn derives_pkce_challenge_from_rfc7636_vector() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn generates_valid_pkce_pairs() {
        let (verifier, challenge) = pkce_pair();
        assert!((43..=128).contains(&verifier.len()));
        assert_eq!(pkce_challenge(&verifier), challenge);
    }

    #[test]
    fn builds_authorize_url_with_account_picker() {
        let url = authorize_url(
            "client-id",
            "http://localhost:45871",
            "state-value",
            "challenge-value",
        )
        .expect("url");
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(
            pairs.get("prompt").map(String::as_str),
            Some("select_account")
        );
        assert_eq!(
            pairs.get("redirect_uri").map(String::as_str),
            Some("http://localhost:45871")
        );
        assert_eq!(
            pairs.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        assert_eq!(pairs.get("state").map(String::as_str), Some("state-value"));
        assert_eq!(
            pairs.get("scope").map(String::as_str),
            Some("XboxLive.signin offline_access")
        );
    }

    #[test]
    fn parses_oauth_callback_code() {
        assert_eq!(
            parse_oauth_callback("/?code=abc123&state=expected", "expected").expect("code"),
            "abc123"
        );
    }

    #[test]
    fn rejects_oauth_callback_with_wrong_state() {
        assert!(parse_oauth_callback("/?code=abc123&state=other", "expected").is_err());
    }

    #[test]
    fn reports_oauth_callback_errors() {
        let error = parse_oauth_callback("/?error=access_denied&state=expected", "expected")
            .expect_err("error");
        assert_eq!(error.to_string(), "Connexion annulée dans le navigateur");
    }

    #[test]
    fn splits_secret_into_bounded_chunks() {
        let secret = "a".repeat(2500);
        let chunks = split_secret(&secret);
        assert_eq!(chunks.len(), 3);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 1000));
        assert_eq!(join_secret(&chunks), secret);
    }

    #[test]
    fn rejoins_secret_chunks() {
        let chunks = vec!["{\"refresh_token\":\"ab".to_string(), "cd\"}".to_string()];
        assert_eq!(join_secret(&chunks), "{\"refresh_token\":\"abcd\"}");
    }

    #[test]
    fn upgrades_texture_urls_to_https() {
        assert_eq!(
            secure_texture_url("http://textures.minecraft.net/texture/abc"),
            "https://textures.minecraft.net/texture/abc"
        );
        assert_eq!(
            secure_texture_url("https://textures.minecraft.net/texture/abc"),
            "https://textures.minecraft.net/texture/abc"
        );
    }

    #[test]
    #[ignore = "écrit temporairement dans le Coffre d’identification Windows"]
    fn round_trips_chunked_credentials_in_the_windows_vault() {
        let account_id = "xallauncher-selftest";
        let credentials = Credentials {
            refresh_token: "r".repeat(1500),
            minecraft_token: "m".repeat(1800),
            expires_at: Utc::now() + Duration::hours(1),
        };
        save_credentials(account_id, &credentials).expect("écriture du coffre");
        let restored = read_credentials(account_id).expect("lecture du coffre");
        assert_eq!(restored.refresh_token, credentials.refresh_token);
        assert_eq!(restored.minecraft_token, credentials.minecraft_token);
        delete_credentials(account_id);
        assert!(read_secret(account_id).expect("coffre").is_none());
    }
}
