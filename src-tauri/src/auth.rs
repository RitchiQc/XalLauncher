use crate::error::{AppError, Result, ResultExt};
use crate::models::{Account, Cape, DeviceCodeEvent};
use chrono::{Duration, Utc};
use keyring::Entry;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

const SERVICE: &str = "studio.xaltar.xallauncher";
const DEVICE_CODE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

async fn ensure_auth_success(
    response: reqwest::Response,
    action: &str,
) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    Err(AppError::new(format!("{action} ({})", response.status())))
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

fn save_credentials(account_id: &str, credentials: &Credentials) -> Result<()> {
    credential_entry(account_id)?.set_password(&serde_json::to_string(credentials)?)?;
    Ok(())
}

fn read_credentials(account_id: &str) -> Result<Credentials> {
    let value = credential_entry(account_id)?.get_password()?;
    Ok(serde_json::from_str(&value)?)
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
    let _ = credential_entry(&account_id)?.delete_credential();
    Ok(accounts)
}

#[tauri::command]
pub async fn begin_microsoft_login(
    client_id: String,
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
) -> Result<Account> {
    if client_id.trim().is_empty() {
        return Err(AppError::new(
            "Renseignez d’abord l’identifiant de l’application Microsoft Entra dans les paramètres",
        ));
    }
    let device = state
        .client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode")
        .form(&[
            ("client_id", client_id.as_str()),
            ("scope", "XboxLive.signin offline_access"),
        ])
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

    let token = loop {
        tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
        let response = state
            .client
            .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
            .form(&[
                ("grant_type", DEVICE_CODE_GRANT),
                ("client_id", client_id.as_str()),
                ("device_code", device_code.as_str()),
            ])
            .send()
            .await?;
        let status = response.status();
        let body: Value = response.json().await?;
        if status.is_success() {
            break serde_json::from_value::<TokenResponse>(body)?;
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
    };
    let (account, credentials) = minecraft_account(&state.client, token).await?;
    let mut accounts = state.store.accounts()?;
    accounts.retain(|item| item.id != account.id);
    accounts.insert(0, account.clone());
    state.store.save_accounts(&accounts)?;
    save_credentials(&account.id, &credentials)?;
    Ok(account)
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
            "Ce compte ne possède pas Minecraft: Java Edition",
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
    let skin_url = profile["skins"]
        .as_array()
        .and_then(|skins| {
            skins
                .iter()
                .find(|skin| skin["state"] == "ACTIVE")
                .or_else(|| skins.first())
        })
        .and_then(|skin| skin["url"].as_str())
        .map(str::to_string);
    let capes = profile["capes"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value::<Cape>(item.clone()).ok())
                .collect()
        })
        .unwrap_or_default();
    Ok(Account {
        id: required_string(&profile, "id")?,
        name: required_string(&profile, "name")?,
        skin_url,
        capes,
        expires_at: Utc::now(),
    })
}

pub async fn valid_token(state: &crate::AppState, account_id: &str) -> Result<(Account, String)> {
    let account = state
        .store
        .accounts()?
        .into_iter()
        .find(|item| item.id == account_id)
        .ok_or_else(|| AppError::new("Compte Minecraft introuvable"))?;
    let credentials = read_credentials(account_id)
        .map_err(|_| AppError::new("Session absente : reconnectez ce compte Microsoft"))?;
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
    use super::DEVICE_CODE_GRANT;

    #[test]
    fn uses_microsoft_device_code_grant() {
        assert_eq!(
            DEVICE_CODE_GRANT,
            "urn:ietf:params:oauth:grant-type:device_code"
        );
    }
}
