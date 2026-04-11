use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    error::{Result, YnabError},
    models::OAuthScope,
};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root_dir: PathBuf,
    pub config_file: PathBuf,
    pub secrets_file: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default = "default_profile_name")]
    pub current_profile: String,
    #[serde(default)]
    pub profiles: BTreeMap<String, ProfileConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub default_plan_id: Option<String>,
    #[serde(default)]
    pub oauth_app: Option<OAuthAppConfig>,
    #[serde(default)]
    pub pending_oauth: Option<PendingOAuth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthAppConfig {
    pub client_id: String,
    pub redirect_uri: String,
    #[serde(default)]
    pub scope: OAuthScope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingOAuth {
    pub state: String,
    pub code_verifier: String,
    pub authorize_url: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    #[default]
    Json,
    PrettyJson,
}

pub struct ConfigManager {
    paths: AppPaths,
    config: AppConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut profiles = BTreeMap::new();
        profiles.insert(default_profile_name(), ProfileConfig::default());

        Self {
            version: default_version(),
            current_profile: default_profile_name(),
            profiles,
        }
    }
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            default_plan_id: None,
            oauth_app: None,
            pending_oauth: None,
        }
    }
}

impl ConfigManager {
    pub fn load() -> Result<Self> {
        let paths = AppPaths::discover()?;
        if !paths.root_dir.exists() {
            fs::create_dir_all(&paths.root_dir)?;
        }

        let config = if paths.config_file.exists() {
            let raw = fs::read_to_string(&paths.config_file)?;
            serde_json::from_str::<AppConfig>(&raw)?
        } else {
            AppConfig::default()
        };

        let mut manager = Self { paths, config };
        let current = manager.config.current_profile.clone();
        manager.ensure_profile_mut(&current);
        manager.save()?;
        Ok(manager)
    }

    pub fn save(&self) -> Result<()> {
        if !self.paths.root_dir.exists() {
            fs::create_dir_all(&self.paths.root_dir)?;
        }
        let raw = serde_json::to_string_pretty(&self.config)?;
        fs::write(&self.paths.config_file, raw)?;
        Ok(())
    }

    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    pub fn current_profile_name(&self) -> &str {
        &self.config.current_profile
    }

    pub fn set_current_profile(&mut self, name: &str) {
        self.config.current_profile = name.to_string();
        self.ensure_profile_mut(name);
    }

    pub fn profile(&self, name: &str) -> Option<&ProfileConfig> {
        self.config.profiles.get(name)
    }

    pub fn ensure_profile_mut(&mut self, name: &str) -> &mut ProfileConfig {
        self.config.profiles.entry(name.to_string()).or_default()
    }

    pub fn profile_mut(&mut self, name: &str) -> Result<&mut ProfileConfig> {
        self.config
            .profiles
            .get_mut(name)
            .ok_or_else(|| YnabError::Config(format!("profile not found: {name}")))
    }
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        if let Ok(root) = std::env::var("YNAB_AGENT_CLI_HOME") {
            let root_dir = PathBuf::from(root);
            return Ok(Self {
                config_file: root_dir.join("config.json"),
                secrets_file: root_dir.join("secrets.json"),
                root_dir,
            });
        }

        let root_dir = default_runtime_home()?;
        Ok(Self {
            config_file: root_dir.join("config.json"),
            secrets_file: root_dir.join("secrets.json"),
            root_dir,
        })
    }
}

impl PendingOAuth {
    pub fn is_recent(&self) -> bool {
        let age = Utc::now() - self.created_at;
        age.num_hours() < 1
    }
}

fn default_version() -> u32 {
    1
}

fn default_profile_name() -> String {
    "default".to_string()
}

fn default_base_url() -> String {
    "https://api.ynab.com/v1".to_string()
}

fn default_runtime_home() -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| YnabError::Config("unable to determine home directory".to_string()))?;
    Ok(home.join(".ynab-agent-cli"))
}

#[allow(dead_code)]
fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::ConfigManager;

    #[test]
    fn loads_default_config_in_custom_home() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        unsafe {
            std::env::set_var("YNAB_AGENT_CLI_HOME", temp_dir.path());
        }
        let manager = ConfigManager::load().unwrap();
        assert_eq!(manager.current_profile_name(), "default");
        assert!(fs::exists(temp_dir.path().join("config.json")).unwrap());
        unsafe {
            std::env::remove_var("YNAB_AGENT_CLI_HOME");
        }
    }
}
