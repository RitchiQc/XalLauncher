use crate::error::{AppError, Result};

fn extract_executable(command: &str) -> Option<String> {
    let command = command.trim();
    if let Some(rest) = command.strip_prefix('"') {
        return rest.find('"').map(|end| rest[..end].to_string());
    }
    let lower = command.to_ascii_lowercase();
    lower
        .find(".exe")
        .map(|index| command[..index + 4].to_string())
}

fn private_flag(executable: &str) -> Option<&'static str> {
    let name = executable
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(executable)
        .to_ascii_lowercase();
    match name.as_str() {
        "msedge.exe" => Some("--inprivate"),
        "chrome.exe" | "brave.exe" | "vivaldi.exe" | "opera.exe" => Some("--incognito"),
        "firefox.exe" => Some("-private-window"),
        _ => None,
    }
}

#[cfg(windows)]
fn default_browser_executable() -> Option<String> {
    use winreg::enums::{HKEY_CLASSES_ROOT, HKEY_CURRENT_USER};
    use winreg::RegKey;

    let choice = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(
            r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https\UserChoice",
        )
        .ok()?;
    let prog_id: String = choice.get_value("ProgId").ok()?;
    let command = RegKey::predef(HKEY_CLASSES_ROOT)
        .open_subkey(format!(r"{prog_id}\shell\open\command"))
        .ok()?;
    let value: String = command.get_value("").ok()?;
    extract_executable(&value)
}

#[tauri::command]
pub fn open_url_private(app: tauri::AppHandle, url: String) -> Result<bool> {
    #[cfg(windows)]
    if let Some(executable) = default_browser_executable() {
        if let Some(flag) = private_flag(&executable) {
            if std::process::Command::new(&executable)
                .arg(flag)
                .arg(&url)
                .spawn()
                .is_ok()
            {
                return Ok(true);
            }
        }
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|error| AppError::new(format!("Ouverture du navigateur impossible : {error}")))?;
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::{extract_executable, private_flag};

    #[test]
    fn maps_browsers_to_private_flags() {
        assert_eq!(
            private_flag(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
            Some("--inprivate")
        );
        assert_eq!(private_flag("chrome.exe"), Some("--incognito"));
        assert_eq!(private_flag("firefox.exe"), Some("-private-window"));
        assert_eq!(private_flag("unknown.exe"), None);
    }

    #[test]
    fn extracts_quoted_executable_from_registry_command() {
        assert_eq!(
            extract_executable(
                "\"C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe\" --single-argument %1"
            ),
            Some("C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe".to_string())
        );
    }
}
