use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Result, YnabError};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct AmountMilliunits(pub i64);

impl AmountMilliunits {
    pub fn parse(input: &str) -> Result<Self> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(YnabError::InvalidAmount(
                "amount cannot be empty".to_string(),
            ));
        }

        let negative = trimmed.starts_with('-');
        let digits = if trimmed.starts_with('-') || trimmed.starts_with('+') {
            &trimmed[1..]
        } else {
            trimmed
        };

        let parts: Vec<&str> = digits.split('.').collect();
        if parts.len() > 2 {
            return Err(YnabError::InvalidAmount(
                "amount must have at most one decimal point".to_string(),
            ));
        }

        let whole = parts[0]
            .parse::<i64>()
            .map_err(|_| YnabError::InvalidAmount(format!("invalid whole amount: {input}")))?;
        let fractional = parts.get(1).copied().unwrap_or("0");
        if fractional.len() > 3 {
            return Err(YnabError::InvalidAmount(
                "amount must have at most three decimal places".to_string(),
            ));
        }

        let padded = format!("{fractional:0<3}");
        let frac = padded
            .parse::<i64>()
            .map_err(|_| YnabError::InvalidAmount(format!("invalid fractional amount: {input}")))?;
        let mut total = whole
            .checked_mul(1000)
            .and_then(|value| value.checked_add(frac))
            .ok_or_else(|| YnabError::InvalidAmount("amount overflow".to_string()))?;
        if negative {
            total *= -1;
        }
        Ok(Self(total))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OAuthScope {
    ReadOnly,
    #[default]
    FullAccess,
}

impl OAuthScope {
    pub fn as_api_scope(&self) -> Option<&'static str> {
        match self {
            Self::ReadOnly => Some("read-only"),
            Self::FullAccess => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StoredSession {
    PersonalAccessToken {
        access_token: String,
    },
    OAuth {
        access_token: String,
        refresh_token: String,
        expires_at: DateTime<Utc>,
        token_type: String,
        scope: Option<String>,
        client_id: String,
        client_secret: String,
        redirect_uri: String,
    },
}

impl StoredSession {
    pub fn bearer_token(&self) -> &str {
        match self {
            Self::PersonalAccessToken { access_token } => access_token,
            Self::OAuth { access_token, .. } => access_token,
        }
    }

    pub fn is_oauth(&self) -> bool {
        matches!(self, Self::OAuth { .. })
    }

    pub fn needs_refresh(&self) -> bool {
        match self {
            Self::PersonalAccessToken { .. } => false,
            Self::OAuth { expires_at, .. } => *expires_at <= Utc::now() + Duration::seconds(60),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct OutputEnvelope {
    pub ok: bool,
    pub data: Value,
}

pub type ApiSuccessEnvelope = OutputEnvelope;

#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
    pub data: T,
}

#[derive(Debug, Deserialize)]
pub struct ApiErrorResponse {
    pub error: ApiErrorBody,
}

#[derive(Debug, Deserialize)]
pub struct ApiErrorBody {
    pub id: String,
    pub name: String,
    pub detail: String,
}

#[derive(Debug, Deserialize)]
pub struct PlansData {
    #[serde(default)]
    pub plans: Vec<PlanSummary>,
    #[serde(default)]
    pub default_plan: Option<PlanSummary>,
    #[serde(default)]
    pub server_knowledge: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct PlanData {
    pub plan: Value,
}

#[derive(Debug, Deserialize)]
pub struct AccountsData {
    #[serde(default)]
    pub accounts: Vec<NamedResource>,
    #[serde(default)]
    pub server_knowledge: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct PayeesData {
    #[serde(default)]
    pub payees: Vec<NamedResource>,
    #[serde(default)]
    pub server_knowledge: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct TransactionsData {
    #[serde(default)]
    pub transactions: Vec<Value>,
    #[serde(default)]
    pub server_knowledge: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
pub enum TransactionClearedFilter {
    Cleared,
    Uncleared,
}

#[derive(Debug, Deserialize)]
pub struct CategoryGroupsData {
    #[serde(default)]
    pub category_groups: Vec<CategoryGroup>,
    #[serde(default)]
    pub server_knowledge: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct CategoryGroup {
    #[serde(default)]
    pub categories: Vec<NamedResource>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlanSummary {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub last_modified_on: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NamedResource {
    pub id: String,
    pub name: String,
}

impl From<PlanSummary> for NamedResource {
    fn from(value: PlanSummary) -> Self {
        Self {
            id: value.id,
            name: value.name,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SaveTransactionRequest {
    pub transaction: SaveTransaction,
}

#[derive(Debug, Serialize)]
pub struct PostPayeeWrapper {
    pub payee: PostPayee,
}

#[derive(Debug, Serialize)]
pub struct PostPayee {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct PatchPayeeWrapper {
    pub payee: SavePayee,
}

#[derive(Debug, Serialize)]
pub struct SavePayee {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PostCategoryWrapper {
    pub category: SaveCategory,
}

#[derive(Debug, Serialize)]
pub struct PatchCategoryWrapper {
    pub category: SaveCategory,
}

#[derive(Debug, Serialize, Default)]
pub struct SaveCategory {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_group_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_target: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_target_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_needs_whole_amount: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct PostCategoryGroupWrapper {
    pub category_group: SaveCategoryGroup,
}

#[derive(Debug, Serialize)]
pub struct PatchCategoryGroupWrapper {
    pub category_group: SaveCategoryGroup,
}

#[derive(Debug, Serialize)]
pub struct SaveCategoryGroup {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct PatchMonthCategoryWrapper {
    pub category: SaveMonthCategory,
}

#[derive(Debug, Serialize)]
pub struct SaveMonthCategory {
    pub budgeted: i64,
}

#[derive(Debug, Serialize)]
pub struct SaveTransaction {
    pub account_id: String,
    pub date: String,
    pub amount: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleared: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flag_color: Option<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct UpdateTransactionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction: Option<UpdateTransaction>,
}

#[derive(Debug, Serialize, Default)]
pub struct UpdateTransaction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleared: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flag_color: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::AmountMilliunits;

    #[test]
    fn parses_whole_and_fractional_amounts() {
        assert_eq!(AmountMilliunits::parse("1").unwrap().0, 1000);
        assert_eq!(AmountMilliunits::parse("1.2").unwrap().0, 1200);
        assert_eq!(AmountMilliunits::parse("1.23").unwrap().0, 1230);
        assert_eq!(AmountMilliunits::parse("1.234").unwrap().0, 1234);
        assert_eq!(AmountMilliunits::parse("-0.001").unwrap().0, -1);
    }

    #[test]
    fn rejects_more_than_three_decimals() {
        assert!(AmountMilliunits::parse("1.2345").is_err());
    }
}
