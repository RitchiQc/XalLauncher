use crate::error::{AppError, Result, ResultExt};
use crate::models::{Account, AppSettings, BackupEntry, Instance, ServerEntry};
use directories::ProjectDirs;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn new() -> Result<Self> {
        let project = ProjectDirs::from("studio", "Xaltar", "XalLauncher").ok_or_else(|| {
            AppError::new("Impossible de déterminer le dossier des données utilisateur")
        })?;
        let root = project.data_local_dir().to_path_buf();
        for path in [
            root.clone(),
            root.join("instances"),
            root.join("versions"),
            root.join("libraries"),
            root.join("assets"),
            root.join("runtimes"),
            root.join("cache"),
            root.join("logs"),
            root.join("backups"),
        ] {
            fs::create_dir_all(path)?;
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn instance_dir(&self, id: &str) -> PathBuf {
        self.root.join("instances").join(id)
    }
    pub fn game_dir(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("game")
    }
    pub fn backups_dir(&self) -> PathBuf {
        self.root.join("backups")
    }

    fn read<T: DeserializeOwned>(&self, name: &str, default: T) -> Result<T> {
        let path = self.root.join(name);
        if !path.exists() {
            return Ok(default);
        }
        let content =
            fs::read_to_string(&path).context(format!("Lecture de {}", path.display()))?;
        serde_json::from_str(&content).context(format!("Format invalide dans {}", path.display()))
    }

    fn write<T: Serialize + ?Sized>(&self, name: &str, value: &T) -> Result<()> {
        let path = self.root.join(name);
        let content = serde_json::to_vec_pretty(value)?;
        let parent = path
            .parent()
            .ok_or_else(|| AppError::new("Dossier de stockage invalide"))?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(&content)?;
        temp.persist(&path).map_err(|error| {
            AppError::new(format!(
                "Remplacement de {} : {}",
                path.display(),
                error.error
            ))
        })?;
        Ok(())
    }

    pub fn settings(&self) -> Result<AppSettings> {
        let mut settings: AppSettings = self.read("settings.json", AppSettings::default())?;
        if settings.microsoft_client_id.is_empty() {
            settings.microsoft_client_id = AppSettings::default().microsoft_client_id;
        }
        Ok(settings)
    }
    pub fn save_settings(&self, value: &AppSettings) -> Result<()> {
        self.write("settings.json", value)
    }
    pub fn accounts(&self) -> Result<Vec<Account>> {
        self.read("accounts.json", Vec::new())
    }
    pub fn save_accounts(&self, value: &[Account]) -> Result<()> {
        self.write("accounts.json", value)
    }
    pub fn instances(&self) -> Result<Vec<Instance>> {
        let mut instances: Vec<Instance> = self.read("instances.json", Vec::new())?;
        instances.sort_by(|a, b| {
            b.last_played_at
                .cmp(&a.last_played_at)
                .then(a.name.cmp(&b.name))
        });
        Ok(instances)
    }
    pub fn save_instances(&self, value: &[Instance]) -> Result<()> {
        self.write("instances.json", value)
    }
    pub fn instance(&self, id: &str) -> Result<Instance> {
        self.instances()?
            .into_iter()
            .find(|item| item.id == id)
            .ok_or_else(|| AppError::new("Instance introuvable"))
    }
    pub fn servers(&self) -> Result<Vec<ServerEntry>> {
        self.read("servers.json", Vec::new())
    }
    pub fn save_servers(&self, value: &[ServerEntry]) -> Result<()> {
        self.write("servers.json", value)
    }
    pub fn backups(&self) -> Result<Vec<BackupEntry>> {
        self.read("backups.json", Vec::new())
    }
    pub fn save_backups(&self, value: &[BackupEntry]) -> Result<()> {
        self.write("backups.json", value)
    }
}

#[cfg(test)]
impl Store {
    pub(crate) fn temporary(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }
}

#[tauri::command]
pub fn get_settings(state: tauri::State<'_, crate::AppState>) -> Result<AppSettings> {
    state.store.settings()
}

#[tauri::command]
pub fn update_settings(
    settings: AppSettings,
    state: tauri::State<'_, crate::AppState>,
) -> Result<AppSettings> {
    if settings.download_concurrency == 0 || settings.download_concurrency > 32 {
        return Err(AppError::new(
            "Le nombre de téléchargements simultanés doit être entre 1 et 32",
        ));
    }
    if !matches!(settings.theme.as_str(), "dark" | "light" | "space")
        || !matches!(settings.language.as_str(), "fr" | "en")
    {
        return Err(AppError::new("Paramètres d’apparence invalides"));
    }
    state.store.save_settings(&settings)?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::Store;
    use crate::models::AppSettings;

    #[test]
    fn replaces_existing_settings() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store {
            root: directory.path().to_path_buf(),
        };
        let mut settings = AppSettings::default();
        store.save_settings(&settings).unwrap();
        settings.download_concurrency = 12;
        store.save_settings(&settings).unwrap();
        assert_eq!(store.settings().unwrap().download_concurrency, 12);
    }
}
