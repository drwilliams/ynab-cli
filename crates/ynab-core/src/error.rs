use serde::Serialize;
use serde_json::{Value, json};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, YnabError>;

#[derive(Debug, Error)]
pub enum YnabError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("missing authentication. set a personal access token or complete OAuth setup")]
    MissingCredentials,
    #[error("resource resolution failed for {resource}: {name}")]
    ResourceResolution {
        resource: &'static str,
        name: String,
        matches: Vec<String>,
    },
    #[error("invalid amount: {0}")]
    InvalidAmount(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("URL error: {0}")]
    Url(#[from] url::ParseError),
    #[error("keyring error: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("browser open failed: {0}")]
    Browser(String),
    #[error("API error {status}: {name} ({detail})")]
    Api {
        status: u16,
        id: String,
        name: String,
        detail: String,
        body: Value,
    },
}

#[derive(Debug, Serialize)]
pub struct CliErrorEnvelope {
    pub ok: bool,
    pub error: CliErrorBody,
}

#[derive(Debug, Serialize)]
pub struct CliErrorBody {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl YnabError {
    pub fn to_cli_envelope(&self) -> CliErrorEnvelope {
        let (code, status, details) = match self {
            Self::Config(_) => ("config_error".to_string(), None, None),
            Self::MissingCredentials => ("missing_credentials".to_string(), Some(401), None),
            Self::ResourceResolution {
                resource,
                name,
                matches,
            } => (
                "resource_resolution_error".to_string(),
                Some(400),
                Some(json!({
                    "resource": resource,
                    "name": name,
                    "matches": matches,
                })),
            ),
            Self::InvalidAmount(_) => ("invalid_amount".to_string(), Some(400), None),
            Self::Io(_) => ("io_error".to_string(), None, None),
            Self::Http(_) => ("http_error".to_string(), None, None),
            Self::Json(_) => ("json_error".to_string(), None, None),
            Self::Url(_) => ("url_error".to_string(), None, None),
            Self::Keyring(_) => ("keyring_error".to_string(), None, None),
            Self::Browser(_) => ("browser_error".to_string(), None, None),
            Self::Api {
                status,
                id,
                name,
                detail,
                body,
            } => (
                format!("api_{id}"),
                Some(*status),
                Some(json!({
                    "api_name": name,
                    "api_detail": detail,
                    "raw": body,
                })),
            ),
        };

        CliErrorEnvelope {
            ok: false,
            error: CliErrorBody {
                code,
                message: self.to_string(),
                status,
                details,
            },
        }
    }
}
