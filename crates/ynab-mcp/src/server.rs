use std::sync::Arc;

use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};
use tokio::sync::Mutex;
use ynab_core::{AppState, ResolveByNameKind, TransactionCreateInput, TransactionUpdateInput};

use crate::types::{
    AccountCreateParams, AccountGetParams, AccountTransactionsParams, CategoryCreateParams,
    CategoryGetParams, CategoryGroupCreateParams, CategoryGroupUpdateParams,
    CategoryTransactionsParams, CategoryUpdateMonthParams, CategoryUpdateParams, GetUserParams,
    ListAccountsParams, ListPlansParams, MonthGetParams, MonthScopedListParams, PayeeCreateParams,
    PayeeLocationGetParams, PayeeLocationsByPayeeParams, PayeeTransactionsParams,
    PayeeUpdateParams, PlanGetParam, PlanIdParam, ResourceListParams,
    ScheduledTransactionCreateParams, ScheduledTransactionDeleteParams,
    ScheduledTransactionGetParams, ScheduledTransactionUpdateParams, TransactionCreateParams,
    TransactionDeleteParams, TransactionGetParams, TransactionJsonRequestParams,
    TransactionUpdateParams, TransactionsListParams, TransactionsSearchParams, normalize_iso_date,
    parse_amount,
};

#[derive(Clone)]
pub struct YnabMcpServer {
    pub app: Arc<Mutex<AppState>>,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

impl YnabMcpServer {
    pub fn new(app: Arc<Mutex<AppState>>) -> Self {
        Self {
            app,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router(router = tool_router)]
impl YnabMcpServer {
    #[tool(description = "Show the active YNAB authentication and runtime status.")]
    pub async fn ynab_auth_status(&self) -> Result<String, String> {
        let app = self.app.lock().await;
        crate::tools::render_json(app.auth_status())
    }

    #[tool(description = "Get a plan by identifier.")]
    pub async fn ynab_get_plan(&self, params: Parameters<PlanGetParam>) -> Result<String, String> {
        let mut app = self.app.lock().await;
        crate::tools::render_json(app.get_plan(&params.0.plan_id).await)
    }

    #[tool(description = "Get plan settings by plan identifier.")]
    pub async fn ynab_get_plan_settings(
        &self,
        params: Parameters<PlanGetParam>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        crate::tools::render_json(app.get_plan_settings(&params.0.plan_id).await)
    }

    #[tool(description = "Set the default plan for the active profile.")]
    pub async fn ynab_set_default_plan(
        &self,
        params: Parameters<PlanGetParam>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        crate::tools::render_json(app.set_default_plan(&params.0.plan_id))
    }

    #[tool(description = "List visible plans for the current YNAB session.")]
    pub async fn ynab_list_plans(
        &self,
        params: Parameters<ListPlansParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        crate::tools::render_json(
            app.list_plans_with_include_accounts(
                ynab_core::ResourceListOptions {
                    last_knowledge_of_server: params.0.last_knowledge_of_server,
                },
                params.0.include_accounts,
            )
            .await,
        )
    }

    #[tool(
        description = "List accounts for a plan. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_list_accounts(
        &self,
        params: Parameters<ListAccountsParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(
            app.list_accounts(
                &plan_id,
                ynab_core::ResourceListOptions {
                    last_knowledge_of_server: params.0.last_knowledge_of_server,
                },
            )
            .await,
        )
    }

    #[tool(
        description = "Get an account by identifier. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_get_account(
        &self,
        params: Parameters<AccountGetParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(app.get_account(&plan_id, &params.0.account_id).await)
    }

    #[tool(description = "Create an account.")]
    pub async fn ynab_create_account(
        &self,
        params: Parameters<AccountCreateParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        let payload = params.0.to_payload()?;
        crate::tools::render_json(app.create_account(&plan_id, payload).await)
    }

    #[tool(
        description = "List categories for a plan. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_list_categories(
        &self,
        params: Parameters<ResourceListParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(app.list_categories(&plan_id, params.0.as_options()).await)
    }

    #[tool(
        description = "Get a category by identifier. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_get_category(
        &self,
        params: Parameters<CategoryGetParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(app.get_category(&plan_id, &params.0.category_id).await)
    }

    #[tool(description = "Create a category.")]
    pub async fn ynab_create_category(
        &self,
        params: Parameters<CategoryCreateParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        let payload = params.0.to_save_category()?;
        crate::tools::render_json(app.create_category(&plan_id, payload).await)
    }

    #[tool(description = "Update a category.")]
    pub async fn ynab_update_category(
        &self,
        params: Parameters<CategoryUpdateParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        let payload = params.0.to_save_category()?;
        crate::tools::render_json(
            app.update_category(&plan_id, &params.0.category_id, payload)
                .await,
        )
    }

    #[tool(description = "Update a category budget for a month.")]
    pub async fn ynab_update_month_category(
        &self,
        params: Parameters<CategoryUpdateMonthParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        let budgeted = params.0.budgeted_milliunits()?;
        crate::tools::render_json(
            app.update_month_category(&plan_id, &params.0.month, &params.0.category_id, budgeted)
                .await,
        )
    }

    #[tool(description = "Create a category group.")]
    pub async fn ynab_create_category_group(
        &self,
        params: Parameters<CategoryGroupCreateParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(app.create_category_group(&plan_id, params.0.name).await)
    }

    #[tool(description = "Update a category group.")]
    pub async fn ynab_update_category_group(
        &self,
        params: Parameters<CategoryGroupUpdateParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(
            app.update_category_group(&plan_id, &params.0.category_group_id, params.0.name)
                .await,
        )
    }

    #[tool(
        description = "List payees for a plan. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_list_payees(
        &self,
        params: Parameters<ResourceListParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(app.list_payees(&plan_id, params.0.as_options()).await)
    }

    #[tool(description = "Create a payee.")]
    pub async fn ynab_create_payee(
        &self,
        params: Parameters<PayeeCreateParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(app.create_payee(&plan_id, params.0.name).await)
    }

    #[tool(description = "Update a payee.")]
    pub async fn ynab_update_payee(
        &self,
        params: Parameters<PayeeUpdateParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(
            app.update_payee(&plan_id, &params.0.payee_id, params.0.name)
                .await,
        )
    }

    #[tool(
        description = "List transactions for a plan. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_list_transactions(
        &self,
        params: Parameters<TransactionsListParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        let options = params.0.to_options()?;
        crate::tools::render_json(app.list_transactions(&plan_id, options).await)
    }

    #[tool(
        description = "Search transactions by query and/or payee, memo, account, or category filters."
    )]
    pub async fn ynab_search_transactions(
        &self,
        params: Parameters<TransactionsSearchParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        let list_options = params.0.to_list_options()?;
        let search_options = params.0.to_search_options();
        crate::tools::render_json(
            app.search_transactions(&plan_id, list_options, search_options)
                .await,
        )
    }

    #[tool(
        description = "Get a transaction by identifier. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_get_transaction(
        &self,
        params: Parameters<TransactionGetParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(
            app.get_transaction(&plan_id, &params.0.transaction_id)
                .await,
        )
    }

    #[tool(description = "Create a transaction.")]
    pub async fn ynab_create_transaction(
        &self,
        params: Parameters<TransactionCreateParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        let account_id = resolve_named_or_explicit(
            &mut app,
            ResolveByNameKind::Account,
            &plan_id,
            params.0.account_id.clone(),
            params.0.account_name.clone(),
        )
        .await?;
        let category_id = resolve_optional_named_or_explicit(
            &mut app,
            ResolveByNameKind::Category,
            &plan_id,
            params.0.category_id.clone(),
            params.0.category_name.clone(),
        )
        .await?;
        let payee_id = resolve_optional_named_or_explicit(
            &mut app,
            ResolveByNameKind::Payee,
            &plan_id,
            params.0.payee_id.clone(),
            None,
        )
        .await?;
        let input = TransactionCreateInput {
            plan_id,
            account_id,
            date: normalize_iso_date(&params.0.date)?,
            amount: parse_amount(&params.0.amount)?,
            id: params.0.id.clone(),
            import_id: params.0.import_id.clone(),
            payee_id,
            payee_name: params.0.payee_name.clone(),
            category_id,
            memo: params.0.memo.clone(),
            cleared: params.0.cleared.clone(),
            approved: params.0.approved,
            flag_color: params.0.flag_color.clone(),
            dry_run: params.0.dry_run(),
        };
        crate::tools::render_json(app.create_transaction(input).await)
    }

    #[tool(description = "Update a transaction.")]
    pub async fn ynab_update_transaction(
        &self,
        params: Parameters<TransactionUpdateParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        let account_id = resolve_optional_named_or_explicit(
            &mut app,
            ResolveByNameKind::Account,
            &plan_id,
            params.0.account_id.clone(),
            params.0.account_name.clone(),
        )
        .await?;
        let category_id = resolve_optional_named_or_explicit(
            &mut app,
            ResolveByNameKind::Category,
            &plan_id,
            params.0.category_id.clone(),
            params.0.category_name.clone(),
        )
        .await?;
        let payee_id = resolve_optional_named_or_explicit(
            &mut app,
            ResolveByNameKind::Payee,
            &plan_id,
            params.0.payee_id.clone(),
            None,
        )
        .await?;
        let input = TransactionUpdateInput {
            plan_id,
            transaction_id: params.0.transaction_id.clone(),
            account_id,
            date: params
                .0
                .date
                .as_deref()
                .map(normalize_iso_date)
                .transpose()?,
            amount: params.0.amount.as_deref().map(parse_amount).transpose()?,
            payee_id,
            payee_name: params.0.payee_name.clone(),
            category_id,
            memo: params.0.memo.clone(),
            cleared: params.0.cleared.clone(),
            approved: params.0.approved,
            flag_color: params.0.flag_color.clone(),
            dry_run: params.0.dry_run(),
        };
        crate::tools::render_json(app.update_transaction(input).await)
    }

    #[tool(description = "Delete a transaction.")]
    pub async fn ynab_delete_transaction(
        &self,
        params: Parameters<TransactionDeleteParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(
            app.delete_transaction(&plan_id, &params.0.transaction_id)
                .await,
        )
    }

    #[tool(description = "Create transactions in bulk from a JSON request body.")]
    pub async fn ynab_create_transactions_bulk(
        &self,
        params: Parameters<TransactionJsonRequestParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(
            app.create_transactions_bulk(&plan_id, params.0.request.clone(), params.0.dry_run())
                .await,
        )
    }

    #[tool(description = "Update transactions in bulk from a JSON request body.")]
    pub async fn ynab_update_transactions_bulk(
        &self,
        params: Parameters<TransactionJsonRequestParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(
            app.update_transactions_bulk(&plan_id, params.0.request.clone(), params.0.dry_run())
                .await,
        )
    }

    #[tool(description = "Import transactions from a JSON request body.")]
    pub async fn ynab_import_transactions(
        &self,
        params: Parameters<TransactionJsonRequestParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(
            app.import_transactions(&plan_id, params.0.request.clone(), params.0.dry_run())
                .await,
        )
    }

    #[tool(
        description = "List transactions for an account. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_list_transactions_by_account(
        &self,
        params: Parameters<AccountTransactionsParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        let options = params.0.list.to_options()?;
        crate::tools::render_json(
            app.list_transactions_by_account(&plan_id, &params.0.account_id, options)
                .await,
        )
    }

    #[tool(
        description = "List transactions for a category. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_list_transactions_by_category(
        &self,
        params: Parameters<CategoryTransactionsParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        let options = params.0.list.to_options()?;
        crate::tools::render_json(
            app.list_transactions_by_category(&plan_id, &params.0.category_id, options)
                .await,
        )
    }

    #[tool(
        description = "List transactions for a payee. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_list_transactions_by_payee(
        &self,
        params: Parameters<PayeeTransactionsParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        let options = params.0.list.to_options()?;
        crate::tools::render_json(
            app.list_transactions_by_payee(&plan_id, &params.0.payee_id, options)
                .await,
        )
    }

    #[tool(
        description = "List months for a plan. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_list_months(
        &self,
        params: Parameters<PlanIdParam>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(app.list_months(&plan_id).await)
    }

    #[tool(
        description = "Get a month by YYYY-MM value. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_get_month(
        &self,
        params: Parameters<MonthGetParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(app.get_month(&plan_id, &params.0.month).await)
    }

    #[tool(
        description = "List scheduled transactions for a plan. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_list_scheduled_transactions(
        &self,
        params: Parameters<ResourceListParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(
            app.list_scheduled_transactions(&plan_id, params.0.as_options())
                .await,
        )
    }

    #[tool(
        description = "Get a scheduled transaction by identifier. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_get_scheduled_transaction(
        &self,
        params: Parameters<ScheduledTransactionGetParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(
            app.get_scheduled_transaction(&plan_id, &params.0.scheduled_transaction_id)
                .await,
        )
    }

    #[tool(description = "Create a scheduled transaction.")]
    pub async fn ynab_create_scheduled_transaction(
        &self,
        params: Parameters<ScheduledTransactionCreateParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        let account_id = resolve_named_or_explicit(
            &mut app,
            ResolveByNameKind::Account,
            &plan_id,
            params.0.account_id.clone(),
            params.0.account_name.clone(),
        )
        .await?;
        let category_id = resolve_optional_named_or_explicit(
            &mut app,
            ResolveByNameKind::Category,
            &plan_id,
            params.0.category_id.clone(),
            params.0.category_name.clone(),
        )
        .await?;
        let payee_id = resolve_optional_named_or_explicit(
            &mut app,
            ResolveByNameKind::Payee,
            &plan_id,
            params.0.payee_id.clone(),
            None,
        )
        .await?;

        let mut scheduled_transaction = serde_json::Map::new();
        scheduled_transaction.insert(
            "account_id".to_string(),
            serde_json::Value::String(account_id),
        );
        scheduled_transaction.insert(
            "date".to_string(),
            serde_json::Value::String(normalize_iso_date(&params.0.date)?),
        );
        scheduled_transaction.insert(
            "amount".to_string(),
            serde_json::Value::from(parse_amount(&params.0.amount)?.0),
        );
        scheduled_transaction.insert(
            "frequency".to_string(),
            serde_json::Value::String(params.0.frequency.as_api_value().to_string()),
        );
        if let Some(value) = payee_id {
            scheduled_transaction.insert("payee_id".to_string(), serde_json::Value::String(value));
        }
        if let Some(value) = params.0.payee_name.as_ref() {
            scheduled_transaction.insert(
                "payee_name".to_string(),
                serde_json::Value::String(value.clone()),
            );
        }
        if let Some(value) = category_id {
            scheduled_transaction
                .insert("category_id".to_string(), serde_json::Value::String(value));
        }
        if let Some(value) = params.0.memo.as_ref() {
            scheduled_transaction
                .insert("memo".to_string(), serde_json::Value::String(value.clone()));
        }
        if let Some(value) = params.0.flag_color.as_ref() {
            scheduled_transaction.insert(
                "flag_color".to_string(),
                serde_json::Value::String(value.clone()),
            );
        }
        crate::tools::render_json(
            app.create_scheduled_transaction(
                &plan_id,
                serde_json::Value::Object(scheduled_transaction),
            )
            .await,
        )
    }

    #[tool(description = "Update a scheduled transaction.")]
    pub async fn ynab_update_scheduled_transaction(
        &self,
        params: Parameters<ScheduledTransactionUpdateParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        let account_id = resolve_optional_named_or_explicit(
            &mut app,
            ResolveByNameKind::Account,
            &plan_id,
            params.0.account_id.clone(),
            params.0.account_name.clone(),
        )
        .await?;
        let category_id = resolve_optional_named_or_explicit(
            &mut app,
            ResolveByNameKind::Category,
            &plan_id,
            params.0.category_id.clone(),
            params.0.category_name.clone(),
        )
        .await?;
        let payee_id = resolve_optional_named_or_explicit(
            &mut app,
            ResolveByNameKind::Payee,
            &plan_id,
            params.0.payee_id.clone(),
            None,
        )
        .await?;

        let mut scheduled_transaction = serde_json::Map::new();
        if let Some(value) = account_id {
            scheduled_transaction
                .insert("account_id".to_string(), serde_json::Value::String(value));
        }
        if let Some(value) = params.0.date.as_deref() {
            scheduled_transaction.insert(
                "date".to_string(),
                serde_json::Value::String(normalize_iso_date(value)?),
            );
        }
        if let Some(value) = params.0.amount.as_deref() {
            scheduled_transaction.insert(
                "amount".to_string(),
                serde_json::Value::from(parse_amount(value)?.0),
            );
        }
        if let Some(value) = payee_id {
            scheduled_transaction.insert("payee_id".to_string(), serde_json::Value::String(value));
        }
        if let Some(value) = params.0.payee_name.as_ref() {
            scheduled_transaction.insert(
                "payee_name".to_string(),
                serde_json::Value::String(value.clone()),
            );
        }
        if let Some(value) = category_id {
            scheduled_transaction
                .insert("category_id".to_string(), serde_json::Value::String(value));
        }
        if let Some(value) = params.0.memo.as_ref() {
            scheduled_transaction
                .insert("memo".to_string(), serde_json::Value::String(value.clone()));
        }
        if let Some(value) = params.0.flag_color.as_ref() {
            scheduled_transaction.insert(
                "flag_color".to_string(),
                serde_json::Value::String(value.clone()),
            );
        }
        if let Some(value) = params.0.frequency.as_ref() {
            scheduled_transaction.insert(
                "frequency".to_string(),
                serde_json::Value::String(value.as_api_value().to_string()),
            );
        }
        if scheduled_transaction.is_empty() {
            return Err(
                "scheduled-transactions update requires at least one field to change".to_string(),
            );
        }
        crate::tools::render_json(
            app.update_scheduled_transaction(
                &plan_id,
                &params.0.scheduled_transaction_id,
                serde_json::Value::Object(scheduled_transaction),
            )
            .await,
        )
    }

    #[tool(description = "Delete a scheduled transaction.")]
    pub async fn ynab_delete_scheduled_transaction(
        &self,
        params: Parameters<ScheduledTransactionDeleteParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(
            app.delete_scheduled_transaction(&plan_id, &params.0.scheduled_transaction_id)
                .await,
        )
    }

    #[tool(
        description = "List money movements for a plan. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_list_money_movements(
        &self,
        params: Parameters<PlanIdParam>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(app.list_money_movements(&plan_id).await)
    }

    #[tool(
        description = "List money movements for a month. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_list_money_movements_by_month(
        &self,
        params: Parameters<MonthScopedListParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(
            app.list_money_movements_by_month(&plan_id, &params.0.month)
                .await,
        )
    }

    #[tool(
        description = "List money movement groups for a plan. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_list_money_movement_groups(
        &self,
        params: Parameters<PlanIdParam>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(app.list_money_movement_groups(&plan_id).await)
    }

    #[tool(
        description = "List money movement groups for a month. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_list_money_movement_groups_by_month(
        &self,
        params: Parameters<MonthScopedListParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(
            app.list_money_movement_groups_by_month(&plan_id, &params.0.month)
                .await,
        )
    }

    #[tool(
        description = "List payee locations for a plan. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_list_payee_locations(
        &self,
        params: Parameters<PlanIdParam>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(app.list_payee_locations(&plan_id).await)
    }

    #[tool(
        description = "Get a payee location by identifier. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_get_payee_location(
        &self,
        params: Parameters<PayeeLocationGetParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(
            app.get_payee_location(&plan_id, &params.0.payee_location_id)
                .await,
        )
    }

    #[tool(
        description = "List payee locations for a payee. If plan_id is omitted, the default plan is used."
    )]
    pub async fn ynab_list_payee_locations_by_payee(
        &self,
        params: Parameters<PayeeLocationsByPayeeParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        let plan_id = app
            .resolve_plan_argument(params.0.plan_id.clone())
            .await
            .map_err(crate::tools::to_tool_error)?;
        crate::tools::render_json(
            app.list_payee_locations_by_payee(&plan_id, &params.0.payee_id)
                .await,
        )
    }

    #[tool(description = "Show basic information about the active YNAB user.")]
    pub async fn ynab_get_user(
        &self,
        _params: Parameters<GetUserParams>,
    ) -> Result<String, String> {
        let mut app = self.app.lock().await;
        crate::tools::render_json(app.get_user().await)
    }

    #[tool(description = "Show the current profile, auth kind, and visible plans.")]
    pub async fn ynab_whoami(&self) -> Result<String, String> {
        let mut app = self.app.lock().await;
        crate::tools::render_json(app.whoami().await)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for YnabMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "Use these tools to inspect YNAB auth, plans, accounts, categories, transactions, and related read-only data.",
            )
    }
}

async fn resolve_named_or_explicit(
    app: &mut AppState,
    kind: ResolveByNameKind,
    plan_id: &str,
    explicit_id: Option<String>,
    name: Option<String>,
) -> Result<String, String> {
    if let Some(explicit_id) = explicit_id {
        return Ok(explicit_id);
    }
    let name = name.ok_or_else(|| format!("missing {} id or name", kind_label(kind)))?;
    app.resolve_name(kind, Some(plan_id), &name)
        .await
        .map_err(crate::tools::to_tool_error)
}

async fn resolve_optional_named_or_explicit(
    app: &mut AppState,
    kind: ResolveByNameKind,
    plan_id: &str,
    explicit_id: Option<String>,
    name: Option<String>,
) -> Result<Option<String>, String> {
    if let Some(explicit_id) = explicit_id {
        return Ok(Some(explicit_id));
    }
    match name {
        Some(name) => app
            .resolve_name(kind, Some(plan_id), &name)
            .await
            .map(Some)
            .map_err(crate::tools::to_tool_error),
        None => Ok(None),
    }
}

fn kind_label(kind: ResolveByNameKind) -> &'static str {
    match kind {
        ResolveByNameKind::Plan => "plan",
        ResolveByNameKind::Account => "account",
        ResolveByNameKind::Category => "category",
        ResolveByNameKind::Payee => "payee",
    }
}
