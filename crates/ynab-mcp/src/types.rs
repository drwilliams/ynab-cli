use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::Deserialize;
use ynab_core::{
    AmountMilliunits, ResourceListOptions, SaveCategory, TransactionClearedFilter,
    TransactionListOptions, TransactionSearchOptions,
};

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct GetUserParams {}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccountTypeParam {
    Checking,
    Savings,
    Cash,
    CreditCard,
    OtherAsset,
    OtherLiability,
}

impl AccountTypeParam {
    pub fn as_api_value(&self) -> &'static str {
        match self {
            Self::Checking => "checking",
            Self::Savings => "savings",
            Self::Cash => "cash",
            Self::CreditCard => "creditCard",
            Self::OtherAsset => "otherAsset",
            Self::OtherLiability => "otherLiability",
        }
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScheduledFrequencyParam {
    Never,
    Daily,
    Weekly,
    EveryOtherWeek,
    TwiceAMonth,
    Every4Weeks,
    Monthly,
    EveryOtherMonth,
    Every3Months,
    Every4Months,
    TwiceAYear,
    Yearly,
    EveryOtherYear,
}

impl ScheduledFrequencyParam {
    pub fn as_api_value(&self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::EveryOtherWeek => "everyOtherWeek",
            Self::TwiceAMonth => "twiceAMonth",
            Self::Every4Weeks => "every4Weeks",
            Self::Monthly => "monthly",
            Self::EveryOtherMonth => "everyOtherMonth",
            Self::Every3Months => "every3Months",
            Self::Every4Months => "every4Months",
            Self::TwiceAYear => "twiceAYear",
            Self::Yearly => "yearly",
            Self::EveryOtherYear => "everyOtherYear",
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlanIdParam {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlanGetParam {
    #[schemars(description = "Plan identifier.")]
    pub plan_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AccountCreateParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    pub name: String,
    pub account_type: AccountTypeParam,
    #[schemars(description = "Opening balance as a decimal amount.")]
    pub balance: String,
    #[schemars(description = "Optional cleared balance as a decimal amount.")]
    pub cleared_balance: Option<String>,
    #[schemars(description = "Optional uncleared balance as a decimal amount.")]
    pub uncleared_balance: Option<String>,
    pub transfer_payee_id: Option<String>,
    pub note: Option<String>,
    pub on_budget: Option<bool>,
    pub closed: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ResourceListParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    #[schemars(description = "Optional last-known server value for incremental list queries.")]
    pub last_knowledge_of_server: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListAccountsParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    #[schemars(description = "Optional last-known server value for incremental list queries.")]
    pub last_knowledge_of_server: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListPlansParams {
    #[schemars(description = "Include plan accounts in the list response.")]
    pub include_accounts: bool,
    #[schemars(description = "Optional last-known server value for incremental list queries.")]
    pub last_knowledge_of_server: Option<u64>,
}

impl Default for ListPlansParams {
    fn default() -> Self {
        Self {
            include_accounts: false,
            last_knowledge_of_server: None,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AccountGetParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    #[schemars(description = "Account identifier.")]
    pub account_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CategoryGetParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    #[schemars(description = "Category identifier.")]
    pub category_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CategoryCreateParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    pub name: String,
    pub group_id: String,
    pub note: Option<String>,
    #[schemars(description = "Optional goal target as a decimal amount.")]
    pub goal_target: Option<String>,
    #[schemars(description = "Optional goal target date in YYYY-MM-DD format.")]
    pub goal_target_date: Option<String>,
    pub goal_needs_whole_amount: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CategoryUpdateParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    pub category_id: String,
    pub name: Option<String>,
    pub group_id: Option<String>,
    pub note: Option<String>,
    #[schemars(description = "Optional goal target as a decimal amount.")]
    pub goal_target: Option<String>,
    #[schemars(description = "Optional goal target date in YYYY-MM-DD format.")]
    pub goal_target_date: Option<String>,
    pub goal_needs_whole_amount: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CategoryUpdateMonthParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    pub month: String,
    pub category_id: String,
    #[schemars(description = "Budgeted amount as a decimal amount.")]
    pub budgeted: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CategoryGroupCreateParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CategoryGroupUpdateParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    pub category_group_id: String,
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PayeeCreateParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PayeeUpdateParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    pub payee_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClearedFilterParam {
    Cleared,
    Uncleared,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TransactionsListParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    #[schemars(description = "Optional last-known server value for incremental list queries.")]
    pub last_knowledge_of_server: Option<u64>,
    #[schemars(description = "Optional month in YYYY-MM format.")]
    pub month: Option<String>,
    #[schemars(description = "Optional YNAB since_date query value in YYYY-MM-DD format.")]
    pub since_date: Option<String>,
    #[schemars(description = "Optional transaction type query value such as unapproved.")]
    pub transaction_type: Option<String>,
    #[schemars(description = "Optional local start date filter in YYYY-MM-DD format.")]
    pub start_date: Option<String>,
    #[schemars(description = "Optional local end date filter in YYYY-MM-DD format.")]
    pub end_date: Option<String>,
    #[schemars(description = "Optional cleared-state filter.")]
    pub cleared_filter: Option<ClearedFilterParam>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TransactionsSearchParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    #[schemars(description = "Optional last-known server value for incremental list queries.")]
    pub last_knowledge_of_server: Option<u64>,
    #[schemars(description = "Optional month in YYYY-MM format.")]
    pub month: Option<String>,
    #[schemars(description = "Optional YNAB since_date query value in YYYY-MM-DD format.")]
    pub since_date: Option<String>,
    #[schemars(description = "Optional transaction type query value such as unapproved.")]
    pub transaction_type: Option<String>,
    #[schemars(description = "Optional local start date filter in YYYY-MM-DD format.")]
    pub start_date: Option<String>,
    #[schemars(description = "Optional local end date filter in YYYY-MM-DD format.")]
    pub end_date: Option<String>,
    #[schemars(description = "Optional cleared-state filter.")]
    pub cleared_filter: Option<ClearedFilterParam>,
    #[schemars(description = "Free-text search across payee, memo, account, and category.")]
    pub query: Option<String>,
    #[schemars(description = "Filter transactions by payee name substring.")]
    pub payee: Option<String>,
    #[schemars(description = "Filter transactions by memo substring.")]
    pub memo: Option<String>,
    #[schemars(description = "Filter transactions by account name substring.")]
    pub account: Option<String>,
    #[schemars(description = "Filter transactions by category name substring.")]
    pub category: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TransactionGetParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    #[schemars(description = "Transaction identifier.")]
    pub transaction_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TransactionCreateParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub date: String,
    #[schemars(description = "Amount as a decimal string.")]
    pub amount: String,
    pub payee_id: Option<String>,
    pub payee_name: Option<String>,
    pub category_id: Option<String>,
    pub category_name: Option<String>,
    pub memo: Option<String>,
    pub cleared: Option<String>,
    pub approved: Option<bool>,
    pub flag_color: Option<String>,
    pub import_id: Option<String>,
    pub id: Option<String>,
    pub dry_run: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TransactionUpdateParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    pub transaction_id: String,
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub date: Option<String>,
    #[schemars(description = "Amount as a decimal string.")]
    pub amount: Option<String>,
    pub payee_id: Option<String>,
    pub payee_name: Option<String>,
    pub category_id: Option<String>,
    pub category_name: Option<String>,
    pub memo: Option<String>,
    pub cleared: Option<String>,
    pub approved: Option<bool>,
    pub flag_color: Option<String>,
    pub dry_run: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TransactionDeleteParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    pub transaction_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TransactionJsonRequestParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    #[schemars(description = "JSON payload matching the corresponding YNAB bulk/import request body.")]
    pub request: serde_json::Value,
    pub dry_run: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AccountTransactionsParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    #[schemars(description = "Account identifier.")]
    pub account_id: String,
    #[serde(flatten)]
    pub list: TransactionsListParams,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CategoryTransactionsParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    #[schemars(description = "Category identifier.")]
    pub category_id: String,
    #[serde(flatten)]
    pub list: TransactionsListParams,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PayeeTransactionsParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    #[schemars(description = "Payee identifier.")]
    pub payee_id: String,
    #[serde(flatten)]
    pub list: TransactionsListParams,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MonthGetParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    #[schemars(description = "Month in YYYY-MM format.")]
    pub month: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScheduledTransactionGetParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    #[schemars(description = "Scheduled transaction identifier.")]
    pub scheduled_transaction_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScheduledTransactionCreateParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub date: String,
    #[schemars(description = "Amount as a decimal string.")]
    pub amount: String,
    pub payee_id: Option<String>,
    pub payee_name: Option<String>,
    pub category_id: Option<String>,
    pub category_name: Option<String>,
    pub memo: Option<String>,
    pub flag_color: Option<String>,
    pub frequency: ScheduledFrequencyParam,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScheduledTransactionUpdateParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    pub scheduled_transaction_id: String,
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub date: Option<String>,
    #[schemars(description = "Amount as a decimal string.")]
    pub amount: Option<String>,
    pub payee_id: Option<String>,
    pub payee_name: Option<String>,
    pub category_id: Option<String>,
    pub category_name: Option<String>,
    pub memo: Option<String>,
    pub flag_color: Option<String>,
    pub frequency: Option<ScheduledFrequencyParam>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ScheduledTransactionDeleteParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    pub scheduled_transaction_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MonthScopedListParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    #[schemars(description = "Month in YYYY-MM format.")]
    pub month: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PayeeLocationGetParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    #[schemars(description = "Payee location identifier.")]
    pub payee_location_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PayeeLocationsByPayeeParams {
    #[schemars(description = "Optional plan identifier. When omitted, the saved default plan is used.")]
    pub plan_id: Option<String>,
    #[schemars(description = "Payee identifier.")]
    pub payee_id: String,
}

impl ResourceListParams {
    pub fn as_options(&self) -> ResourceListOptions {
        ResourceListOptions {
            last_knowledge_of_server: self.last_knowledge_of_server,
        }
    }
}

impl TransactionsListParams {
    pub fn to_options(&self) -> Result<TransactionListOptions, String> {
        Ok(TransactionListOptions {
            last_knowledge_of_server: self.last_knowledge_of_server,
            month: self.month.clone(),
            since_date: self.since_date.clone(),
            transaction_type: self.transaction_type.clone(),
            start_date: parse_optional_date(self.start_date.as_deref())?,
            end_date: parse_optional_date(self.end_date.as_deref())?,
            cleared_filter: self.cleared_filter.clone().map(Into::into),
        })
    }
}

impl TransactionsSearchParams {
    pub fn to_list_options(&self) -> Result<TransactionListOptions, String> {
        Ok(TransactionListOptions {
            last_knowledge_of_server: self.last_knowledge_of_server,
            month: self.month.clone(),
            since_date: self.since_date.clone(),
            transaction_type: self.transaction_type.clone(),
            start_date: parse_optional_date(self.start_date.as_deref())?,
            end_date: parse_optional_date(self.end_date.as_deref())?,
            cleared_filter: self.cleared_filter.clone().map(Into::into),
        })
    }

    pub fn to_search_options(&self) -> TransactionSearchOptions {
        TransactionSearchOptions {
            query: self.query.clone(),
            payee: self.payee.clone(),
            memo: self.memo.clone(),
            account: self.account.clone(),
            category: self.category.clone(),
        }
    }
}

impl AccountCreateParams {
    pub fn to_payload(&self) -> Result<serde_json::Value, String> {
        let mut account = serde_json::Map::new();
        account.insert("name".to_string(), serde_json::Value::String(self.name.clone()));
        account.insert(
            "type".to_string(),
            serde_json::Value::String(self.account_type.as_api_value().to_string()),
        );
        account.insert(
            "balance".to_string(),
            serde_json::Value::from(parse_amount(&self.balance)?.0),
        );
        if let Some(value) = self.cleared_balance.as_deref() {
            account.insert(
                "cleared_balance".to_string(),
                serde_json::Value::from(parse_amount(value)?.0),
            );
        }
        if let Some(value) = self.uncleared_balance.as_deref() {
            account.insert(
                "uncleared_balance".to_string(),
                serde_json::Value::from(parse_amount(value)?.0),
            );
        }
        if let Some(value) = self.transfer_payee_id.as_ref() {
            account.insert("transfer_payee_id".to_string(), serde_json::Value::String(value.clone()));
        }
        if let Some(value) = self.note.as_ref() {
            account.insert("note".to_string(), serde_json::Value::String(value.clone()));
        }
        if let Some(value) = self.on_budget {
            account.insert("on_budget".to_string(), serde_json::Value::Bool(value));
        }
        if let Some(value) = self.closed {
            account.insert("closed".to_string(), serde_json::Value::Bool(value));
        }
        Ok(serde_json::Value::Object(account))
    }
}

impl CategoryCreateParams {
    pub fn to_save_category(&self) -> Result<SaveCategory, String> {
        Ok(SaveCategory {
            name: Some(self.name.clone()),
            note: self.note.clone(),
            category_group_id: Some(self.group_id.clone()),
            goal_target: self.goal_target.as_deref().map(parse_amount).transpose()?.map(|a| a.0),
            goal_target_date: self.goal_target_date.as_deref().map(normalize_iso_date).transpose()?,
            goal_needs_whole_amount: self.goal_needs_whole_amount,
        })
    }
}

impl CategoryUpdateParams {
    pub fn to_save_category(&self) -> Result<SaveCategory, String> {
        let payload = SaveCategory {
            name: self.name.clone(),
            note: self.note.clone(),
            category_group_id: self.group_id.clone(),
            goal_target: self.goal_target.as_deref().map(parse_amount).transpose()?.map(|a| a.0),
            goal_target_date: self.goal_target_date.as_deref().map(normalize_iso_date).transpose()?,
            goal_needs_whole_amount: self.goal_needs_whole_amount,
        };
        if payload.name.is_none()
            && payload.note.is_none()
            && payload.category_group_id.is_none()
            && payload.goal_target.is_none()
            && payload.goal_target_date.is_none()
            && payload.goal_needs_whole_amount.is_none()
        {
            return Err("categories update requires at least one field to change".to_string());
        }
        Ok(payload)
    }
}

impl CategoryUpdateMonthParams {
    pub fn budgeted_milliunits(&self) -> Result<i64, String> {
        Ok(parse_amount(&self.budgeted)?.0)
    }
}

impl TransactionCreateParams {
    pub fn dry_run(&self) -> bool {
        self.dry_run.unwrap_or(false)
    }
}

impl TransactionUpdateParams {
    pub fn dry_run(&self) -> bool {
        self.dry_run.unwrap_or(false)
    }
}

impl TransactionJsonRequestParams {
    pub fn dry_run(&self) -> bool {
        self.dry_run.unwrap_or(false)
    }
}

pub fn normalize_iso_date(input: &str) -> Result<String, String> {
    Ok(parse_date(input)?.format("%Y-%m-%d").to_string())
}

pub fn parse_date(input: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(input.trim(), "%Y-%m-%d")
        .map_err(|_| format!("invalid date `{}`. use YYYY-MM-DD", input.trim()))
}

pub fn parse_amount(input: &str) -> Result<AmountMilliunits, String> {
    AmountMilliunits::parse(input).map_err(|error| error.to_string())
}

impl From<ClearedFilterParam> for TransactionClearedFilter {
    fn from(value: ClearedFilterParam) -> Self {
        match value {
            ClearedFilterParam::Cleared => TransactionClearedFilter::Cleared,
            ClearedFilterParam::Uncleared => TransactionClearedFilter::Uncleared,
        }
    }
}

fn parse_optional_date(value: Option<&str>) -> Result<Option<NaiveDate>, String> {
    value
        .map(|value| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|error| format!("invalid date {value}: {error}"))
        })
        .transpose()
}
