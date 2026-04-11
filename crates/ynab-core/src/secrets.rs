use std::{collections::BTreeMap, fs};

use serde::{Deserialize, Serialize};

use crate::{
    config::AppPaths,
    error::{Result, YnabError},
    models::StoredSession,
};

const SERVICE_NAME: &str = "com.openai.ynab-agent-cli";

#[derive(Debug, Clone, Copy)]
pub enum SecretBackend {
    Keyring,
    File,
}

pub struct SecretStore {
    backend: SecretBackend,
    paths: AppPaths,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct FileSecrets {
    #[serde(default)]
    profiles: BTreeMap<String, ProfileSecrets>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ProfileSecrets {
    session: Option<StoredSession>,
    oauth_client_secret: Option<String>,
}

impl SecretStore {
    pub fn new(paths: AppPaths, use_keyring: bool) -> Self {
        let backend = if use_keyring {
            SecretBackend::Keyring
        } else {
            SecretBackend::File
        };
        Self { backend, paths }
    }

    pub fn load_session(&self, profile: &str) -> Result<Option<StoredSession>> {
        match self.backend {
            SecretBackend::Keyring => self.read_keyring(profile, "session"),
            SecretBackend::File => self.read_file_secrets(profile, |entry| entry.session.clone()),
        }
    }

    pub fn save_session(&self, profile: &str, session: &StoredSession) -> Result<()> {
        match self.backend {
            SecretBackend::Keyring => self.write_keyring_verified(profile, "session", session),
            SecretBackend::File => self.write_file_secrets(profile, |entry| {
                entry.session = Some(session.clone());
            }),
        }
    }

    pub fn clear_session(&self, profile: &str) -> Result<()> {
        match self.backend {
            SecretBackend::Keyring => self.delete_keyring(profile, "session"),
            SecretBackend::File => self.write_file_secrets(profile, |entry| {
                entry.session = None;
            }),
        }
    }

    pub fn load_oauth_client_secret(&self, profile: &str) -> Result<Option<String>> {
        match self.backend {
            SecretBackend::Keyring => self.read_keyring(profile, "oauth_client_secret"),
            SecretBackend::File => {
                self.read_file_secrets(profile, |entry| entry.oauth_client_secret.clone())
            }
        }
    }

    pub fn save_oauth_client_secret(&self, profile: &str, secret: &str) -> Result<()> {
        match self.backend {
            SecretBackend::Keyring => {
                self.write_keyring_verified(profile, "oauth_client_secret", &secret.to_string())
            }
            SecretBackend::File => self.write_file_secrets(profile, |entry| {
                entry.oauth_client_secret = Some(secret.to_string());
            }),
        }
    }

    fn read_keyring<T>(&self, profile: &str, suffix: &str) -> Result<Option<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        let entry = keyring::Entry::new(SERVICE_NAME, &format!("{profile}:{suffix}"))?;
        match entry.get_password() {
            Ok(raw) => Ok(Some(serde_json::from_str(&raw)?)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn write_keyring_verified<T>(&self, profile: &str, suffix: &str, value: &T) -> Result<()>
    where
        T: Serialize + ?Sized,
    {
        let entry = keyring::Entry::new(SERVICE_NAME, &format!("{profile}:{suffix}"))?;
        let expected = serde_json::to_string(value)?;
        entry.set_password(&expected)?;
        let actual = entry.get_password()?;
        if actual != expected {
            return Err(YnabError::Config(format!(
                "keyring write verification failed for {profile}:{suffix}"
            )));
        }
        Ok(())
    }

    fn delete_keyring(&self, profile: &str, suffix: &str) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE_NAME, &format!("{profile}:{suffix}"))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn read_file_secrets<T>(
        &self,
        profile: &str,
        mapper: impl FnOnce(&ProfileSecrets) -> Option<T>,
    ) -> Result<Option<T>> {
        let secrets = self.load_file_secrets()?;
        Ok(secrets.profiles.get(profile).and_then(mapper))
    }

    fn write_file_secrets(
        &self,
        profile: &str,
        mut updater: impl FnMut(&mut ProfileSecrets),
    ) -> Result<()> {
        let mut secrets = self.load_file_secrets()?;
        let entry = secrets.profiles.entry(profile.to_string()).or_default();
        updater(entry);
        self.save_file_secrets(&secrets)
    }

    fn load_file_secrets(&self) -> Result<FileSecrets> {
        if !self.paths.root_dir.exists() {
            fs::create_dir_all(&self.paths.root_dir)?;
        }
        if !self.paths.secrets_file.exists() {
            return Ok(FileSecrets::default());
        }
        let raw = fs::read_to_string(&self.paths.secrets_file)?;
        Ok(serde_json::from_str(&raw)?)
    }

    fn save_file_secrets(&self, secrets: &FileSecrets) -> Result<()> {
        if !self.paths.root_dir.exists() {
            fs::create_dir_all(&self.paths.root_dir)?;
        }
        let raw = serde_json::to_string_pretty(secrets)?;
        fs::write(&self.paths.secrets_file, raw)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = fs::Permissions::from_mode(0o600);
            fs::set_permissions(&self.paths.secrets_file, permissions)?;
        }
        Ok(())
    }
}
