use crate::error::{AppError, Result};
use keyring::Entry;

pub(crate) const SERVICE: &str = "studio.xaltar.xallauncher";
const CURSEFORGE_ENTRY: &str = "curseforge-api-key";
const CURSEFORGE_ENV: &str = "XAL_CURSEFORGE_API_KEY";

fn curseforge_entry() -> Result<Entry> {
    Entry::new(SERVICE, CURSEFORGE_ENTRY)
        .map_err(|error| AppError::new(format!("Coffre d’identification Windows : {error}")))
}

pub(crate) fn curseforge_env_key() -> Option<String> {
    std::env::var(CURSEFORGE_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub(crate) fn curseforge_vault_key() -> Result<Option<String>> {
    match curseforge_entry()?.get_password() {
        Ok(value) if !value.trim().is_empty() => Ok(Some(value)),
        Ok(_) => Ok(None),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(AppError::new(format!(
            "Lecture de la clé CurseForge impossible : {error}"
        ))),
    }
}

pub(crate) fn curseforge_builtin_key() -> Option<String> {
    option_env!("XAL_CURSEFORGE_API_KEY")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn curseforge_key() -> Result<Option<String>> {
    if let Some(value) = curseforge_builtin_key() {
        return Ok(Some(value));
    }
    if let Some(value) = curseforge_env_key() {
        return Ok(Some(value));
    }
    curseforge_vault_key()
}

pub(crate) fn set_curseforge_key(key: &str) -> Result<()> {
    let key = key.trim();
    if key.len() < 10
        || key.len() > 512
        || !key.chars().all(|character| character.is_ascii_graphic())
    {
        return Err(AppError::new(
            "Clé API CurseForge invalide : vérifiez la valeur copiée depuis la console CurseForge",
        ));
    }
    curseforge_entry()?.set_password(key)?;
    Ok(())
}

pub(crate) fn clear_curseforge_key() -> Result<()> {
    match curseforge_entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(AppError::new(format!(
            "Suppression de la clé CurseForge impossible : {error}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::set_curseforge_key;

    #[test]
    fn rejects_empty_or_unsafe_keys() {
        assert!(set_curseforge_key("").is_err());
        assert!(set_curseforge_key("clé avec des espaces").is_err());
        assert!(set_curseforge_key("key\ninjection").is_err());
    }
}
