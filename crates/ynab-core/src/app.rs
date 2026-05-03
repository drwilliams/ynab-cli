use std::{cmp::Ordering, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration as ChronoDuration, NaiveDate, Utc};
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
        OutputEnvelope, PatchCategoryGroupWrapper, PatchCategoryWrapper, PatchMonthCategoryWrapper,
        PatchPayeeWrapper, PayeesData, PlanSummary, PlansData, PostCategoryGroupWrapper,
        PostCategoryWrapper, PostPayee, PostPayeeWrapper, SaveCategory, SaveCategoryGroup,
        SaveMonthCategory, SavePayee, SaveTransaction, SaveTransactionRequest, StoredSession,
        TransactionClearedFilter, TransactionsData, UpdateTransaction, UpdateTransactionRequest,
    },
    secrets::SecretStore,
};

#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    pub profile: Option<String>,
    pub use_keyring: bool,
    pub base_url_override: Option<String>,
    pub output_format: OutputFormat,
    pub access_token_override: Option<String>,
    pub access_token_override_source: Option<&'static str>,
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

#[derive(Debug, Clone, Default)]
pub struct TransactionListOptions {
    pub last_knowledge_of_server: Option<u64>,
    pub month: Option<String>,
    pub since_date: Option<String>,
    pub transaction_type: Option<String>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub cleared_filter: Option<TransactionClearedFilter>,
}

#[derive(Debug, Clone, Default)]
pub struct TransactionSearchOptions {
    pub query: Option<String>,
    pub payee: Option<String>,
    pub memo: Option<String>,
    pub account: Option<String>,
    pub category: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TransactionCreateInput {
    pub plan_id: String,
    pub account_id: String,
    pub date: String,
    pub amount: AmountMilliunits,
    pub id: Option<String>,
    pub import_id: Option<String>,
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
    access_token_override: Option<String>,
    access_token_override_source: Option<&'static str>,
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
            access_token_override: options.access_token_override,
            access_token_override_source: options.access_token_override_source,
            http,
        })
    }

    pub fn output_format(&self) -> OutputFormat {
        self.output_format
    }

    pub fn profile_name(&self) -> &str {
        &self.profile_name
    }

    pub fn auth_status(&self) -> Result<OutputEnvelope> {
        let profile = self
            .config
            .profile(&self.profile_name)
            .ok_or_else(|| YnabError::Config("profile missing".to_string()))?;
        let stored_session = self.secrets.load_session(&self.profile_name)?;
        let auth_source = if let Some(source) = self.access_token_override_source {
            source
        } else if std::env::var("YNAB_ACCESS_TOKEN").is_ok() {
            "env"
        } else if stored_session.is_some() {
            "stored"
        } else {
            "none"
        };
        let stored_auth_kind = stored_session.as_ref().map(|session| {
            if session.is_oauth() {
                "oauth"
            } else {
                "personal_access_token"
            }
        });

        Ok(Self::ok(json!({
            "profile": self.profile_name,
            "base_url": self.base_url.as_str(),
            "default_plan_id": profile.default_plan_id,
            "auth_source": auth_source,
            "auth_override_active": matches!(auth_source, "flag" | "env"),
            "stored_auth_kind": stored_auth_kind,
            "keyring_enabled": self.secrets.uses_keyring(),
            "runtime_home": self.config.paths().root_dir,
            "config_file": self.config.paths().config_file,
            "secrets_file": self.config.paths().secrets_file
        })))
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

    pub async fn set_personal_access_token(&mut self, token: String) -> Result<OutputEnvelope> {
        self.secrets.save_session(
            &self.profile_name,
            &StoredSession::PersonalAccessToken {
                access_token: token,
            },
        )?;
        let (default_plan_id, default_plan_auto_selected, default_plan_error) =
            default_plan_auth_fields(self.ensure_default_plan_selected().await);
        Ok(Self::ok(json!({
            "profile": self.profile_name,
            "auth_kind": "personal_access_token",
            "default_plan_id": default_plan_id,
            "default_plan_auto_selected": default_plan_auto_selected,
            "default_plan_error": default_plan_error
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
        let (default_plan_id, default_plan_auto_selected, default_plan_error) =
            default_plan_auth_fields(self.ensure_default_plan_selected().await);

        Ok(Self::ok(json!({
            "profile": self.profile_name,
            "auth_kind": "oauth",
            "expires_at": match &session {
                StoredSession::OAuth { expires_at, .. } => expires_at.to_rfc3339(),
                _ => unreachable!(),
            },
            "default_plan_id": default_plan_id,
            "default_plan_auto_selected": default_plan_auto_selected,
            "default_plan_error": default_plan_error
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
            .request_data_with_query(
                Method::GET,
                "/plans",
                (),
                options.last_knowledge_of_server,
                &[],
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn get_plan(&mut self, plan_id: &str) -> Result<OutputEnvelope> {
        let data = self
            .request_data(Method::GET, &format!("/plans/{plan_id}"), (), None)
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn list_plans_with_include_accounts(
        &mut self,
        options: ResourceListOptions,
        include_accounts: bool,
    ) -> Result<OutputEnvelope> {
        let query = if include_accounts {
            vec![("include_accounts", "true".to_string())]
        } else {
            Vec::new()
        };
        let data = self
            .request_data_with_query(
                Method::GET,
                "/plans",
                (),
                options.last_knowledge_of_server,
                &query,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn get_plan_settings(&mut self, plan_id: &str) -> Result<OutputEnvelope> {
        let data = self
            .request_data(Method::GET, &format!("/plans/{plan_id}/settings"), (), None)
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

    pub async fn get_account(&mut self, plan_id: &str, account_id: &str) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::GET,
                &format!("/plans/{plan_id}/accounts/{account_id}"),
                (),
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn create_account(
        &mut self,
        plan_id: &str,
        account: Value,
    ) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::POST,
                &format!("/plans/{plan_id}/accounts"),
                &json!({ "account": account }),
                None,
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

    pub async fn create_category(
        &mut self,
        plan_id: &str,
        category: SaveCategory,
    ) -> Result<OutputEnvelope> {
        let request = PostCategoryWrapper { category };
        let data = self
            .request_data(
                Method::POST,
                &format!("/plans/{plan_id}/categories"),
                &request,
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn get_category(
        &mut self,
        plan_id: &str,
        category_id: &str,
    ) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::GET,
                &format!("/plans/{plan_id}/categories/{category_id}"),
                (),
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn update_category(
        &mut self,
        plan_id: &str,
        category_id: &str,
        category: SaveCategory,
    ) -> Result<OutputEnvelope> {
        let request = PatchCategoryWrapper { category };
        let data = self
            .request_data(
                Method::PATCH,
                &format!("/plans/{plan_id}/categories/{category_id}"),
                &request,
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn update_month_category(
        &mut self,
        plan_id: &str,
        month: &str,
        category_id: &str,
        budgeted: i64,
    ) -> Result<OutputEnvelope> {
        let request = PatchMonthCategoryWrapper {
            category: SaveMonthCategory { budgeted },
        };
        let data = self
            .request_data(
                Method::PATCH,
                &format!("/plans/{plan_id}/months/{month}/categories/{category_id}"),
                &request,
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn create_category_group(
        &mut self,
        plan_id: &str,
        name: String,
    ) -> Result<OutputEnvelope> {
        let request = PostCategoryGroupWrapper {
            category_group: SaveCategoryGroup { name },
        };
        let data = self
            .request_data(
                Method::POST,
                &format!("/plans/{plan_id}/category_groups"),
                &request,
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn update_category_group(
        &mut self,
        plan_id: &str,
        category_group_id: &str,
        name: String,
    ) -> Result<OutputEnvelope> {
        let request = PatchCategoryGroupWrapper {
            category_group: SaveCategoryGroup { name },
        };
        let data = self
            .request_data(
                Method::PATCH,
                &format!("/plans/{plan_id}/category_groups/{category_group_id}"),
                &request,
                None,
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

    pub async fn create_payee(&mut self, plan_id: &str, name: String) -> Result<OutputEnvelope> {
        let request = PostPayeeWrapper {
            payee: PostPayee { name },
        };
        let data = self
            .request_data(
                Method::POST,
                &format!("/plans/{plan_id}/payees"),
                &request,
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn update_payee(
        &mut self,
        plan_id: &str,
        payee_id: &str,
        name: String,
    ) -> Result<OutputEnvelope> {
        let request = PatchPayeeWrapper {
            payee: SavePayee { name: Some(name) },
        };
        let data = self
            .request_data(
                Method::PATCH,
                &format!("/plans/{plan_id}/payees/{payee_id}"),
                &request,
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn list_transactions(
        &mut self,
        plan_id: &str,
        options: TransactionListOptions,
    ) -> Result<OutputEnvelope> {
        if let Some(month) = options.month.as_deref() {
            self.list_transactions_from_path(
                &format!("/plans/{plan_id}/months/{month}/transactions"),
                options,
                false,
            )
            .await
        } else {
            self.list_transactions_from_path(
                &format!("/plans/{plan_id}/transactions"),
                options,
                true,
            )
            .await
        }
    }

    pub async fn get_transaction(
        &mut self,
        plan_id: &str,
        transaction_id: &str,
    ) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::GET,
                &format!("/plans/{plan_id}/transactions/{transaction_id}"),
                (),
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn search_transactions(
        &mut self,
        plan_id: &str,
        list_options: TransactionListOptions,
        search_options: TransactionSearchOptions,
    ) -> Result<OutputEnvelope> {
        validate_transaction_search_options(&search_options)?;
        let mut envelope = self.list_transactions(plan_id, list_options).await?;
        let filtered = envelope
            .data
            .get("transactions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|transaction| transaction_matches_search(transaction, &search_options))
            .cloned()
            .collect::<Vec<_>>();
        envelope.data = json!({
            "transactions": filtered,
            "server_knowledge": envelope.data.get("server_knowledge").cloned().unwrap_or(Value::Null),
        });
        Ok(envelope)
    }

    pub async fn delete_transaction(
        &mut self,
        plan_id: &str,
        transaction_id: &str,
    ) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::DELETE,
                &format!("/plans/{plan_id}/transactions/{transaction_id}"),
                (),
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn list_transactions_by_account(
        &mut self,
        plan_id: &str,
        account_id: &str,
        options: TransactionListOptions,
    ) -> Result<OutputEnvelope> {
        self.list_transactions_from_path(
            &format!("/plans/{plan_id}/accounts/{account_id}/transactions"),
            options,
            true,
        )
        .await
    }

    pub async fn list_transactions_by_category(
        &mut self,
        plan_id: &str,
        category_id: &str,
        options: TransactionListOptions,
    ) -> Result<OutputEnvelope> {
        self.list_transactions_from_path(
            &format!("/plans/{plan_id}/categories/{category_id}/transactions"),
            options,
            true,
        )
        .await
    }

    pub async fn list_transactions_by_payee(
        &mut self,
        plan_id: &str,
        payee_id: &str,
        options: TransactionListOptions,
    ) -> Result<OutputEnvelope> {
        self.list_transactions_from_path(
            &format!("/plans/{plan_id}/payees/{payee_id}/transactions"),
            options,
            true,
        )
        .await
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
                id: input.id,
                import_id: input.import_id,
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
                Method::PUT,
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

    pub async fn create_transactions_bulk(
        &mut self,
        plan_id: &str,
        request: Value,
        dry_run: bool,
    ) -> Result<OutputEnvelope> {
        self.submit_json_request(
            Method::POST,
            &format!("/plans/{plan_id}/transactions"),
            request,
            dry_run,
        )
        .await
    }

    pub async fn import_transactions(
        &mut self,
        plan_id: &str,
        request: Value,
        dry_run: bool,
    ) -> Result<OutputEnvelope> {
        self.submit_json_request(
            Method::POST,
            &format!("/plans/{plan_id}/transactions/import"),
            request,
            dry_run,
        )
        .await
    }

    pub async fn update_transactions_bulk(
        &mut self,
        plan_id: &str,
        request: Value,
        dry_run: bool,
    ) -> Result<OutputEnvelope> {
        self.submit_json_request(
            Method::PATCH,
            &format!("/plans/{plan_id}/transactions"),
            request,
            dry_run,
        )
        .await
    }

    pub async fn list_months(&mut self, plan_id: &str) -> Result<OutputEnvelope> {
        let data = self
            .request_data(Method::GET, &format!("/plans/{plan_id}/months"), (), None)
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn get_month(&mut self, plan_id: &str, month: &str) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::GET,
                &format!("/plans/{plan_id}/months/{month}"),
                (),
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn list_scheduled_transactions(
        &mut self,
        plan_id: &str,
        options: ResourceListOptions,
    ) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::GET,
                &format!("/plans/{plan_id}/scheduled_transactions"),
                (),
                options.last_knowledge_of_server,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn get_scheduled_transaction(
        &mut self,
        plan_id: &str,
        scheduled_transaction_id: &str,
    ) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::GET,
                &format!("/plans/{plan_id}/scheduled_transactions/{scheduled_transaction_id}"),
                (),
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn create_scheduled_transaction(
        &mut self,
        plan_id: &str,
        scheduled_transaction: Value,
    ) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::POST,
                &format!("/plans/{plan_id}/scheduled_transactions"),
                &json!({ "scheduled_transaction": scheduled_transaction }),
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn update_scheduled_transaction(
        &mut self,
        plan_id: &str,
        scheduled_transaction_id: &str,
        scheduled_transaction: Value,
    ) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::PUT,
                &format!("/plans/{plan_id}/scheduled_transactions/{scheduled_transaction_id}"),
                &json!({ "scheduled_transaction": scheduled_transaction }),
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn delete_scheduled_transaction(
        &mut self,
        plan_id: &str,
        scheduled_transaction_id: &str,
    ) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::DELETE,
                &format!("/plans/{plan_id}/scheduled_transactions/{scheduled_transaction_id}"),
                (),
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn list_money_movements(&mut self, plan_id: &str) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::GET,
                &format!("/plans/{plan_id}/money_movements"),
                (),
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn list_money_movements_by_month(
        &mut self,
        plan_id: &str,
        month: &str,
    ) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::GET,
                &format!("/plans/{plan_id}/months/{month}/money_movements"),
                (),
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn list_money_movement_groups(&mut self, plan_id: &str) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::GET,
                &format!("/plans/{plan_id}/money_movement_groups"),
                (),
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn list_money_movement_groups_by_month(
        &mut self,
        plan_id: &str,
        month: &str,
    ) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::GET,
                &format!("/plans/{plan_id}/months/{month}/money_movement_groups"),
                (),
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn list_payee_locations(&mut self, plan_id: &str) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::GET,
                &format!("/plans/{plan_id}/payee_locations"),
                (),
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn get_payee_location(
        &mut self,
        plan_id: &str,
        payee_location_id: &str,
    ) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::GET,
                &format!("/plans/{plan_id}/payee_locations/{payee_location_id}"),
                (),
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn list_payee_locations_by_payee(
        &mut self,
        plan_id: &str,
        payee_id: &str,
    ) -> Result<OutputEnvelope> {
        let data = self
            .request_data(
                Method::GET,
                &format!("/plans/{plan_id}/payees/{payee_id}/payee_locations"),
                (),
                None,
            )
            .await?;
        Ok(Self::ok(data))
    }

    pub async fn get_user(&mut self) -> Result<OutputEnvelope> {
        let data = self.request_data(Method::GET, "/user", (), None).await?;
        Ok(Self::ok(data))
    }

    pub async fn resolve_plan_argument(&mut self, provided: Option<String>) -> Result<String> {
        if let Some(plan_id) = provided {
            return Ok(plan_id);
        }
        let profile = self
            .config
            .profile(&self.profile_name)
            .ok_or_else(|| YnabError::Config("profile not found".to_string()))?;
        if let Some(plan_id) = profile.default_plan_id.clone() {
            return Ok(plan_id);
        }

        self.auto_select_default_plan().await
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
                let mut list: Vec<NamedResource> = response
                    .data
                    .plans
                    .into_iter()
                    .map(NamedResource::from)
                    .collect();
                if let Some(default_plan) = response.data.default_plan {
                    list.push(default_plan.into());
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
        self.request_data_with_query(method, path, body, last_knowledge_of_server, &[])
            .await
    }

    async fn request_data_with_query<B: Serialize>(
        &mut self,
        method: Method,
        path: &str,
        body: B,
        last_knowledge_of_server: Option<u64>,
        query: &[(&str, String)],
    ) -> Result<Value> {
        let response: ApiResponse<Value> = self
            .request_typed_with_query(method, path, body, last_knowledge_of_server, query)
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
        self.request_typed_with_query(method, path, body, last_knowledge_of_server, &[])
            .await
    }

    async fn request_typed_with_query<T: for<'de> serde::Deserialize<'de>, B: Serialize>(
        &mut self,
        method: Method,
        path: &str,
        body: B,
        last_knowledge_of_server: Option<u64>,
        query: &[(&str, String)],
    ) -> Result<T> {
        let response = self
            .send_api_request_with_query(method, path, body, last_knowledge_of_server, query)
            .await?;
        Ok(serde_json::from_value(response)?)
    }

    async fn send_api_request_with_query<B: Serialize>(
        &mut self,
        method: Method,
        path: &str,
        body: B,
        last_knowledge_of_server: Option<u64>,
        query: &[(&str, String)],
    ) -> Result<Value> {
        let mut refreshed = false;

        loop {
            let token = self.ensure_access_token(false).await?;
            let mut url = self.base_url.join(path.trim_start_matches('/'))?;
            if let Some(value) = last_knowledge_of_server {
                url.query_pairs_mut()
                    .append_pair("last_knowledge_of_server", &value.to_string());
            }
            for (key, value) in query {
                url.query_pairs_mut().append_pair(key, value);
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

    async fn list_transactions_from_path(
        &mut self,
        path: &str,
        options: TransactionListOptions,
        include_query_filters: bool,
    ) -> Result<OutputEnvelope> {
        let query = if include_query_filters {
            build_transaction_query(&options)
        } else {
            Vec::new()
        };
        let response: ApiResponse<TransactionsData> = self
            .request_typed_with_query(
                Method::GET,
                path,
                (),
                if include_query_filters {
                    options.last_knowledge_of_server
                } else {
                    None
                },
                &query,
            )
            .await?;

        let filtered_transactions = response
            .data
            .transactions
            .into_iter()
            .filter(|transaction| transaction_matches_filters(transaction, &options))
            .collect::<Vec<_>>();

        Ok(Self::ok(json!({
            "transactions": filtered_transactions,
            "server_knowledge": response.data.server_knowledge
        })))
    }

    async fn submit_json_request(
        &mut self,
        method: Method,
        path: &str,
        request: Value,
        dry_run: bool,
    ) -> Result<OutputEnvelope> {
        if dry_run {
            return Ok(Self::ok(json!({
                "dry_run": true,
                "request": request
            })));
        }

        let data = self.request_data(method, path, &request, None).await?;
        Ok(Self::ok(data))
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
        if let Some(token) = self.access_token_override.as_ref() {
            return Ok(token.clone());
        }
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

    async fn auto_select_default_plan(&mut self) -> Result<String> {
        let response: ApiResponse<PlansData> =
            self.request_typed(Method::GET, "/plans", (), None).await?;
        let plan = select_most_recent_plan(&response.data).ok_or_else(|| {
            YnabError::Config(
                "missing plan id and no plans were available to auto-select a default".to_string(),
            )
        })?;
        let plan_id = plan.id.clone();

        let profile = self.config.profile_mut(&self.profile_name)?;
        profile.default_plan_id = Some(plan_id.clone());
        self.config.save()?;

        Ok(plan_id)
    }

    async fn ensure_default_plan_selected(&mut self) -> Result<(Option<String>, bool)> {
        let profile = self
            .config
            .profile(&self.profile_name)
            .ok_or_else(|| YnabError::Config("profile not found".to_string()))?;
        if let Some(plan_id) = profile.default_plan_id.clone() {
            return Ok((Some(plan_id), false));
        }

        let plan_id = self.auto_select_default_plan().await?;
        Ok((Some(plan_id), true))
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

fn select_most_recent_plan(plans_data: &PlansData) -> Option<&PlanSummary> {
    let mut candidates: Vec<&PlanSummary> = plans_data.plans.iter().collect();
    if let Some(default_plan) = plans_data.default_plan.as_ref()
        && !candidates.iter().any(|plan| plan.id == default_plan.id)
    {
        candidates.push(default_plan);
    }

    candidates.into_iter().max_by(|left, right| {
        compare_last_modified(
            left.last_modified_on.as_ref(),
            right.last_modified_on.as_ref(),
        )
        .then_with(|| left.id.cmp(&right.id))
    })
}

fn compare_last_modified(
    left: Option<&chrono::DateTime<Utc>>,
    right: Option<&chrono::DateTime<Utc>>,
) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(right),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => Ordering::Equal,
    }
}

fn build_transaction_query(options: &TransactionListOptions) -> Vec<(&'static str, String)> {
    let mut query = Vec::new();
    if let Some(since_date) = options.since_date.as_ref() {
        query.push(("since_date", since_date.clone()));
    }
    if let Some(transaction_type) = options.transaction_type.as_ref() {
        query.push(("type", transaction_type.clone()));
    }
    query
}

fn transaction_matches_filters(transaction: &Value, options: &TransactionListOptions) -> bool {
    let Some(date) = transaction
        .get("date")
        .and_then(Value::as_str)
        .and_then(parse_transaction_date)
    else {
        return false;
    };

    if let Some(start_date) = options.start_date
        && date < start_date
    {
        return false;
    }

    if let Some(end_date) = options.end_date
        && date > end_date
    {
        return false;
    }

    if let Some(cleared_filter) = options.cleared_filter {
        let status = transaction
            .get("cleared")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches_cleared_filter(status, cleared_filter) {
            return false;
        }
    }

    true
}

fn parse_transaction_date(input: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(input, "%Y-%m-%d").ok()
}

fn matches_cleared_filter(status: &str, filter: TransactionClearedFilter) -> bool {
    match filter {
        TransactionClearedFilter::Cleared => matches!(status, "cleared" | "reconciled"),
        TransactionClearedFilter::Uncleared => status == "uncleared",
    }
}

fn validate_transaction_search_options(options: &TransactionSearchOptions) -> Result<()> {
    if options.query.is_none()
        && options.payee.is_none()
        && options.memo.is_none()
        && options.account.is_none()
        && options.category.is_none()
    {
        return Err(YnabError::Config(
            "transactions search requires at least one of query, payee, memo, account, or category"
                .to_string(),
        ));
    }
    Ok(())
}

fn transaction_matches_search(transaction: &Value, options: &TransactionSearchOptions) -> bool {
    if let Some(query) = options.query.as_deref()
        && !matches_any_transaction_field(
            transaction,
            query,
            &["payee_name", "memo", "account_name", "category_name"],
        )
    {
        return false;
    }

    if let Some(payee) = options.payee.as_deref()
        && !transaction_field_contains(transaction, "payee_name", payee)
    {
        return false;
    }

    if let Some(memo) = options.memo.as_deref()
        && !transaction_field_contains(transaction, "memo", memo)
    {
        return false;
    }

    if let Some(account) = options.account.as_deref()
        && !transaction_field_contains(transaction, "account_name", account)
    {
        return false;
    }

    if let Some(category) = options.category.as_deref()
        && !transaction_field_contains(transaction, "category_name", category)
    {
        return false;
    }

    true
}

fn matches_any_transaction_field(transaction: &Value, needle: &str, fields: &[&str]) -> bool {
    fields
        .iter()
        .any(|field| transaction_field_contains(transaction, field, needle))
}

fn transaction_field_contains(transaction: &Value, field: &str, needle: &str) -> bool {
    let needle = needle.trim();
    if needle.is_empty() {
        return false;
    }
    transaction
        .get(field)
        .and_then(Value::as_str)
        .map(|value| value.to_lowercase().contains(&needle.to_lowercase()))
        .unwrap_or(false)
}

fn default_plan_auth_fields(
    result: Result<(Option<String>, bool)>,
) -> (Option<String>, bool, Option<String>) {
    match result {
        Ok((plan_id, auto_selected)) => (plan_id, auto_selected, None),
        Err(error) => (None, false, Some(error.to_string())),
    }
}

#[cfg(test)]
#[allow(clippy::await_holding_lock)]
mod tests {
    use std::{
        fs,
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use chrono::NaiveDate;
    use serde_json::{Value, json};
    use tempfile::TempDir;

    use super::{
        AppState, ResourceListOptions, RuntimeOptions, TransactionCreateInput,
        TransactionListOptions, TransactionUpdateInput, normalize_base_url,
        transaction_matches_filters,
    };
    use crate::{
        AmountMilliunits, OAuthAppInput, OAuthScope, OutputFormat, SaveCategory,
        TransactionClearedFilter,
    };

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
            access_token_override: None,
            access_token_override_source: None,
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
        assert_eq!(
            url.join("plans").unwrap().as_str(),
            "https://api.ynab.com/v1/plans"
        );
    }

    #[tokio::test]
    async fn resolve_plan_argument_auto_selects_and_persists_most_recent_plan() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let base_url = spawn_json_server(
            json!({
                "data": {
                    "plans": [
                        {
                            "id": "older-plan",
                            "name": "Older Plan",
                            "last_modified_on": "2025-01-01T00:00:00Z"
                        },
                        {
                            "id": "newer-plan",
                            "name": "Newer Plan",
                            "last_modified_on": "2025-02-01T00:00:00Z"
                        }
                    ]
                }
            })
            .to_string(),
        );

        unsafe {
            std::env::set_var("YNAB_AGENT_CLI_HOME", temp_dir.path());
            std::env::set_var("YNAB_ACCESS_TOKEN", "test-token");
        }

        let mut app = AppState::load(RuntimeOptions {
            profile: None,
            use_keyring: false,
            base_url_override: Some(base_url),
            output_format: OutputFormat::Json,
            access_token_override: None,
            access_token_override_source: None,
        })
        .unwrap();

        let plan_id = app.resolve_plan_argument(None).await.unwrap();
        assert_eq!(plan_id, "newer-plan");

        let config: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(temp_dir.path().join("config.json")).unwrap())
                .unwrap();
        assert_eq!(
            config["profiles"]["default"]["default_plan_id"].as_str(),
            Some("newer-plan")
        );

        unsafe {
            std::env::remove_var("YNAB_AGENT_CLI_HOME");
            std::env::remove_var("YNAB_ACCESS_TOKEN");
        }
    }

    #[tokio::test]
    async fn resolve_plan_argument_keeps_existing_default_plan() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        fs::write(
            temp_dir.path().join("config.json"),
            json!({
                "version": 1,
                "current_profile": "default",
                "profiles": {
                    "default": {
                        "base_url": "https://api.ynab.com/v1/",
                        "default_plan_id": "configured-plan"
                    }
                }
            })
            .to_string(),
        )
        .unwrap();

        unsafe {
            std::env::set_var("YNAB_AGENT_CLI_HOME", temp_dir.path());
        }

        let mut app = AppState::load(RuntimeOptions {
            profile: None,
            use_keyring: false,
            base_url_override: Some("http://127.0.0.1:9/v1".to_string()),
            output_format: OutputFormat::Json,
            access_token_override: None,
            access_token_override_source: None,
        })
        .unwrap();

        let plan_id = app.resolve_plan_argument(None).await.unwrap();
        assert_eq!(plan_id, "configured-plan");

        unsafe {
            std::env::remove_var("YNAB_AGENT_CLI_HOME");
        }
    }

    #[test]
    fn transaction_filters_apply_date_and_cleared_rules() {
        let options = TransactionListOptions {
            start_date: Some(NaiveDate::from_ymd_opt(2026, 4, 11).unwrap()),
            end_date: Some(NaiveDate::from_ymd_opt(2026, 4, 18).unwrap()),
            cleared_filter: Some(TransactionClearedFilter::Uncleared),
            ..TransactionListOptions::default()
        };

        assert!(transaction_matches_filters(
            &json!({
                "date": "2026-04-11",
                "cleared": "uncleared"
            }),
            &options
        ));
        assert!(!transaction_matches_filters(
            &json!({
                "date": "2026-04-10",
                "cleared": "uncleared"
            }),
            &options
        ));
        assert!(!transaction_matches_filters(
            &json!({
                "date": "2026-04-12",
                "cleared": "cleared"
            }),
            &options
        ));
    }

    #[tokio::test]
    async fn list_transactions_uses_month_endpoint_when_requested() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let base_url = spawn_asserting_server(
            "/v1/plans/default/months/2026-04-01/transactions",
            json!({
                "data": {
                    "transactions": [
                        {
                            "id": "tx-1",
                            "date": "2026-04-11",
                            "cleared": "uncleared"
                        }
                    ],
                    "server_knowledge": 77
                }
            })
            .to_string(),
        );

        unsafe {
            std::env::set_var("YNAB_AGENT_CLI_HOME", temp_dir.path());
            std::env::set_var("YNAB_ACCESS_TOKEN", "test-token");
        }

        let mut app = AppState::load(RuntimeOptions {
            profile: None,
            use_keyring: false,
            base_url_override: Some(base_url),
            output_format: OutputFormat::Json,
            access_token_override: None,
            access_token_override_source: None,
        })
        .unwrap();

        let response = app
            .list_transactions(
                "default",
                TransactionListOptions {
                    month: Some("2026-04-01".to_string()),
                    ..TransactionListOptions::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(
            response
                .data
                .get("transactions")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            response
                .data
                .get("server_knowledge")
                .and_then(Value::as_u64),
            Some(77)
        );

        unsafe {
            std::env::remove_var("YNAB_AGENT_CLI_HOME");
            std::env::remove_var("YNAB_ACCESS_TOKEN");
        }
    }

    #[tokio::test]
    async fn create_payee_uses_expected_endpoint_and_body() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let base_url = spawn_asserting_server_with_body(
            "POST",
            "/v1/plans/default/payees",
            "\"name\":\"Test Payee\"",
            json!({
                "data": {
                    "payee": {
                        "id": "payee-1",
                        "name": "Test Payee"
                    }
                }
            })
            .to_string(),
        );

        unsafe {
            std::env::set_var("YNAB_AGENT_CLI_HOME", temp_dir.path());
            std::env::set_var("YNAB_ACCESS_TOKEN", "test-token");
        }

        let mut app = AppState::load(RuntimeOptions {
            profile: None,
            use_keyring: false,
            base_url_override: Some(base_url),
            output_format: OutputFormat::Json,
            access_token_override: None,
            access_token_override_source: None,
        })
        .unwrap();

        let response = app
            .create_payee("default", "Test Payee".to_string())
            .await
            .unwrap();
        assert_eq!(
            response
                .data
                .get("payee")
                .and_then(|payee| payee.get("name"))
                .and_then(Value::as_str),
            Some("Test Payee")
        );

        unsafe {
            std::env::remove_var("YNAB_AGENT_CLI_HOME");
            std::env::remove_var("YNAB_ACCESS_TOKEN");
        }
    }

    #[tokio::test]
    async fn update_payee_uses_expected_endpoint_and_body() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let base_url = spawn_asserting_server_with_body(
            "PATCH",
            "/v1/plans/default/payees/payee-1",
            "\"name\":\"Renamed Payee\"",
            json!({
                "data": {
                    "payee": {
                        "id": "payee-1",
                        "name": "Renamed Payee"
                    }
                }
            })
            .to_string(),
        );

        unsafe {
            std::env::set_var("YNAB_AGENT_CLI_HOME", temp_dir.path());
            std::env::set_var("YNAB_ACCESS_TOKEN", "test-token");
        }

        let mut app = AppState::load(RuntimeOptions {
            profile: None,
            use_keyring: false,
            base_url_override: Some(base_url),
            output_format: OutputFormat::Json,
            access_token_override: None,
            access_token_override_source: None,
        })
        .unwrap();

        let response = app
            .update_payee("default", "payee-1", "Renamed Payee".to_string())
            .await
            .unwrap();
        assert_eq!(
            response
                .data
                .get("payee")
                .and_then(|payee| payee.get("name"))
                .and_then(Value::as_str),
            Some("Renamed Payee")
        );

        unsafe {
            std::env::remove_var("YNAB_AGENT_CLI_HOME");
            std::env::remove_var("YNAB_ACCESS_TOKEN");
        }
    }

    #[tokio::test]
    async fn create_category_uses_expected_endpoint_and_body() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let base_url = spawn_asserting_server_with_body(
            "POST",
            "/v1/plans/default/categories",
            "\"category_group_id\":\"group-1\"",
            json!({
                "data": {
                    "category": {
                        "id": "category-1",
                        "name": "Test Category"
                    }
                }
            })
            .to_string(),
        );

        unsafe {
            std::env::set_var("YNAB_AGENT_CLI_HOME", temp_dir.path());
            std::env::set_var("YNAB_ACCESS_TOKEN", "test-token");
        }

        let mut app = AppState::load(RuntimeOptions {
            profile: None,
            use_keyring: false,
            base_url_override: Some(base_url),
            output_format: OutputFormat::Json,
            access_token_override: None,
            access_token_override_source: None,
        })
        .unwrap();

        let response = app
            .create_category(
                "default",
                SaveCategory {
                    name: Some("Test Category".to_string()),
                    category_group_id: Some("group-1".to_string()),
                    ..SaveCategory::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(
            response
                .data
                .get("category")
                .and_then(|category| category.get("name"))
                .and_then(Value::as_str),
            Some("Test Category")
        );

        unsafe {
            std::env::remove_var("YNAB_AGENT_CLI_HOME");
            std::env::remove_var("YNAB_ACCESS_TOKEN");
        }
    }

    #[tokio::test]
    async fn update_category_uses_expected_endpoint_and_body() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let base_url = spawn_asserting_server_with_body(
            "PATCH",
            "/v1/plans/default/categories/category-1",
            "\"name\":\"Renamed Category\"",
            json!({
                "data": {
                    "category": {
                        "id": "category-1",
                        "name": "Renamed Category"
                    }
                }
            })
            .to_string(),
        );

        unsafe {
            std::env::set_var("YNAB_AGENT_CLI_HOME", temp_dir.path());
            std::env::set_var("YNAB_ACCESS_TOKEN", "test-token");
        }

        let mut app = AppState::load(RuntimeOptions {
            profile: None,
            use_keyring: false,
            base_url_override: Some(base_url),
            output_format: OutputFormat::Json,
            access_token_override: None,
            access_token_override_source: None,
        })
        .unwrap();

        let response = app
            .update_category(
                "default",
                "category-1",
                SaveCategory {
                    name: Some("Renamed Category".to_string()),
                    ..SaveCategory::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(
            response
                .data
                .get("category")
                .and_then(|category| category.get("name"))
                .and_then(Value::as_str),
            Some("Renamed Category")
        );

        unsafe {
            std::env::remove_var("YNAB_AGENT_CLI_HOME");
            std::env::remove_var("YNAB_ACCESS_TOKEN");
        }
    }

    #[tokio::test]
    async fn get_category_uses_expected_endpoint() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let base_url = spawn_asserting_server(
            "/v1/plans/default/categories/category-1",
            json!({
                "data": {
                    "category": {
                        "id": "category-1",
                        "name": "Test Category"
                    }
                }
            })
            .to_string(),
        );

        unsafe {
            std::env::set_var("YNAB_AGENT_CLI_HOME", temp_dir.path());
            std::env::set_var("YNAB_ACCESS_TOKEN", "test-token");
        }

        let mut app = AppState::load(RuntimeOptions {
            profile: None,
            use_keyring: false,
            base_url_override: Some(base_url),
            output_format: OutputFormat::Json,
            access_token_override: None,
            access_token_override_source: None,
        })
        .unwrap();

        let response = app.get_category("default", "category-1").await.unwrap();
        assert_eq!(
            response
                .data
                .get("category")
                .and_then(|category| category.get("name"))
                .and_then(Value::as_str),
            Some("Test Category")
        );

        unsafe {
            std::env::remove_var("YNAB_AGENT_CLI_HOME");
            std::env::remove_var("YNAB_ACCESS_TOKEN");
        }
    }

    #[tokio::test]
    async fn update_month_category_uses_expected_endpoint_and_body() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let base_url = spawn_asserting_server_with_body(
            "PATCH",
            "/v1/plans/default/months/2026-04-01/categories/category-1",
            "\"budgeted\":150000",
            json!({
                "data": {
                    "category": {
                        "id": "category-1",
                        "name": "Test Category",
                        "budgeted": 150000
                    }
                }
            })
            .to_string(),
        );

        unsafe {
            std::env::set_var("YNAB_AGENT_CLI_HOME", temp_dir.path());
            std::env::set_var("YNAB_ACCESS_TOKEN", "test-token");
        }

        let mut app = AppState::load(RuntimeOptions {
            profile: None,
            use_keyring: false,
            base_url_override: Some(base_url),
            output_format: OutputFormat::Json,
            access_token_override: None,
            access_token_override_source: None,
        })
        .unwrap();

        let response = app
            .update_month_category("default", "2026-04-01", "category-1", 150000)
            .await
            .unwrap();
        assert_eq!(
            response
                .data
                .get("category")
                .and_then(|category| category.get("budgeted"))
                .and_then(Value::as_i64),
            Some(150000)
        );

        unsafe {
            std::env::remove_var("YNAB_AGENT_CLI_HOME");
            std::env::remove_var("YNAB_ACCESS_TOKEN");
        }
    }

    #[tokio::test]
    async fn create_category_group_uses_expected_endpoint_and_body() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let base_url = spawn_asserting_server_with_body(
            "POST",
            "/v1/plans/default/category_groups",
            "\"name\":\"New Group\"",
            json!({
                "data": {
                    "category_group": {
                        "id": "group-1",
                        "name": "New Group"
                    }
                }
            })
            .to_string(),
        );

        unsafe {
            std::env::set_var("YNAB_AGENT_CLI_HOME", temp_dir.path());
            std::env::set_var("YNAB_ACCESS_TOKEN", "test-token");
        }

        let mut app = AppState::load(RuntimeOptions {
            profile: None,
            use_keyring: false,
            base_url_override: Some(base_url),
            output_format: OutputFormat::Json,
            access_token_override: None,
            access_token_override_source: None,
        })
        .unwrap();

        let response = app
            .create_category_group("default", "New Group".to_string())
            .await
            .unwrap();
        assert_eq!(
            response
                .data
                .get("category_group")
                .and_then(|group| group.get("name"))
                .and_then(Value::as_str),
            Some("New Group")
        );

        unsafe {
            std::env::remove_var("YNAB_AGENT_CLI_HOME");
            std::env::remove_var("YNAB_ACCESS_TOKEN");
        }
    }

    #[tokio::test]
    async fn update_category_group_uses_expected_endpoint_and_body() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let base_url = spawn_asserting_server_with_body(
            "PATCH",
            "/v1/plans/default/category_groups/group-1",
            "\"name\":\"Renamed Group\"",
            json!({
                "data": {
                    "category_group": {
                        "id": "group-1",
                        "name": "Renamed Group"
                    }
                }
            })
            .to_string(),
        );

        unsafe {
            std::env::set_var("YNAB_AGENT_CLI_HOME", temp_dir.path());
            std::env::set_var("YNAB_ACCESS_TOKEN", "test-token");
        }

        let mut app = AppState::load(RuntimeOptions {
            profile: None,
            use_keyring: false,
            base_url_override: Some(base_url),
            output_format: OutputFormat::Json,
            access_token_override: None,
            access_token_override_source: None,
        })
        .unwrap();

        let response = app
            .update_category_group("default", "group-1", "Renamed Group".to_string())
            .await
            .unwrap();
        assert_eq!(
            response
                .data
                .get("category_group")
                .and_then(|group| group.get("name"))
                .and_then(Value::as_str),
            Some("Renamed Group")
        );

        unsafe {
            std::env::remove_var("YNAB_AGENT_CLI_HOME");
            std::env::remove_var("YNAB_ACCESS_TOKEN");
        }
    }

    #[tokio::test]
    async fn list_plans_with_include_accounts_uses_query_parameter() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let base_url = spawn_asserting_server(
            "/v1/plans?include_accounts=true",
            json!({
                "data": {
                    "plans": []
                }
            })
            .to_string(),
        );

        unsafe {
            std::env::set_var("YNAB_AGENT_CLI_HOME", temp_dir.path());
            std::env::set_var("YNAB_ACCESS_TOKEN", "test-token");
        }

        let mut app = AppState::load(RuntimeOptions {
            profile: None,
            use_keyring: false,
            base_url_override: Some(base_url),
            output_format: OutputFormat::Json,
            access_token_override: None,
            access_token_override_source: None,
        })
        .unwrap();

        let response = app
            .list_plans_with_include_accounts(ResourceListOptions::default(), true)
            .await
            .unwrap();
        assert_eq!(
            response
                .data
                .get("plans")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(0)
        );

        unsafe {
            std::env::remove_var("YNAB_AGENT_CLI_HOME");
            std::env::remove_var("YNAB_ACCESS_TOKEN");
        }
    }

    #[tokio::test]
    async fn create_account_uses_expected_endpoint_and_body() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let base_url = spawn_asserting_server_with_body(
            "POST",
            "/v1/plans/default/accounts",
            "\"type\":\"checking\"",
            json!({
                "data": {
                    "account": {
                        "id": "account-1",
                        "name": "Checking"
                    }
                }
            })
            .to_string(),
        );

        unsafe {
            std::env::set_var("YNAB_AGENT_CLI_HOME", temp_dir.path());
            std::env::set_var("YNAB_ACCESS_TOKEN", "test-token");
        }

        let mut app = AppState::load(RuntimeOptions {
            profile: None,
            use_keyring: false,
            base_url_override: Some(base_url),
            output_format: OutputFormat::Json,
            access_token_override: None,
            access_token_override_source: None,
        })
        .unwrap();

        let response = app
            .create_account(
                "default",
                json!({
                    "name": "Checking",
                    "type": "checking",
                    "balance": 1000
                }),
            )
            .await
            .unwrap();
        assert_eq!(
            response
                .data
                .get("account")
                .and_then(|account| account.get("name"))
                .and_then(Value::as_str),
            Some("Checking")
        );

        unsafe {
            std::env::remove_var("YNAB_AGENT_CLI_HOME");
            std::env::remove_var("YNAB_ACCESS_TOKEN");
        }
    }

    #[tokio::test]
    async fn update_transaction_uses_put_and_body() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let base_url = spawn_asserting_server_with_body(
            "PUT",
            "/v1/plans/default/transactions/tx-1",
            "\"memo\":\"Updated\"",
            json!({
                "data": {
                    "transaction": {
                        "id": "tx-1",
                        "memo": "Updated"
                    }
                }
            })
            .to_string(),
        );

        unsafe {
            std::env::set_var("YNAB_AGENT_CLI_HOME", temp_dir.path());
            std::env::set_var("YNAB_ACCESS_TOKEN", "test-token");
        }

        let mut app = AppState::load(RuntimeOptions {
            profile: None,
            use_keyring: false,
            base_url_override: Some(base_url),
            output_format: OutputFormat::Json,
            access_token_override: None,
            access_token_override_source: None,
        })
        .unwrap();

        let response = app
            .update_transaction(TransactionUpdateInput {
                plan_id: "default".to_string(),
                transaction_id: "tx-1".to_string(),
                amount: Some(AmountMilliunits(2500)),
                memo: Some("Updated".to_string()),
                ..TransactionUpdateInput::default()
            })
            .await
            .unwrap();
        assert_eq!(
            response
                .data
                .get("transaction")
                .and_then(|transaction| transaction.get("memo"))
                .and_then(Value::as_str),
            Some("Updated")
        );

        unsafe {
            std::env::remove_var("YNAB_AGENT_CLI_HOME");
            std::env::remove_var("YNAB_ACCESS_TOKEN");
        }
    }

    #[tokio::test]
    async fn create_transaction_includes_import_id_when_provided() {
        let _guard = crate::TEST_ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let base_url = spawn_asserting_server_with_body(
            "POST",
            "/v1/plans/default/transactions",
            "\"import_id\":\"YNAB:IMPORT:1\"",
            json!({
                "data": {
                    "transaction": {
                        "id": "tx-1",
                        "import_id": "YNAB:IMPORT:1"
                    }
                }
            })
            .to_string(),
        );

        unsafe {
            std::env::set_var("YNAB_AGENT_CLI_HOME", temp_dir.path());
            std::env::set_var("YNAB_ACCESS_TOKEN", "test-token");
        }

        let mut app = AppState::load(RuntimeOptions {
            profile: None,
            use_keyring: false,
            base_url_override: Some(base_url),
            output_format: OutputFormat::Json,
            access_token_override: None,
            access_token_override_source: None,
        })
        .unwrap();

        let response = app
            .create_transaction(TransactionCreateInput {
                plan_id: "default".to_string(),
                account_id: "account-1".to_string(),
                date: "2026-04-18".to_string(),
                amount: AmountMilliunits(2500),
                id: None,
                import_id: Some("YNAB:IMPORT:1".to_string()),
                payee_id: None,
                payee_name: None,
                category_id: None,
                memo: None,
                cleared: None,
                approved: None,
                flag_color: None,
                dry_run: false,
            })
            .await
            .unwrap();
        assert_eq!(
            response
                .data
                .get("transaction")
                .and_then(|transaction| transaction.get("import_id"))
                .and_then(Value::as_str),
            Some("YNAB:IMPORT:1")
        );

        unsafe {
            std::env::remove_var("YNAB_AGENT_CLI_HOME");
            std::env::remove_var("YNAB_ACCESS_TOKEN");
        }
    }

    fn spawn_json_server(body: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 4096];
            let _ = stream.read(&mut buffer);
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        format!("http://{address}/v1")
    }

    fn spawn_asserting_server(expected_path: &'static str, body: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 4096];
            let bytes_read = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..bytes_read]);
            let first_line = request.lines().next().unwrap_or_default();
            assert!(first_line.contains(expected_path), "{first_line}");
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        format!("http://{address}/v1")
    }

    fn spawn_asserting_server_with_body(
        expected_method: &'static str,
        expected_path: &'static str,
        expected_body_fragment: &'static str,
        body: String,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 8192];
            let bytes_read = stream.read(&mut buffer).unwrap();
            let request = String::from_utf8_lossy(&buffer[..bytes_read]);
            let first_line = request.lines().next().unwrap_or_default();
            assert!(first_line.starts_with(expected_method), "{first_line}");
            assert!(first_line.contains(expected_path), "{first_line}");
            assert!(
                request.contains(expected_body_fragment),
                "request body did not contain expected fragment: {request}"
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        format!("http://{address}/v1")
    }
}
