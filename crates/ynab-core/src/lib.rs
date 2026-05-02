#[cfg(test)]
use std::sync::{LazyLock, Mutex};

pub mod app;
pub mod config;
pub mod error;
pub mod models;
pub mod secrets;

pub use app::{
    AppState, OAuthAppInput, OAuthStartResult, ResolveByNameKind, ResourceListOptions,
    RuntimeOptions, TransactionCreateInput, TransactionListOptions, TransactionUpdateInput,
};
pub use config::{AppConfig, ConfigManager, OutputFormat, PendingOAuth, ProfileConfig};
pub use error::{CliErrorEnvelope, Result, YnabError};
pub use models::{
    AmountMilliunits, ApiSuccessEnvelope, OAuthScope, OutputEnvelope, SaveCategory, StoredSession,
    TransactionClearedFilter,
};

#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
