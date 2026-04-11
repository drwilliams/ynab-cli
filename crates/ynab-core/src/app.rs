use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration as ChronoDuration, Utc};
use rand::{RngCore, rngs::OsRng};
use reqwest::{Method, StatusCode};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use url::Url;

use crate::{
    AmountMilliunits, OAuthScope,
    config::{ConfigManager, OAuthAppConfig, OutputFormat, PendingOAuth},
    error::{Result, YnabError},
    models::{
        AccountsData, ApiErrorResponse, ApiResponse, CategoryGroupsData, NamedResource,
        OutputEnvelope, PayeesData, PlansData, SaveTransaction, SaveTransactionRequest,
        StoredSession, UpdateTransaction, UpdateTransactionRequest,
    },
    secrets::SecretStore,
};

#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    pub profile: Option<String>,
    pub use_keyring: bool,
    pub base_url_override: Option<String>,
    pub output_format: OutputFormat,
}

#[derive(Debug, Clone)]
pub struct OAuthAppInput {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub scope: OAuthScope,
}

#[derive(Debug, Clone, Serialize)]
pub struct OAuthStartResult {
    pub authorize_url: String,
    pub state: String,
    pub profile: String,
    pub code_challenge_method: &'static str,
    pub scope: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum ResolveByNameKind {
    Plan,
    Account,
    Category,
    Payee,
}

#[derive(Debug, Clone, Default)]
pub struct ResourceListOptions {
    pub last_knowledge_of_server: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct TransactionCreateInput {
    pub plan_id: String,
    pub account_id: String,
    pub date: String,
    pub amount: AmountMilliunits,
    pub payee_id: Option<String>,
    pub payee_name: Option<String>,
    pub category_id: Option<String>,
    pub memo: Option<String>,
    pub cleared: Option<String>,
    pub approved: Option<bool>,
    pub flag_color: Option<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TransactionUpdateInput {
    pub plan_id: String,
    pub transaction_id: String,
    pub account_id: Option<String>,
    pub date: Option<String>,
    pub amount: Option<AmountMilliunits>,
    pub payee_id: Option<String>,
    pub payee_name: Option<String>,
    pub category_id: Option<String>,
    pub memo: Option<String>,
    pub cleared: Option<String>,
    pub approved: Option<bool>,
    pub flag_color: Option<String>,
    pub dry_run: bool,
}

pub struct AppState {
    config: ConfigManager,
    secrets: SecretStore,
    profile_name: String,
    base_url: Url,
    output_format: OutputFormat,
    http: reqwest::Client,
}

impl AppState {
    pub fn load(options: RuntimeOptions) -> Result<Self> {
        let mut config = ConfigManager::load()?;
        let profile_name = options
            .profile
            .unwrap_or_else(|| config.current_profile_name().to_string());
        config.set_current_profile(&profile_name);
        config.save()?;

        let profile = config
            .profile(&profile_name)
            .cloned()
            .ok_or_else(|| YnabError::Config(format!("profile not found: {profile_name}")))?;
        let base_url = normalize_base_url(
            options
                .base_url_override
                .as_deref()
                .unwrap_or(&profile.base_url),
        )?;
        let secrets = SecretStore::new(config.paths().clone(), options.use_keyring);
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self {
            config,
            secrets,
            profile_name,
            base_url,
            output_format: options.output_format,
            http,
        })
    }

    pub fn output_format(&self) -> OutputFormat {
        self.output_format
    }

    pub fn profile_name(&self) -> &str {
        &self.profile_name
    }

    pub fn oauth_redirect_uri(&self) -> Result<String> {
        let profile = self
            .config
            .profile(&self.profile_name)
            .ok_or_else(|| YnabError::Config("profile missing".to_string()))?;
        let oauth_app = profile.oauth_app.as_ref().ok_or_else(|| {
            YnabError::Config("OAuth app is not configured for this profile".to_string())
        })?;
        Ok(oauth_app.redirect_uri.clone())
    }

    pub fn set_personal_access_token(&mut self, token: String) -> Result<OutputEnvelope> {
        self.secrets.save_session(
            &self.profile_name,
            &StoredSession::PersonalAccessToken {
                access_token: token,
            },
        )?;
        Ok(Self::ok(json!({
            "profile": self.profile_name,
            "auth_kind": "personal_access_token"
        })))
    }

    pub fn clear_session(&mut self) -> Result<OutputEnvelope> {
        self.secrets.clear_session(&self.profile_name)?;
        Ok(Self::ok(json!({
            "profile": self.profile_name,
            "cleared": true
        })))
    }

    pub fn configure_oauth_app(&mut self, input: OAuthAppInput) -> Result<OutputEnvelope> {
        self.secrets
            .save_oauth_client_secret(&self.profile_name, &input.client_secret)?;
        let profile = self.config.profile_mut(&self.profile_name)?;
        profile.oauth_app = Some(OAuthAppConfig {
            client_id: input.client_id,
            redirect_uri: input.redirect_uri,
            scope: input.scope,
        });
        self.config.save()?;
        Ok(Self::ok(json!({
            "profile": self.profile_name,
            "oauth_app_configured": true
        })))
    }

    pub fn start_oauth_flow(&mut self) -> Result<OAuthStartResult> {
        let profile = self
            .config
            .profile(&self.profile_name)
            .cloned()
            .ok_or_else(|| YnabError::Config("profile missing".to_string()))?;
        let oauth_app = profile.oauth_app.ok_or_else(|| {
            YnabError::Config("OAuth app is not configured for this profile".to_string())
        })?;
        self.secrets
            .load_oauth_client_secret(&self.profile_name)?
            .ok_or_else(|| YnabError::Config("OAuth client secret is missing".to_string()))?;

        let state = random_url_safe_bytes(24);
        let code_verifier = random_url_safe_bytes(48);
        let code_challenge = code_challenge(&code_verifier);

        let mut authorize_url = Url::parse("https://app.ynab.com/oauth/authorize")?;
        {
            let mut pairs = authorize_url.query_pairs_mut();
            pairs.append_pair("client_id", &oauth_app.client_id);
            pairs.append_pair("redirect_uri", &oauth_app.redirect_uri);
            pairs.append_pair("response_type", "code");
            pairs.append_pair("state", &state);
            pairs.append_pair("code_challenge", &code_challenge);
            pairs.append_pair("code_challenge_method", "S256");
            if let Some(scope) = oauth_app.scope.as_api_scope() {
                pairs.append_pair("scope", scope);
            }
        }

        let authorize_url_string = authorize_url.to_string();
        let profile_mut = self.config.profile_mut(&self.profile_name)?;
        profile_mut.pending_oauth = Some(PendingOAuth {
            state: state.clone(),
            code_verifier,
            authorize_url: authorize_url_string.clone(),
            created_at: Utc::now(),
        });
        self.config.save()?;

        Ok(OAuthStartResult {
            authorize_url: authorize_url_string,
            state,
            profile: self.profile_name.clone(),
            code_challenge_method: "S256",
            scope: oauth_app.scope.as_api_scope().map(str::to_string),
        })
    }

    pub fn start_oauth(&mut self, open_browser: bool) -> Result<OutputEnvelope> {
        let result = self.start_oauth_flow()?;

        if open_browser {
            webbrowser::open(&result.authorize_url)
                .map_err(|error| YnabError::Browser(error.to_string()))?;
        }

        Ok(Self::ok(serde_json::to_value(result)?))
    }

    pub async fn exchange_oauth_code(
        &mut self,
        code: &str,
        state: Option<&str>,
    ) -> Result<OutputEnvelope> {
        let (oauth_app, client_secret, pending) = self.oauth_exchange_prereqs()?;
        if !pending.is_recent() {
            return Err(YnabError::Config(
                "pending OAuth state is stale; run `auth oauth start` again".to_string(),
            ));
        }
        if let Some(expected_state) = state
            && expected_state != pending.state
        {
            return Err(YnabError::Config("OAuth state mismatch".to_string()));
        }

        let response = self
            .http
            .post("https://app.ynab.com/oauth/token")
            .form(&[
                ("client_id", oauth_app.client_id.as_str()),
                ("client_secret", client_secret.as_str()),
                ("redirect_uri", oauth_app.redirect_uri.as_str()),
                ("grant_type", "authorization_code"),
                ("code", code),
                ("code_verifier", pending.code_verifier.as_str()),
            ])
            .send()
            .await?;

        let payload = self.parse_response(response).await?;
        let access_token = required_string(&payload, "access_token")?;
        let refresh_token = required_string(&payload, "refresh_token")?;
        let token_type = required_string(&payload, "token_type")?;
        let expires_in = payload
            .get("expires_in")
            .and_then(Value::as_i64)
            .ok_or_else(|| YnabError::Config("missing expires_in in token response".to_string()))?;
        let scope = payload
            .get("scope")
            .and_then(Value::as_str)
            .map(str::to_string);

        let session = StoredSession::OAuth {
            access_token,
            refresh_token,
            expires_at: Utc::now() + ChronoDuration::seconds(expires_in),
            token_type,
            scope,
            client_id: oauth_app.client_id,
            client_secret,
            redirect_uri: oauth_app.redirect_uri,
        };
        self.secrets.save_session(&self.profile_name, &session)?;
        let profile_mut = self.config.profile_mut(&self.profile_name)?;
        profile_mut.pending_oauth = None;
        self.config.save()?;

        Ok(Self::ok(json!({
            "profile": self.profile_name,
            "auth_kind": "oauth",
            "expires_at": match &session {
                StoredSession::OAuth { expires_at, .. } => expires_at.to_rfc3339(),
                _ => unreachable!(),
            }
        })))
    }

    pub async fn whoami(&mut self) -> Result<OutputEnvelope> {
        let plans = self.list_plans(ResourceListOptions::default()).await?;
        let session = self
            .secrets
            .load_session(&self.profile_name)?
            .ok_or(YnabError::MissingCredentials)?;
        Ok(Self::ok(json!({
            "profile": self.profile_name,
            "auth_kind": if session.is_oauth() { "oauth" } else { "personal_access_token" },
            "plans": plans.data.get("plans").cloned().unwrap_or_else(|| json!([]))
        })))
    }

    pub async fn list_plans(&mut self, options: ResourceListOptions) -> Result<OutputEnvelope> {
        let data = self
            .request_data(Method::GET, "/plans", (), options.last_knowledge_of_server)
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn get_plan(&mut self, plan_id: &str) -> Result<OutputEnvelope> {
        let data = self
            .request_data(Method::GET, &format!("/plans/{plan_id}"), (), None)
            .await?;
        Ok(Self::ok(data))
    }

    pub fn set_default_plan(&mut self, plan_id: &str) -> Result<OutputEnvelope> {
        let profile = self.config.profile_mut(&self.profile_name)?;
        profile.default_plan_id = Some(plan_id.to_string());
        self.config.save()?;
        Ok(Self::ok(json!({
            "profile": self.profile_name,
            "default_plan_id": plan_id
        })))
    }

    pub async fn list_accounts(
        &mut self,
        plan_id: &str,
        options: ResourceListOptions,
    ) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::GET,
                &format!("/plans/{plan_id}/accounts"),
                (),
                options.last_knowledge_of_server,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn list_categories(
        &mut self,
        plan_id: &str,
        options: ResourceListOptions,
    ) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::GET,
                &format!("/plans/{plan_id}/categories"),
                (),
                options.last_knowledge_of_server,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn list_payees(
        &mut self,
        plan_id: &str,
        options: ResourceListOptions,
    ) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::GET,
                &format!("/plans/{plan_id}/payees"),
                (),
                options.last_knowledge_of_server,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn list_transactions(
        &mut self,
        plan_id: &str,
        options: ResourceListOptions,
    ) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::GET,
                &format!("/plans/{plan_id}/transactions"),
                (),
                options.last_knowledge_of_server,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn create_transaction(
        &mut self,
        input: TransactionCreateInput,
    ) -> Result<OutputEnvelope> {
        let request = SaveTransactionRequest {
            transaction: SaveTransaction {
                account_id: input.account_id,
                date: input.date,
                amount: input.amount.0,
                payee_id: input.payee_id,
                payee_name: input.payee_name,
                category_id: input.category_id,
                memo: input.memo,
                cleared: input.cleared,
                approved: input.approved,
                flag_color: input.flag_color,
            },
        };

        if input.dry_run {
            return Ok(Self::ok(json!({
                "dry_run": true,
                "request": request
            })));
        }

        let data = self
            .request_data(
                Method::POST,
                &format!("/plans/{}/transactions", input.plan_id),
                &request,
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn update_transaction(
        &mut self,
        input: TransactionUpdateInput,
    ) -> Result<OutputEnvelope> {
        let request = UpdateTransactionRequest {
            transaction: Some(UpdateTransaction {
                account_id: input.account_id,
                date: input.date,
                amount: input.amount.map(|value| value.0),
                payee_id: input.payee_id,
                payee_name: input.payee_name,
                category_id: input.category_id,
                memo: input.memo,
                cleared: input.cleared,
                approved: input.approved,
                flag_color: input.flag_color,
            }),
        };

        if input.dry_run {
            return Ok(Self::ok(json!({
                "dry_run": true,
                "request": request
            })));
        }

        let data = self
            .request_data(
                Method::PATCH,
                &format!(
                    "/plans/{}/transactions/{}",
                    input.plan_id, input.transaction_id
                ),
                &request,
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub fn resolve_plan_argument(&self, provided: Option<String>) -> Result<String> {
        if let Some(plan_id) = provided {
            return Ok(plan_id);
        }
        let profile = self
            .config
            .profile(&self.profile_name)
            .ok_or_else(|| YnabError::Config("profile not found".to_string()))?;
        profile.default_plan_id.clone().ok_or_else(|| {
            YnabError::Config(
                "missing plan id. pass --plan or set a default with `plans set-default <id>`"
                    .to_string(),
            )
        })
    }

    pub async fn resolve_name(
        &mut self,
        kind: ResolveByNameKind,
        plan_id: Option<&str>,
        name: &str,
    ) -> Result<String> {
        let resources = match kind {
            ResolveByNameKind::Plan => {
                let response: ApiResponse<PlansData> =
                    self.request_typed(Method::GET, "/plans", (), None).await?;
                let mut list = response.data.plans;
                if let Some(default_plan) = response.data.default_plan {
                    list.push(default_plan);
                }
                list
            }
            ResolveByNameKind::Account => {
                let response: ApiResponse<AccountsData> = self
                    .request_typed(
                        Method::GET,
                        &format!("/plans/{}/accounts", plan_id.expect("plan required")),
                        (),
                        None,
                    )
                    .await?;
                response.data.accounts
            }
            ResolveByNameKind::Category => {
                let response: ApiResponse<CategoryGroupsData> = self
                    .request_typed(
                        Method::GET,
                        &format!("/plans/{}/categories", plan_id.expect("plan required")),
                        (),
                        None,
                    )
                    .await?;
                response
                    .data
                    .category_groups
                    .into_iter()
                    .flat_map(|group| group.categories)
                    .collect()
            }
            ResolveByNameKind::Payee => {
                let response: ApiResponse<PayeesData> = self
                    .request_typed(
                        Method::GET,
                        &format!("/plans/{}/payees", plan_id.expect("plan required")),
                        (),
                        None,
                    )
                    .await?;
                response.data.payees
            }
        };

        resolve_named_resource(kind, name, resources)
    }

    async fn request_data<B: Serialize>(
        &mut self,
        method: Method,
        path: &str,
        body: B,
        last_knowledge_of_server: Option<u64>,
    ) -> Result<Value> {
        let response: ApiResponse<Value> = self
            .request_typed(method, path, body, last_knowledge_of_server)
            .await?;
        Ok(response.data)
    }

    async fn request_typed<T: for<'de> serde::Deserialize<'de>, B: Serialize>(
        &mut self,
        method: Method,
        path: &str,
        body: B,
        last_knowledge_of_server: Option<u64>,
    ) -> Result<T> {
        let response = self
            .send_api_request(method, path, body, last_knowledge_of_server)
            .await?;
        Ok(serde_json::from_value(response)?)
    }

    async fn send_api_request<B: Serialize>(
        &mut self,
        method: Method,
        path: &str,
        body: B,
        last_knowledge_of_server: Option<u64>,
    ) -> Result<Value> {
        let mut refreshed = false;

        loop {
            let token = self.ensure_access_token(false).await?;
            let mut url = self.base_url.join(path.trim_start_matches('/'))?;
            if let Some(value) = last_knowledge_of_server {
                url.query_pairs_mut()
                    .append_pair("last_knowledge_of_server", &value.to_string());
            }

            let mut request = self.http.request(method.clone(), url).bearer_auth(token);
            if method != Method::GET {
                request = request.json(&body);
            }

            let response = request.send().await?;
            if response.status() == StatusCode::UNAUTHORIZED && !refreshed {
                let session = self
                    .secrets
                    .load_session(&self.profile_name)?
                    .ok_or(YnabError::MissingCredentials)?;
                if session.is_oauth() {
                    self.ensure_access_token(true).await?;
                    refreshed = true;
                    continue;
                }
            }

            return self.parse_response(response).await;
        }
    }

    async fn parse_response(&self, response: reqwest::Response) -> Result<Value> {
        let status = response.status();
        let body = response.json::<Value>().await?;
        if status.is_success() {
            return Ok(body);
        }

        if let Ok(error) = serde_json::from_value::<ApiErrorResponse>(body.clone()) {
            return Err(YnabError::Api {
                status: status.as_u16(),
                id: error.error.id,
                name: error.error.name,
                detail: error.error.detail,
                body,
            });
        }

        Err(YnabError::Api {
            status: status.as_u16(),
            id: status.as_u16().to_string(),
            name: "unknown_api_error".to_string(),
            detail: "API request failed".to_string(),
            body,
        })
    }

    async fn ensure_access_token(&mut self, force_refresh: bool) -> Result<String> {
        if let Ok(token) = std::env::var("YNAB_ACCESS_TOKEN") {
            return Ok(token);
        }

        let session = self
            .secrets
            .load_session(&self.profile_name)?
            .ok_or(YnabError::MissingCredentials)?;

        match session {
            StoredSession::PersonalAccessToken { access_token } => Ok(access_token),
            StoredSession::OAuth { .. } => {
                if force_refresh || session.needs_refresh() {
                    let refreshed = self.refresh_oauth_session(session).await?;
                    let token = refreshed.bearer_token().to_string();
                    self.secrets.save_session(&self.profile_name, &refreshed)?;
                    Ok(token)
                } else {
                    Ok(session.bearer_token().to_string())
                }
            }
        }
    }

    async fn refresh_oauth_session(&self, session: StoredSession) -> Result<StoredSession> {
        let StoredSession::OAuth {
            refresh_token,
            client_id,
            client_secret,
            redirect_uri,
            ..
        } = session
        else {
            return Ok(session);
        };

        let response = self
            .http
            .post("https://app.ynab.com/oauth/token")
            .form(&[
                ("client_id", client_id.as_str()),
                ("client_secret", client_secret.as_str()),
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token.as_str()),
            ])
            .send()
            .await?;
        let payload = self.parse_response(response).await?;
        let access_token = required_string(&payload, "access_token")?;
        let token_type = required_string(&payload, "token_type")?;
        let expires_in = payload
            .get("expires_in")
            .and_then(Value::as_i64)
            .ok_or_else(|| YnabError::Config("missing expires_in in token response".to_string()))?;
        let new_refresh = payload
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or(refresh_token);

        Ok(StoredSession::OAuth {
            access_token,
            refresh_token: new_refresh,
            expires_at: Utc::now() + ChronoDuration::seconds(expires_in),
            token_type,
            scope: payload
                .get("scope")
                .and_then(Value::as_str)
                .map(str::to_string),
            client_id,
            client_secret,
            redirect_uri,
        })
    }

    fn oauth_exchange_prereqs(&self) -> Result<(OAuthAppConfig, String, PendingOAuth)> {
        let profile = self
            .config
            .profile(&self.profile_name)
            .ok_or_else(|| YnabError::Config("profile missing".to_string()))?;
        let oauth_app = profile
            .oauth_app
            .clone()
            .ok_or_else(|| YnabError::Config("OAuth app is not configured".to_string()))?;
        let pending = profile.pending_oauth.clone().ok_or_else(|| {
            YnabError::Config("no pending OAuth flow. run `auth oauth start` first".to_string())
        })?;
        let client_secret = self
            .secrets
            .load_oauth_client_secret(&self.profile_name)?
            .ok_or_else(|| YnabError::Config("OAuth client secret missing".to_string()))?;
        Ok((oauth_app, client_secret, pending))
    }

    fn ok(data: Value) -> OutputEnvelope {
        OutputEnvelope { ok: true, data }
    }
}

fn resolve_named_resource(
    kind: ResolveByNameKind,
    name: &str,
    resources: Vec<NamedResource>,
) -> Result<String> {
    let target = name.trim().to_lowercase();
    let matches: Vec<NamedResource> = resources
        .into_iter()
        .filter(|resource| resource.name.trim().to_lowercase() == target)
        .collect();

    match matches.as_slice() {
        [single] => Ok(single.id.clone()),
        [] => Err(YnabError::ResourceResolution {
            resource: kind.as_str(),
            name: name.to_string(),
            matches: Vec::new(),
        }),
        many => Err(YnabError::ResourceResolution {
            resource: kind.as_str(),
            name: name.to_string(),
            matches: many.iter().map(|item| item.id.clone()).collect(),
        }),
    }
}

impl ResolveByNameKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Account => "account",
            Self::Category => "category",
            Self::Payee => "payee",
        }
    }
}

fn random_url_safe_bytes(length: usize) -> String {
    let mut bytes = vec![0u8; length];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn code_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

fn required_string(payload: &Value, field: &str) -> Result<String> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| YnabError::Config(format!("missing `{field}` in token response")))
}

fn normalize_base_url(input: &str) -> Result<Url> {
    let mut normalized = input.trim().to_string();
    if !normalized.ends_with('/') {
        normalized.push('/');
    }
    Ok(Url::parse(&normalized)?)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::{AppState, OAuthAppInput, RuntimeOptions, normalize_base_url};
    use crate::{OAuthScope, OutputFormat};

    #[test]
    fn start_oauth_builds_pkce_authorize_url() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        unsafe {
            std::env::set_var("YNAB_AGENT_CLI_HOME", temp_dir.path());
        }
        let mut app = AppState::load(RuntimeOptions {
            profile: None,
            use_keyring: false,
            base_url_override: None,
            output_format: OutputFormat::Json,
        })
        .unwrap();
        app.configure_oauth_app(OAuthAppInput {
            client_id: "client-id".to_string(),
            client_secret: "client-secret".to_string(),
            redirect_uri: "http://127.0.0.1:8765/callback".to_string(),
            scope: OAuthScope::ReadOnly,
        })
        .unwrap();

        let result = app.start_oauth(false).unwrap();
        let authorize_url = result.data.get("authorize_url").unwrap().as_str().unwrap();
        assert!(authorize_url.contains("response_type=code"));
        assert!(authorize_url.contains("code_challenge_method=S256"));
        assert!(authorize_url.contains("scope=read-only"));
        unsafe {
            std::env::remove_var("YNAB_AGENT_CLI_HOME");
        }
    }

    #[test]
    fn normalize_base_url_preserves_v1_path() {
        let url = normalize_base_url("https://api.ynab.com/v1").unwrap();
        assert_eq!(url.as_str(), "https://api.ynab.com/v1/");
        assert_eq!(url.join("plans").unwrap().as_str(), "https://api.ynab.com/v1/plans");
    }
}
