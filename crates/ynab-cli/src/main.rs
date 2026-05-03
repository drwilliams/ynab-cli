use std::{
    fs,
    io::{self, IsTerminal, Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, ExitCode},
    time::{Duration, Instant},
};

use chrono::NaiveDate;
use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use url::Url;
use ynab_core::{
    AmountMilliunits, AppState, OAuthAppInput, OAuthScope, OutputEnvelope, OutputFormat,
    ResolveByNameKind, ResourceListOptions, RuntimeOptions, SaveCategory, TransactionClearedFilter,
    TransactionCreateInput, TransactionListOptions, TransactionSearchOptions,
    TransactionUpdateInput, YnabError,
};

#[derive(Debug, Parser)]
#[command(name = "ynab")]
#[command(about = "JSON-first YNAB CLI for local automation and AI agents")]
struct Cli {
    #[arg(long, global = true)]
    profile: Option<String>,
    #[arg(long, global = true)]
    base_url: Option<String>,
    #[arg(long, global = true, default_value = "json")]
    output: OutputMode,
    #[arg(long, global = true)]
    transform: Option<String>,
    #[arg(long = "query", global = true, alias = "jq")]
    query: Option<String>,
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    raw_output: bool,
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    no_keyring: bool,
    #[arg(long, global = true, env = "YNAB_ACCESS_TOKEN", hide_env_values = true)]
    access_token: Option<String>,
    #[arg(long, short = 'y', global = true, action = ArgAction::SetTrue)]
    yes: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputMode {
    Json,
    PrettyJson,
    Jsonl,
}

impl From<OutputMode> for OutputFormat {
    fn from(value: OutputMode) -> Self {
        match value {
            OutputMode::Json => OutputFormat::Json,
            OutputMode::PrettyJson => OutputFormat::PrettyJson,
            OutputMode::Jsonl => OutputFormat::Jsonl,
        }
    }
}

#[derive(Debug, Clone)]
struct RenderOptions {
    format: OutputFormat,
    transform: Option<String>,
    raw_output: bool,
}

#[derive(Debug, Clone, Copy)]
struct RunOptions {
    yes: bool,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(subcommand)]
    Auth(AuthCommands),
    #[command(subcommand)]
    Skill(SkillCommands),
    #[command(subcommand)]
    Mcp(McpCommands),
    #[command(visible_alias = "budgets")]
    #[command(subcommand)]
    Plans(PlansCommands),
    #[command(subcommand)]
    Accounts(AccountsCommands),
    #[command(subcommand)]
    Categories(CategoriesCommands),
    #[command(subcommand)]
    CategoryGroups(CategoryGroupsCommands),
    #[command(subcommand)]
    Payees(PayeesCommands),
    #[command(subcommand)]
    Transactions(Box<TransactionsCommands>),
    #[command(subcommand)]
    Months(MonthsCommands),
    #[command(subcommand)]
    ScheduledTransactions(Box<ScheduledTransactionsCommands>),
    #[command(subcommand)]
    MoneyMovements(MoneyMovementsCommands),
    #[command(subcommand)]
    MoneyMovementGroups(MoneyMovementGroupsCommands),
    #[command(subcommand)]
    PayeeLocations(PayeeLocationsCommands),
    #[command(subcommand)]
    User(UserCommands),
}

#[derive(Debug, Subcommand)]
enum AuthCommands {
    Login(AuthLoginArgs),
    #[command(subcommand)]
    Token(TokenCommands),
    #[command(subcommand)]
    Oauth(OAuthCommands),
    Whoami,
    Status,
    Logout,
}

#[derive(Debug, Subcommand)]
enum McpCommands {
    PrintConfig(McpPrintConfigArgs),
    Doctor(McpDoctorArgs),
}

#[derive(Debug, Subcommand)]
enum SkillCommands {
    Install(SkillInstallArgs),
    Status(SkillStatusArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SkillTargetArg {
    Codex,
    Claude,
    Openclaw,
}

impl SkillTargetArg {
    fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Openclaw => "openclaw",
        }
    }
}

#[derive(Debug, Args)]
struct SkillInstallArgs {
    #[arg(value_enum)]
    target: SkillTargetArg,
    #[arg(long)]
    project: Option<PathBuf>,
    #[arg(long, action = ArgAction::SetTrue)]
    force: bool,
}

#[derive(Debug, Args)]
struct SkillStatusArgs {
    #[arg(long)]
    project: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct McpPrintConfigArgs {
    #[arg(long)]
    project: PathBuf,
}

#[derive(Debug, Args)]
struct McpDoctorArgs {
    #[arg(long)]
    project: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum TokenCommands {
    Set(TokenSetArgs),
}

#[derive(Debug, Args)]
struct TokenSetArgs {
    #[arg(long)]
    token: Option<String>,
}

#[derive(Debug, Args)]
struct AuthLoginArgs {
    #[arg(long, default_value_t = 180)]
    timeout_seconds: u64,
    #[arg(long, action = ArgAction::SetTrue)]
    no_browser: bool,
}

#[derive(Debug, Subcommand)]
enum OAuthCommands {
    Configure(OAuthConfigureArgs),
    Start(OAuthStartArgs),
    Exchange(OAuthExchangeArgs),
}

#[derive(Debug, Args)]
struct OAuthConfigureArgs {
    #[arg(long)]
    client_id: String,
    #[arg(long)]
    client_secret: String,
    #[arg(long)]
    redirect_uri: String,
    #[arg(long, default_value = "full-access")]
    scope: OAuthScopeArg,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OAuthScopeArg {
    #[value(name = "read-only")]
    ReadOnly,
    #[value(name = "full-access")]
    FullAccess,
}

impl From<OAuthScopeArg> for OAuthScope {
    fn from(value: OAuthScopeArg) -> Self {
        match value {
            OAuthScopeArg::ReadOnly => OAuthScope::ReadOnly,
            OAuthScopeArg::FullAccess => OAuthScope::FullAccess,
        }
    }
}

#[derive(Debug, Args)]
struct OAuthStartArgs {
    #[arg(long, action = ArgAction::SetTrue)]
    open_browser: bool,
}

#[derive(Debug, Args)]
struct OAuthExchangeArgs {
    #[arg(long)]
    code: String,
    #[arg(long)]
    state: Option<String>,
}

#[derive(Debug, Subcommand)]
enum PlansCommands {
    List(PlansListArgs),
    Get(PlanGetArgs),
    Settings(PlanGetArgs),
    SetDefault(PlanDefaultArgs),
}

#[derive(Debug, Args)]
struct PlansListArgs {
    #[arg(long)]
    last_knowledge_of_server: Option<u64>,
    #[arg(long, action = ArgAction::SetTrue)]
    include_accounts: bool,
}

#[derive(Debug, Args)]
struct PlanGetArgs {
    plan_id: String,
}

#[derive(Debug, Args)]
struct PlanDefaultArgs {
    plan_id: String,
}

#[derive(Debug, Subcommand)]
enum AccountsCommands {
    List(ResourceListArgs),
    Get(AccountGetArgs),
    Create(AccountCreateArgs),
}

#[derive(Debug, Subcommand)]
enum CategoriesCommands {
    List(ResourceListArgs),
    Get(CategoryGetArgs),
    Create(CategoryCreateArgs),
    Update(CategoryUpdateArgs),
    UpdateMonth(CategoryUpdateMonthArgs),
}

#[derive(Debug, Subcommand)]
enum CategoryGroupsCommands {
    Create(CategoryGroupCreateArgs),
    Update(CategoryGroupUpdateArgs),
}

#[derive(Debug, Subcommand)]
enum PayeesCommands {
    List(ResourceListArgs),
    Create(PayeeCreateArgs),
    Update(PayeeUpdateArgs),
}

#[derive(Debug, Subcommand)]
enum MonthsCommands {
    List(PlanOnlyArgs),
    Get(MonthGetArgs),
}

#[derive(Debug, Subcommand)]
enum ScheduledTransactionsCommands {
    List(ResourceListArgs),
    Get(ScheduledTransactionGetArgs),
    Create(ScheduledTransactionCreateArgs),
    Update(ScheduledTransactionUpdateArgs),
    Delete(ScheduledTransactionDeleteArgs),
}

#[derive(Debug, Subcommand)]
enum MoneyMovementsCommands {
    List(PlanOnlyArgs),
    ListMonth(MonthPlanArgs),
}

#[derive(Debug, Subcommand)]
enum MoneyMovementGroupsCommands {
    List(PlanOnlyArgs),
    ListMonth(MonthPlanArgs),
}

#[derive(Debug, Subcommand)]
enum PayeeLocationsCommands {
    List(PlanOnlyArgs),
    Get(PayeeLocationGetArgs),
    ListPayee(PayeeLocationsByPayeeArgs),
}

#[derive(Debug, Subcommand)]
enum UserCommands {
    Get,
}

#[derive(Debug, Args)]
struct ResourceListArgs {
    #[arg(long)]
    plan: Option<String>,
    #[arg(long)]
    last_knowledge_of_server: Option<u64>,
}

#[derive(Debug, Args)]
struct PlanOnlyArgs {
    #[arg(long)]
    plan: Option<String>,
}

#[derive(Debug, Args)]
struct MonthPlanArgs {
    #[arg(long)]
    plan: Option<String>,
    month: String,
}

#[derive(Debug, Args)]
struct AccountGetArgs {
    #[arg(long)]
    plan: Option<String>,
    account_id: String,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AccountTypeArg {
    Checking,
    Savings,
    Cash,
    #[value(name = "credit-card")]
    CreditCard,
    #[value(name = "other-asset")]
    OtherAsset,
    #[value(name = "other-liability")]
    OtherLiability,
}

impl AccountTypeArg {
    fn as_api_value(self) -> &'static str {
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

#[derive(Debug, Args)]
struct AccountCreateArgs {
    #[arg(long)]
    plan: Option<String>,
    #[arg(long)]
    name: String,
    #[arg(long, value_enum)]
    account_type: AccountTypeArg,
    #[arg(long)]
    balance: String,
    #[arg(long)]
    cleared_balance: Option<String>,
    #[arg(long)]
    uncleared_balance: Option<String>,
    #[arg(long)]
    transfer_payee_id: Option<String>,
    #[arg(long)]
    note: Option<String>,
    #[arg(long)]
    on_budget: Option<bool>,
    #[arg(long)]
    closed: Option<bool>,
}

#[derive(Debug, Args)]
struct CategoryCreateArgs {
    #[arg(long)]
    plan: Option<String>,
    #[arg(long)]
    name: String,
    #[arg(long)]
    group_id: String,
    #[arg(long)]
    note: Option<String>,
    #[arg(long)]
    goal_target: Option<String>,
    #[arg(long)]
    goal_target_date: Option<String>,
    #[arg(long)]
    goal_needs_whole_amount: Option<bool>,
}

#[derive(Debug, Args)]
struct CategoryGetArgs {
    #[arg(long)]
    plan: Option<String>,
    category_id: String,
}

#[derive(Debug, Args)]
struct CategoryUpdateArgs {
    #[arg(long)]
    plan: Option<String>,
    category_id: String,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    group_id: Option<String>,
    #[arg(long)]
    note: Option<String>,
    #[arg(long)]
    goal_target: Option<String>,
    #[arg(long)]
    goal_target_date: Option<String>,
    #[arg(long)]
    goal_needs_whole_amount: Option<bool>,
}

#[derive(Debug, Args)]
struct CategoryUpdateMonthArgs {
    #[arg(long)]
    plan: Option<String>,
    month: String,
    category_id: String,
    #[arg(long)]
    budgeted: String,
}

#[derive(Debug, Args)]
struct CategoryGroupCreateArgs {
    #[arg(long)]
    plan: Option<String>,
    #[arg(long)]
    name: String,
}

#[derive(Debug, Args)]
struct CategoryGroupUpdateArgs {
    #[arg(long)]
    plan: Option<String>,
    category_group_id: String,
    #[arg(long)]
    name: String,
}

#[derive(Debug, Args)]
struct PayeeCreateArgs {
    #[arg(long)]
    plan: Option<String>,
    #[arg(long)]
    name: String,
}

#[derive(Debug, Args)]
struct PayeeUpdateArgs {
    #[arg(long)]
    plan: Option<String>,
    payee_id: String,
    #[arg(long)]
    name: String,
}

#[derive(Debug, Subcommand)]
enum TransactionsCommands {
    List(TransactionListArgs),
    Search(TransactionSearchArgs),
    Get(TransactionGetArgs),
    Delete(TransactionDeleteArgs),
    ListAccount(TransactionAccountListArgs),
    ListCategory(TransactionCategoryListArgs),
    ListPayee(TransactionPayeeListArgs),
    Create(TransactionCreateArgs),
    CreateBulk(TransactionJsonInputArgs),
    Import(TransactionJsonInputArgs),
    Export(TransactionExportArgs),
    Update(TransactionUpdateArgs),
    UpdateBulk(TransactionJsonInputArgs),
}

#[derive(Debug, Clone, Args)]
struct TransactionFilterArgs {
    #[arg(long)]
    plan: Option<String>,
    #[arg(long)]
    last_knowledge_of_server: Option<u64>,
    #[arg(long, value_name = "YYYY-MM-DD")]
    since_date: Option<String>,
    #[arg(long, value_enum)]
    transaction_type: Option<TransactionApiTypeArg>,
    #[arg(long, value_name = "YYYY-MM-DD")]
    startdate: Option<String>,
    #[arg(long, value_name = "YYYY-MM-DD")]
    enddate: Option<String>,
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "uncleared_only")]
    cleared_only: bool,
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "cleared_only")]
    uncleared_only: bool,
}

#[derive(Debug, Args)]
struct TransactionListArgs {
    #[command(flatten)]
    filters: TransactionFilterArgs,
    #[arg(long, value_name = "YYYY-MM or YYYY-MM-01")]
    month: Option<String>,
}

#[derive(Debug, Args)]
struct TransactionSearchArgs {
    #[command(flatten)]
    filters: TransactionFilterArgs,
    #[arg(long, value_name = "YYYY-MM or YYYY-MM-01")]
    month: Option<String>,
    #[arg(long)]
    query: Option<String>,
    #[arg(long)]
    payee: Option<String>,
    #[arg(long)]
    memo: Option<String>,
    #[arg(long)]
    account: Option<String>,
    #[arg(long)]
    category: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TransactionApiTypeArg {
    Uncategorized,
    Unapproved,
}

impl TransactionApiTypeArg {
    fn as_api_value(self) -> &'static str {
        match self {
            Self::Uncategorized => "uncategorized",
            Self::Unapproved => "unapproved",
        }
    }
}

#[derive(Debug, Args)]
struct TransactionGetArgs {
    #[arg(long)]
    plan: Option<String>,
    transaction_id: String,
}

#[derive(Debug, Args)]
struct TransactionDeleteArgs {
    #[arg(long)]
    plan: Option<String>,
    transaction_id: String,
}

#[derive(Debug, Args)]
struct TransactionAccountListArgs {
    #[command(flatten)]
    filters: TransactionFilterArgs,
    account_id: String,
}

#[derive(Debug, Args)]
struct TransactionCategoryListArgs {
    #[command(flatten)]
    filters: TransactionFilterArgs,
    category_id: String,
}

#[derive(Debug, Args)]
struct TransactionPayeeListArgs {
    #[command(flatten)]
    filters: TransactionFilterArgs,
    payee_id: String,
}

#[derive(Debug, Args)]
struct TransactionJsonInputArgs {
    #[arg(long)]
    plan: Option<String>,
    #[arg(long, value_name = "FILE|-", default_value = "-")]
    input: String,
    #[arg(long, action = ArgAction::SetTrue)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct TransactionExportArgs {
    #[command(flatten)]
    filters: TransactionFilterArgs,
    #[arg(long, value_name = "YYYY-MM or YYYY-MM-01")]
    month: Option<String>,
    #[arg(long, value_name = "FILE")]
    output: String,
}

#[derive(Debug, Args)]
struct TransactionCreateArgs {
    #[arg(long)]
    plan: Option<String>,
    #[arg(long)]
    account_id: Option<String>,
    #[arg(long)]
    account_name: Option<String>,
    #[arg(long)]
    date: String,
    #[arg(long)]
    amount: String,
    #[arg(long)]
    payee_id: Option<String>,
    #[arg(long)]
    payee_name: Option<String>,
    #[arg(long)]
    category_id: Option<String>,
    #[arg(long)]
    category_name: Option<String>,
    #[arg(long)]
    memo: Option<String>,
    #[arg(long)]
    cleared: Option<String>,
    #[arg(long)]
    approved: Option<bool>,
    #[arg(long)]
    flag_color: Option<String>,
    #[arg(long)]
    import_id: Option<String>,
    #[arg(long)]
    id: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct TransactionUpdateArgs {
    #[arg(long)]
    plan: Option<String>,
    transaction_id: String,
    #[arg(long)]
    account_id: Option<String>,
    #[arg(long)]
    account_name: Option<String>,
    #[arg(long)]
    date: Option<String>,
    #[arg(long)]
    amount: Option<String>,
    #[arg(long)]
    payee_id: Option<String>,
    #[arg(long)]
    payee_name: Option<String>,
    #[arg(long)]
    category_id: Option<String>,
    #[arg(long)]
    category_name: Option<String>,
    #[arg(long)]
    memo: Option<String>,
    #[arg(long)]
    cleared: Option<String>,
    #[arg(long)]
    approved: Option<bool>,
    #[arg(long)]
    flag_color: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct MonthGetArgs {
    #[arg(long)]
    plan: Option<String>,
    month: String,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ScheduledFrequencyArg {
    Never,
    Daily,
    Weekly,
    #[value(name = "every-other-week")]
    EveryOtherWeek,
    #[value(name = "twice-a-month")]
    TwiceAMonth,
    #[value(name = "every-4-weeks")]
    Every4Weeks,
    Monthly,
    #[value(name = "every-other-month")]
    EveryOtherMonth,
    #[value(name = "every-3-months")]
    Every3Months,
    #[value(name = "every-4-months")]
    Every4Months,
    #[value(name = "twice-a-year")]
    TwiceAYear,
    Yearly,
    #[value(name = "every-other-year")]
    EveryOtherYear,
}

impl ScheduledFrequencyArg {
    fn as_api_value(self) -> &'static str {
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

#[derive(Debug, Args)]
struct ScheduledTransactionGetArgs {
    #[arg(long)]
    plan: Option<String>,
    scheduled_transaction_id: String,
}

#[derive(Debug, Args)]
struct ScheduledTransactionDeleteArgs {
    #[arg(long)]
    plan: Option<String>,
    scheduled_transaction_id: String,
}

#[derive(Debug, Args)]
struct ScheduledTransactionCreateArgs {
    #[arg(long)]
    plan: Option<String>,
    #[arg(long)]
    account_id: Option<String>,
    #[arg(long)]
    account_name: Option<String>,
    #[arg(long)]
    date: String,
    #[arg(long)]
    amount: String,
    #[arg(long)]
    payee_id: Option<String>,
    #[arg(long)]
    payee_name: Option<String>,
    #[arg(long)]
    category_id: Option<String>,
    #[arg(long)]
    category_name: Option<String>,
    #[arg(long)]
    memo: Option<String>,
    #[arg(long)]
    flag_color: Option<String>,
    #[arg(long, value_enum)]
    frequency: ScheduledFrequencyArg,
}

#[derive(Debug, Args)]
struct ScheduledTransactionUpdateArgs {
    #[arg(long)]
    plan: Option<String>,
    scheduled_transaction_id: String,
    #[arg(long)]
    account_id: Option<String>,
    #[arg(long)]
    account_name: Option<String>,
    #[arg(long)]
    date: Option<String>,
    #[arg(long)]
    amount: Option<String>,
    #[arg(long)]
    payee_id: Option<String>,
    #[arg(long)]
    payee_name: Option<String>,
    #[arg(long)]
    category_id: Option<String>,
    #[arg(long)]
    category_name: Option<String>,
    #[arg(long)]
    memo: Option<String>,
    #[arg(long)]
    flag_color: Option<String>,
    #[arg(long, value_enum)]
    frequency: Option<ScheduledFrequencyArg>,
}

#[derive(Debug, Args)]
struct PayeeLocationGetArgs {
    #[arg(long)]
    plan: Option<String>,
    payee_location_id: String,
}

#[derive(Debug, Args)]
struct PayeeLocationsByPayeeArgs {
    #[arg(long)]
    plan: Option<String>,
    payee_id: String,
}

#[derive(Debug, Clone)]
struct SkillInstallPlan {
    target: SkillTargetArg,
    scope: &'static str,
    destination: PathBuf,
    project: Option<String>,
}

const BUNDLED_SKILL_NAME: &str = "ynab-cli";
const BUNDLED_SKILL_MARKDOWN: &str = include_str!("../../../skills/ynab-cli/SKILL.md");
const BUNDLED_SKILL_OPENAI_YAML: &str = include_str!("../../../skills/ynab-cli/agents/openai.yaml");

#[tokio::main]
async fn main() -> ExitCode {
    let access_token_source = access_token_source_from_args();
    let cli = Cli::parse();
    let result = run(cli, access_token_source).await;
    match result {
        Ok((render_options, envelope)) => {
            print_json(&render_options, &serde_json::to_value(envelope).unwrap());
            ExitCode::SUCCESS
        }
        Err((render_options, error)) => {
            let value = serde_json::to_value(error.to_cli_envelope()).unwrap();
            eprint!("{}", render_json(&render_options, &value));
            ExitCode::from(1)
        }
    }
}

async fn run(
    cli: Cli,
    access_token_source: Option<&'static str>,
) -> Result<(RenderOptions, OutputEnvelope), (RenderOptions, YnabError)> {
    let output_format: OutputFormat = cli.output.into();
    let transform = cli.transform.or(cli.query);
    let render_options = RenderOptions {
        format: output_format,
        transform,
        raw_output: cli.raw_output,
    };
    let run_options = RunOptions { yes: cli.yes };
    let mut app = AppState::load(RuntimeOptions {
        profile: cli.profile,
        use_keyring: !cli.no_keyring,
        base_url_override: cli.base_url,
        output_format,
        access_token_override: cli.access_token,
        access_token_override_source: access_token_source,
    })
    .map_err(|error| (render_options.clone(), error))?;

    let result = match cli.command {
        Commands::Auth(command) => run_auth(&mut app, command).await,
        Commands::Skill(command) => run_skill(command).await,
        Commands::Mcp(command) => run_mcp(&mut app, command).await,
        Commands::Plans(command) => run_plans(&mut app, command).await,
        Commands::Accounts(command) => run_accounts(&mut app, command, run_options).await,
        Commands::Categories(command) => run_categories(&mut app, command, run_options).await,
        Commands::CategoryGroups(command) => {
            run_category_groups(&mut app, command, run_options).await
        }
        Commands::Payees(command) => run_payees(&mut app, command, run_options).await,
        Commands::Transactions(command) => run_transactions(&mut app, *command, run_options).await,
        Commands::Months(command) => run_months(&mut app, command).await,
        Commands::ScheduledTransactions(command) => {
            run_scheduled_transactions(&mut app, *command, run_options).await
        }
        Commands::MoneyMovements(command) => run_money_movements(&mut app, command).await,
        Commands::MoneyMovementGroups(command) => {
            run_money_movement_groups(&mut app, command).await
        }
        Commands::PayeeLocations(command) => run_payee_locations(&mut app, command).await,
        Commands::User(command) => run_user(&mut app, command).await,
    };

    result
        .map(|value| (render_options.clone(), value))
        .map_err(|error| (render_options, error))
}

fn access_token_source_from_args() -> Option<&'static str> {
    for arg in std::env::args_os() {
        if arg == "--access-token" {
            return Some("flag");
        }
        if arg
            .to_str()
            .map(|value| value.starts_with("--access-token="))
            .unwrap_or(false)
        {
            return Some("flag");
        }
    }
    if std::env::var("YNAB_ACCESS_TOKEN").is_ok() {
        Some("env")
    } else {
        None
    }
}

async fn run_auth(app: &mut AppState, command: AuthCommands) -> Result<OutputEnvelope, YnabError> {
    match command {
        AuthCommands::Login(args) => run_login(app, args).await,
        AuthCommands::Token(TokenCommands::Set(args)) => {
            let token = match args.token {
                Some(token) => token,
                None => read_stdin_token()?,
            };
            app.set_personal_access_token(token).await
        }
        AuthCommands::Oauth(command) => match command {
            OAuthCommands::Configure(args) => app.configure_oauth_app(OAuthAppInput {
                client_id: args.client_id,
                client_secret: args.client_secret,
                redirect_uri: args.redirect_uri,
                scope: args.scope.into(),
            }),
            OAuthCommands::Start(args) => app.start_oauth(args.open_browser),
            OAuthCommands::Exchange(args) => {
                app.exchange_oauth_code(&args.code, args.state.as_deref())
                    .await
            }
        },
        AuthCommands::Whoami => app.whoami().await,
        AuthCommands::Status => app.auth_status(),
        AuthCommands::Logout => app.clear_session(),
    }
}

async fn run_skill(command: SkillCommands) -> Result<OutputEnvelope, YnabError> {
    match command {
        SkillCommands::Install(args) => run_skill_install(args),
        SkillCommands::Status(args) => run_skill_status(args),
    }
}

async fn run_mcp(app: &mut AppState, command: McpCommands) -> Result<OutputEnvelope, YnabError> {
    match command {
        McpCommands::PrintConfig(args) => {
            let binary_path = resolve_mcp_binary_path()?;
            let project_path = normalize_project_path(&args.project)?;
            Ok(OutputEnvelope {
                ok: true,
                data: json!({
                    "server_name": "ynab",
                    "project": project_path,
                    "binary_path": binary_path,
                    "codex_config_file": codex_config_file_path(),
                    "codex_config_toml": render_codex_project_config(&project_path, &binary_path),
                    "workspace_mcp_json": render_workspace_mcp_json(&binary_path),
                    "notes": [
                        "Codex typically starts and stops stdio MCP servers for you.",
                        "For Codex app workspaces, the project-scoped ~/.codex/config.toml stanza is the most reliable setup path.",
                        "The .mcp.json snippet is included as a fallback for clients that use project-local MCP config files."
                    ]
                }),
            })
        }
        McpCommands::Doctor(args) => run_mcp_doctor(app, args).await,
    }
}

async fn run_plans(
    app: &mut AppState,
    command: PlansCommands,
) -> Result<OutputEnvelope, YnabError> {
    match command {
        PlansCommands::List(args) => {
            app.list_plans_with_include_accounts(
                ResourceListOptions {
                    last_knowledge_of_server: args.last_knowledge_of_server,
                },
                args.include_accounts,
            )
            .await
        }
        PlansCommands::Get(args) => app.get_plan(&args.plan_id).await,
        PlansCommands::Settings(args) => app.get_plan_settings(&args.plan_id).await,
        PlansCommands::SetDefault(args) => app.set_default_plan(&args.plan_id),
    }
}

async fn run_accounts(
    app: &mut AppState,
    command: AccountsCommands,
    run_options: RunOptions,
) -> Result<OutputEnvelope, YnabError> {
    match command {
        AccountsCommands::List(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            app.list_accounts(
                &plan_id,
                ResourceListOptions {
                    last_knowledge_of_server: args.last_knowledge_of_server,
                },
            )
            .await
        }
        AccountsCommands::Get(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            app.get_account(&plan_id, &args.account_id).await
        }
        AccountsCommands::Create(args) => {
            let plan_id = app.resolve_plan_argument(args.plan.clone()).await?;
            confirm_write(
                run_options,
                "create account",
                &[("plan", plan_id.as_str()), ("name", args.name.as_str())],
            )?;
            let account = build_account_payload(args)?;
            app.create_account(&plan_id, account).await
        }
    }
}

async fn run_categories(
    app: &mut AppState,
    command: CategoriesCommands,
    run_options: RunOptions,
) -> Result<OutputEnvelope, YnabError> {
    match command {
        CategoriesCommands::List(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            app.list_categories(
                &plan_id,
                ResourceListOptions {
                    last_knowledge_of_server: args.last_knowledge_of_server,
                },
            )
            .await
        }
        CategoriesCommands::Get(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            app.get_category(&plan_id, &args.category_id).await
        }
        CategoriesCommands::Create(args) => {
            let plan_id = app.resolve_plan_argument(args.plan.clone()).await?;
            let category = build_create_category_payload(args)?;
            confirm_write(
                run_options,
                "create category",
                &[
                    ("plan", plan_id.as_str()),
                    ("name", category.name.as_deref().unwrap_or("")),
                ],
            )?;
            app.create_category(&plan_id, category).await
        }
        CategoriesCommands::Update(args) => {
            let plan_id = app.resolve_plan_argument(args.plan.clone()).await?;
            let category = build_update_category_payload(&args)?;
            confirm_write(
                run_options,
                "update category",
                &[
                    ("plan", plan_id.as_str()),
                    ("category", args.category_id.as_str()),
                ],
            )?;
            app.update_category(&plan_id, &args.category_id, category)
                .await
        }
        CategoriesCommands::UpdateMonth(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            let month = normalize_month_arg(&args.month)?;
            let budgeted = AmountMilliunits::parse(&args.budgeted)?.0;
            confirm_write(
                run_options,
                "update category budget",
                &[
                    ("plan", plan_id.as_str()),
                    ("month", month.as_str()),
                    ("category", args.category_id.as_str()),
                    ("budgeted", args.budgeted.as_str()),
                ],
            )?;
            app.update_month_category(&plan_id, &month, &args.category_id, budgeted)
                .await
        }
    }
}

async fn run_category_groups(
    app: &mut AppState,
    command: CategoryGroupsCommands,
    run_options: RunOptions,
) -> Result<OutputEnvelope, YnabError> {
    match command {
        CategoryGroupsCommands::Create(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            confirm_write(
                run_options,
                "create category group",
                &[("plan", plan_id.as_str()), ("name", args.name.as_str())],
            )?;
            app.create_category_group(&plan_id, args.name).await
        }
        CategoryGroupsCommands::Update(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            confirm_write(
                run_options,
                "update category group",
                &[
                    ("plan", plan_id.as_str()),
                    ("category_group", args.category_group_id.as_str()),
                    ("name", args.name.as_str()),
                ],
            )?;
            app.update_category_group(&plan_id, &args.category_group_id, args.name)
                .await
        }
    }
}

async fn run_payees(
    app: &mut AppState,
    command: PayeesCommands,
    run_options: RunOptions,
) -> Result<OutputEnvelope, YnabError> {
    match command {
        PayeesCommands::List(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            app.list_payees(
                &plan_id,
                ResourceListOptions {
                    last_knowledge_of_server: args.last_knowledge_of_server,
                },
            )
            .await
        }
        PayeesCommands::Create(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            confirm_write(
                run_options,
                "create payee",
                &[("plan", plan_id.as_str()), ("name", args.name.as_str())],
            )?;
            app.create_payee(&plan_id, args.name).await
        }
        PayeesCommands::Update(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            confirm_write(
                run_options,
                "update payee",
                &[
                    ("plan", plan_id.as_str()),
                    ("payee", args.payee_id.as_str()),
                    ("name", args.name.as_str()),
                ],
            )?;
            app.update_payee(&plan_id, &args.payee_id, args.name).await
        }
    }
}

async fn run_transactions(
    app: &mut AppState,
    command: TransactionsCommands,
    run_options: RunOptions,
) -> Result<OutputEnvelope, YnabError> {
    match command {
        TransactionsCommands::List(args) => {
            let plan_id = app.resolve_plan_argument(args.filters.plan.clone()).await?;
            let options = build_transaction_list_options(args.filters, args.month)?;
            app.list_transactions(&plan_id, options).await
        }
        TransactionsCommands::Search(args) => {
            let plan_id = app.resolve_plan_argument(args.filters.plan.clone()).await?;
            let options = build_transaction_list_options(args.filters.clone(), args.month.clone())?;
            app.search_transactions(&plan_id, options, build_transaction_search_options(&args))
                .await
        }
        TransactionsCommands::Get(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            app.get_transaction(&plan_id, &args.transaction_id).await
        }
        TransactionsCommands::Delete(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            confirm_write(
                run_options,
                "delete transaction",
                &[
                    ("plan", plan_id.as_str()),
                    ("transaction", args.transaction_id.as_str()),
                ],
            )?;
            app.delete_transaction(&plan_id, &args.transaction_id).await
        }
        TransactionsCommands::ListAccount(args) => {
            let plan_id = app.resolve_plan_argument(args.filters.plan.clone()).await?;
            let options = build_transaction_list_options(args.filters, None)?;
            app.list_transactions_by_account(&plan_id, &args.account_id, options)
                .await
        }
        TransactionsCommands::ListCategory(args) => {
            let plan_id = app.resolve_plan_argument(args.filters.plan.clone()).await?;
            let options = build_transaction_list_options(args.filters, None)?;
            app.list_transactions_by_category(&plan_id, &args.category_id, options)
                .await
        }
        TransactionsCommands::ListPayee(args) => {
            let plan_id = app.resolve_plan_argument(args.filters.plan.clone()).await?;
            let options = build_transaction_list_options(args.filters, None)?;
            app.list_transactions_by_payee(&plan_id, &args.payee_id, options)
                .await
        }
        TransactionsCommands::Create(args) => {
            let plan_id = resolve_plan_id(app, args.plan).await?;
            let account_id = resolve_named_or_explicit(
                app,
                ResolveByNameKind::Account,
                Some(&plan_id),
                args.account_id,
                args.account_name,
            )
            .await?;
            let category_id = resolve_optional_named_or_explicit(
                app,
                ResolveByNameKind::Category,
                Some(&plan_id),
                args.category_id,
                args.category_name,
            )
            .await?;
            let payee_id = resolve_optional_named_or_explicit(
                app,
                ResolveByNameKind::Payee,
                Some(&plan_id),
                args.payee_id,
                None,
            )
            .await?;
            if !args.dry_run {
                confirm_write(
                    run_options,
                    "create transaction",
                    &[
                        ("plan", plan_id.as_str()),
                        ("account", account_id.as_str()),
                        ("date", args.date.as_str()),
                        ("amount", args.amount.as_str()),
                    ],
                )?;
            }

            app.create_transaction(TransactionCreateInput {
                plan_id,
                account_id,
                date: args.date,
                amount: AmountMilliunits::parse(&args.amount)?,
                id: args.id,
                import_id: args.import_id,
                payee_id,
                payee_name: args.payee_name,
                category_id,
                memo: args.memo,
                cleared: args.cleared,
                approved: args.approved,
                flag_color: args.flag_color,
                dry_run: args.dry_run,
            })
            .await
        }
        TransactionsCommands::CreateBulk(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            let request = load_transactions_request(&args.input, true)?;
            if !args.dry_run {
                confirm_write(
                    run_options,
                    "create transactions bulk",
                    &[("plan", plan_id.as_str()), ("input", args.input.as_str())],
                )?;
            }
            app.create_transactions_bulk(&plan_id, request, args.dry_run)
                .await
        }
        TransactionsCommands::Import(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            let request = load_transactions_request(&args.input, true)?;
            if !args.dry_run {
                confirm_write(
                    run_options,
                    "import transactions",
                    &[("plan", plan_id.as_str()), ("input", args.input.as_str())],
                )?;
            }
            app.import_transactions(&plan_id, request, args.dry_run)
                .await
        }
        TransactionsCommands::Export(args) => {
            let plan_id = app.resolve_plan_argument(args.filters.plan.clone()).await?;
            let options = build_transaction_list_options(args.filters, args.month)?;
            let envelope = app.list_transactions(&plan_id, options).await?;
            let transactions = envelope
                .data
                .get("transactions")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    YnabError::Config(
                        "transactions list response is missing `transactions` array".to_string(),
                    )
                })?;
            let export_payload = build_import_export_payload(transactions)?;
            write_json_output(&args.output, &export_payload)?;
            let transactions_exported = export_payload
                .get("transactions")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            Ok(OutputEnvelope {
                ok: true,
                data: json!({
                    "path": args.output,
                    "transactions_exported": transactions_exported,
                    "compatible_with": "transactions import"
                }),
            })
        }
        TransactionsCommands::Update(args) => {
            let plan_id = resolve_plan_id(app, args.plan).await?;
            let transaction_id = args.transaction_id.clone();
            let account_id = resolve_optional_named_or_explicit(
                app,
                ResolveByNameKind::Account,
                Some(&plan_id),
                args.account_id,
                args.account_name,
            )
            .await?;
            let category_id = resolve_optional_named_or_explicit(
                app,
                ResolveByNameKind::Category,
                Some(&plan_id),
                args.category_id,
                args.category_name,
            )
            .await?;
            let payee_id = resolve_optional_named_or_explicit(
                app,
                ResolveByNameKind::Payee,
                Some(&plan_id),
                args.payee_id,
                None,
            )
            .await?;
            if !args.dry_run {
                confirm_write(
                    run_options,
                    "update transaction",
                    &[
                        ("plan", plan_id.as_str()),
                        ("transaction", transaction_id.as_str()),
                    ],
                )?;
            }

            app.update_transaction(TransactionUpdateInput {
                plan_id,
                transaction_id,
                account_id,
                date: args.date,
                amount: args
                    .amount
                    .as_deref()
                    .map(AmountMilliunits::parse)
                    .transpose()?,
                payee_id,
                payee_name: args.payee_name,
                category_id,
                memo: args.memo,
                cleared: args.cleared,
                approved: args.approved,
                flag_color: args.flag_color,
                dry_run: args.dry_run,
            })
            .await
        }
        TransactionsCommands::UpdateBulk(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            let request = load_transactions_request(&args.input, false)?;
            if !args.dry_run {
                confirm_write(
                    run_options,
                    "update transactions bulk",
                    &[("plan", plan_id.as_str()), ("input", args.input.as_str())],
                )?;
            }
            app.update_transactions_bulk(&plan_id, request, args.dry_run)
                .await
        }
    }
}

async fn run_months(
    app: &mut AppState,
    command: MonthsCommands,
) -> Result<OutputEnvelope, YnabError> {
    match command {
        MonthsCommands::List(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            app.list_months(&plan_id).await
        }
        MonthsCommands::Get(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            let month = normalize_month_arg(&args.month)?;
            app.get_month(&plan_id, &month).await
        }
    }
}

async fn run_scheduled_transactions(
    app: &mut AppState,
    command: ScheduledTransactionsCommands,
    run_options: RunOptions,
) -> Result<OutputEnvelope, YnabError> {
    match command {
        ScheduledTransactionsCommands::List(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            app.list_scheduled_transactions(
                &plan_id,
                ResourceListOptions {
                    last_knowledge_of_server: args.last_knowledge_of_server,
                },
            )
            .await
        }
        ScheduledTransactionsCommands::Get(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            app.get_scheduled_transaction(&plan_id, &args.scheduled_transaction_id)
                .await
        }
        ScheduledTransactionsCommands::Create(args) => {
            let plan_id = resolve_plan_id(app, args.plan.clone()).await?;
            let scheduled_transaction =
                build_scheduled_transaction_create_payload(app, &plan_id, args).await?;
            confirm_write(
                run_options,
                "create scheduled transaction",
                &[("plan", plan_id.as_str())],
            )?;
            app.create_scheduled_transaction(&plan_id, scheduled_transaction)
                .await
        }
        ScheduledTransactionsCommands::Update(args) => {
            let plan_id = resolve_plan_id(app, args.plan.clone()).await?;
            let scheduled_transaction =
                build_scheduled_transaction_update_payload(app, &plan_id, &args).await?;
            confirm_write(
                run_options,
                "update scheduled transaction",
                &[
                    ("plan", plan_id.as_str()),
                    (
                        "scheduled_transaction",
                        args.scheduled_transaction_id.as_str(),
                    ),
                ],
            )?;
            app.update_scheduled_transaction(
                &plan_id,
                &args.scheduled_transaction_id,
                scheduled_transaction,
            )
            .await
        }
        ScheduledTransactionsCommands::Delete(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            confirm_write(
                run_options,
                "delete scheduled transaction",
                &[
                    ("plan", plan_id.as_str()),
                    (
                        "scheduled_transaction",
                        args.scheduled_transaction_id.as_str(),
                    ),
                ],
            )?;
            app.delete_scheduled_transaction(&plan_id, &args.scheduled_transaction_id)
                .await
        }
    }
}

async fn run_money_movements(
    app: &mut AppState,
    command: MoneyMovementsCommands,
) -> Result<OutputEnvelope, YnabError> {
    match command {
        MoneyMovementsCommands::List(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            app.list_money_movements(&plan_id).await
        }
        MoneyMovementsCommands::ListMonth(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            let month = normalize_month_arg(&args.month)?;
            app.list_money_movements_by_month(&plan_id, &month).await
        }
    }
}

async fn run_money_movement_groups(
    app: &mut AppState,
    command: MoneyMovementGroupsCommands,
) -> Result<OutputEnvelope, YnabError> {
    match command {
        MoneyMovementGroupsCommands::List(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            app.list_money_movement_groups(&plan_id).await
        }
        MoneyMovementGroupsCommands::ListMonth(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            let month = normalize_month_arg(&args.month)?;
            app.list_money_movement_groups_by_month(&plan_id, &month)
                .await
        }
    }
}

async fn run_payee_locations(
    app: &mut AppState,
    command: PayeeLocationsCommands,
) -> Result<OutputEnvelope, YnabError> {
    match command {
        PayeeLocationsCommands::List(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            app.list_payee_locations(&plan_id).await
        }
        PayeeLocationsCommands::Get(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            app.get_payee_location(&plan_id, &args.payee_location_id)
                .await
        }
        PayeeLocationsCommands::ListPayee(args) => {
            let plan_id = app.resolve_plan_argument(args.plan).await?;
            app.list_payee_locations_by_payee(&plan_id, &args.payee_id)
                .await
        }
    }
}

async fn run_user(app: &mut AppState, command: UserCommands) -> Result<OutputEnvelope, YnabError> {
    match command {
        UserCommands::Get => app.get_user().await,
    }
}

async fn run_mcp_doctor(
    app: &mut AppState,
    args: McpDoctorArgs,
) -> Result<OutputEnvelope, YnabError> {
    let binary_path = resolve_mcp_binary_path()?;
    let binary_exists = binary_path.is_file();
    let binary_help = if binary_exists {
        Some(run_binary_help_check(&binary_path))
    } else {
        None
    };
    let auth_status = app.auth_status()?.data;
    let auth_ready = auth_status["auth_source"].as_str().unwrap_or("none") != "none";
    let binary_ready = binary_exists
        && binary_help
            .as_ref()
            .and_then(|value| value["ok"].as_bool())
            .unwrap_or(false);
    let project = if let Some(project) = args.project {
        let normalized = normalize_project_path(&project)?;
        let mcp_json_path = PathBuf::from(&normalized).join(".mcp.json");
        Some(json!({
            "path": normalized,
            "mcp_json_file": mcp_json_path,
            "mcp_json_exists": mcp_json_path.is_file(),
            "recommended_codex_config": render_codex_project_config(&normalized, &binary_path),
            "workspace_mcp_json": render_workspace_mcp_json(&binary_path)
        }))
    } else {
        None
    };

    Ok(OutputEnvelope {
        ok: true,
        data: json!({
            "binary": {
                "path": binary_path,
                "exists": binary_exists,
                "help_check": binary_help
            },
            "auth": auth_status,
            "codex": {
                "config_file": codex_config_file_path()
            },
            "project": project,
            "summary": {
                "binary_ready": binary_ready,
                "auth_ready": auth_ready
            }
        }),
    })
}

fn run_skill_install(args: SkillInstallArgs) -> Result<OutputEnvelope, YnabError> {
    let plan = resolve_skill_install_plan(args.target, args.project.as_deref())?;
    install_bundled_skill(&plan, args.force)?;

    Ok(OutputEnvelope {
        ok: true,
        data: json!({
            "target": plan.target.as_str(),
            "scope": plan.scope,
            "project": plan.project,
            "destination": plan.destination,
            "files_written": [
                plan.destination.join("SKILL.md"),
                plan.destination.join("agents").join("openai.yaml")
            ],
            "force": args.force,
            "notes": skill_install_notes(plan.target, plan.scope)
        }),
    })
}

fn run_skill_status(args: SkillStatusArgs) -> Result<OutputEnvelope, YnabError> {
    let targets = [
        SkillTargetArg::Codex,
        SkillTargetArg::Claude,
        SkillTargetArg::Openclaw,
    ]
    .into_iter()
    .map(|target| {
        let default_plan = resolve_skill_install_plan(target, None)?;
        let project_plan = args
            .project
            .as_deref()
            .map(|project| resolve_skill_install_plan(target, Some(project)))
            .transpose();

        let project_install = match project_plan {
            Ok(plan) => plan.map(skill_status_json),
            Err(error) => Some(json!({
                "supported": false,
                "error": error.to_string()
            })),
        };

        Ok(json!({
            "target": target.as_str(),
            "default_install": skill_status_json(default_plan),
            "project_install": project_install
        }))
    })
    .collect::<Result<Vec<_>, YnabError>>()?;

    Ok(OutputEnvelope {
        ok: true,
        data: json!({
            "skill_name": BUNDLED_SKILL_NAME,
            "project": args
                .project
                .as_deref()
                .map(normalize_project_path)
                .transpose()?,
            "targets": targets
        }),
    })
}

async fn resolve_plan_id(app: &mut AppState, plan: Option<String>) -> Result<String, YnabError> {
    app.resolve_plan_argument(plan).await
}

async fn resolve_named_or_explicit(
    app: &mut AppState,
    kind: ResolveByNameKind,
    plan_id: Option<&str>,
    explicit_id: Option<String>,
    name: Option<String>,
) -> Result<String, YnabError> {
    if let Some(explicit_id) = explicit_id {
        return Ok(explicit_id);
    }
    let name =
        name.ok_or_else(|| YnabError::Config(format!("missing {} id or name", kind_label(kind))))?;
    app.resolve_name(kind, plan_id, &name).await
}

async fn resolve_optional_named_or_explicit(
    app: &mut AppState,
    kind: ResolveByNameKind,
    plan_id: Option<&str>,
    explicit_id: Option<String>,
    name: Option<String>,
) -> Result<Option<String>, YnabError> {
    if let Some(explicit_id) = explicit_id {
        return Ok(Some(explicit_id));
    }
    match name {
        Some(name) => app.resolve_name(kind, plan_id, &name).await.map(Some),
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

fn transaction_cleared_filter(args: &TransactionFilterArgs) -> Option<TransactionClearedFilter> {
    if args.cleared_only {
        Some(TransactionClearedFilter::Cleared)
    } else if args.uncleared_only {
        Some(TransactionClearedFilter::Uncleared)
    } else {
        None
    }
}

fn build_transaction_list_options(
    args: TransactionFilterArgs,
    month: Option<String>,
) -> Result<TransactionListOptions, YnabError> {
    let start_date = args.startdate.as_deref().map(parse_cli_date).transpose()?;
    let end_date = args.enddate.as_deref().map(parse_cli_date).transpose()?;
    if let (Some(start_date), Some(end_date)) = (start_date, end_date)
        && start_date > end_date
    {
        return Err(YnabError::Config(
            "--startdate must be on or before --enddate".to_string(),
        ));
    }

    Ok(TransactionListOptions {
        last_knowledge_of_server: args.last_knowledge_of_server,
        month: month.as_deref().map(normalize_month_arg).transpose()?,
        since_date: args
            .since_date
            .as_deref()
            .map(normalize_iso_date)
            .transpose()?,
        transaction_type: args
            .transaction_type
            .map(TransactionApiTypeArg::as_api_value)
            .map(str::to_string),
        start_date,
        end_date,
        cleared_filter: transaction_cleared_filter(&args),
    })
}

fn build_transaction_search_options(args: &TransactionSearchArgs) -> TransactionSearchOptions {
    TransactionSearchOptions {
        query: args.query.clone(),
        payee: args.payee.clone(),
        memo: args.memo.clone(),
        account: args.account.clone(),
        category: args.category.clone(),
    }
}

fn build_account_payload(args: AccountCreateArgs) -> Result<Value, YnabError> {
    let mut account = serde_json::Map::new();
    account.insert("name".to_string(), Value::String(args.name));
    account.insert(
        "type".to_string(),
        Value::String(args.account_type.as_api_value().to_string()),
    );
    account.insert(
        "balance".to_string(),
        Value::from(AmountMilliunits::parse(&args.balance)?.0),
    );
    if let Some(value) = args.cleared_balance {
        account.insert(
            "cleared_balance".to_string(),
            Value::from(AmountMilliunits::parse(&value)?.0),
        );
    }
    if let Some(value) = args.uncleared_balance {
        account.insert(
            "uncleared_balance".to_string(),
            Value::from(AmountMilliunits::parse(&value)?.0),
        );
    }
    if let Some(value) = args.transfer_payee_id {
        account.insert("transfer_payee_id".to_string(), Value::String(value));
    }
    if let Some(value) = args.note {
        account.insert("note".to_string(), Value::String(value));
    }
    if let Some(value) = args.on_budget {
        account.insert("on_budget".to_string(), Value::Bool(value));
    }
    if let Some(value) = args.closed {
        account.insert("closed".to_string(), Value::Bool(value));
    }
    Ok(Value::Object(account))
}

async fn build_scheduled_transaction_create_payload(
    app: &mut AppState,
    plan_id: &str,
    args: ScheduledTransactionCreateArgs,
) -> Result<Value, YnabError> {
    let account_id = resolve_named_or_explicit(
        app,
        ResolveByNameKind::Account,
        Some(plan_id),
        args.account_id,
        args.account_name,
    )
    .await?;
    let category_id = resolve_optional_named_or_explicit(
        app,
        ResolveByNameKind::Category,
        Some(plan_id),
        args.category_id,
        args.category_name,
    )
    .await?;
    let payee_id = resolve_optional_named_or_explicit(
        app,
        ResolveByNameKind::Payee,
        Some(plan_id),
        args.payee_id,
        None,
    )
    .await?;

    let mut scheduled_transaction = serde_json::Map::new();
    scheduled_transaction.insert("account_id".to_string(), Value::String(account_id));
    scheduled_transaction.insert(
        "date".to_string(),
        Value::String(normalize_iso_date(&args.date)?),
    );
    scheduled_transaction.insert(
        "amount".to_string(),
        Value::from(AmountMilliunits::parse(&args.amount)?.0),
    );
    scheduled_transaction.insert(
        "frequency".to_string(),
        Value::String(args.frequency.as_api_value().to_string()),
    );
    if let Some(value) = payee_id {
        scheduled_transaction.insert("payee_id".to_string(), Value::String(value));
    }
    if let Some(value) = args.payee_name {
        scheduled_transaction.insert("payee_name".to_string(), Value::String(value));
    }
    if let Some(value) = category_id {
        scheduled_transaction.insert("category_id".to_string(), Value::String(value));
    }
    if let Some(value) = args.memo {
        scheduled_transaction.insert("memo".to_string(), Value::String(value));
    }
    if let Some(value) = args.flag_color {
        scheduled_transaction.insert("flag_color".to_string(), Value::String(value));
    }

    Ok(Value::Object(scheduled_transaction))
}

async fn build_scheduled_transaction_update_payload(
    app: &mut AppState,
    plan_id: &str,
    args: &ScheduledTransactionUpdateArgs,
) -> Result<Value, YnabError> {
    let account_id = resolve_optional_named_or_explicit(
        app,
        ResolveByNameKind::Account,
        Some(plan_id),
        args.account_id.clone(),
        args.account_name.clone(),
    )
    .await?;
    let category_id = resolve_optional_named_or_explicit(
        app,
        ResolveByNameKind::Category,
        Some(plan_id),
        args.category_id.clone(),
        args.category_name.clone(),
    )
    .await?;
    let payee_id = resolve_optional_named_or_explicit(
        app,
        ResolveByNameKind::Payee,
        Some(plan_id),
        args.payee_id.clone(),
        None,
    )
    .await?;

    let mut scheduled_transaction = serde_json::Map::new();
    if let Some(value) = account_id {
        scheduled_transaction.insert("account_id".to_string(), Value::String(value));
    }
    if let Some(value) = args.date.as_deref() {
        scheduled_transaction.insert(
            "date".to_string(),
            Value::String(normalize_iso_date(value)?),
        );
    }
    if let Some(value) = args.amount.as_deref() {
        scheduled_transaction.insert(
            "amount".to_string(),
            Value::from(AmountMilliunits::parse(value)?.0),
        );
    }
    if let Some(value) = payee_id {
        scheduled_transaction.insert("payee_id".to_string(), Value::String(value));
    }
    if let Some(value) = args.payee_name.as_ref() {
        scheduled_transaction.insert("payee_name".to_string(), Value::String(value.clone()));
    }
    if let Some(value) = category_id {
        scheduled_transaction.insert("category_id".to_string(), Value::String(value));
    }
    if let Some(value) = args.memo.as_ref() {
        scheduled_transaction.insert("memo".to_string(), Value::String(value.clone()));
    }
    if let Some(value) = args.flag_color.as_ref() {
        scheduled_transaction.insert("flag_color".to_string(), Value::String(value.clone()));
    }
    if let Some(value) = args.frequency {
        scheduled_transaction.insert(
            "frequency".to_string(),
            Value::String(value.as_api_value().to_string()),
        );
    }

    if scheduled_transaction.is_empty() {
        return Err(YnabError::Config(
            "scheduled-transactions update requires at least one field to change".to_string(),
        ));
    }

    Ok(Value::Object(scheduled_transaction))
}

fn read_json_input(path: &str) -> Result<Value, YnabError> {
    let normalized_path = path
        .strip_prefix("@file://")
        .or_else(|| path.strip_prefix('@'))
        .unwrap_or(path);
    let raw = if normalized_path == "-" {
        let mut input = String::new();
        io::stdin().read_to_string(&mut input)?;
        input
    } else {
        fs::read_to_string(normalized_path)?
    };
    Ok(serde_json::from_str(&raw)?)
}

fn load_transactions_request(
    path: &str,
    allow_single_transaction: bool,
) -> Result<Value, YnabError> {
    let input = read_json_input(path)?;
    if let Some(object) = input.as_object()
        && (object.contains_key("transaction") || object.contains_key("transactions"))
    {
        return Ok(input);
    }

    if let Some(transactions) = input.as_array() {
        return Ok(json!({ "transactions": transactions }));
    }

    if allow_single_transaction && input.is_object() {
        return Ok(json!({ "transaction": input }));
    }

    Err(YnabError::Config(
        "transaction input must be a JSON object wrapper or an array of transactions".to_string(),
    ))
}

fn build_import_export_payload(transactions: &[Value]) -> Result<Value, YnabError> {
    let mut export_transactions = Vec::with_capacity(transactions.len());
    for (index, transaction) in transactions.iter().enumerate() {
        if let Some(transaction) = export_transaction(transaction, index)? {
            export_transactions.push(transaction);
        }
    }
    Ok(json!({ "transactions": export_transactions }))
}

fn export_transaction(transaction: &Value, index: usize) -> Result<Option<Value>, YnabError> {
    let object = transaction.as_object().ok_or_else(|| {
        YnabError::Config(format!(
            "unable to export transaction at index {index}: expected an object"
        ))
    })?;

    if object.get("deleted").and_then(Value::as_bool) == Some(true) {
        return Ok(None);
    }

    let account_id = object
        .get("account_id")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            YnabError::Config(format!(
                "unable to export transaction at index {index}: missing required string `account_id`"
            ))
        })?;
    let date = object.get("date").and_then(Value::as_str).ok_or_else(|| {
        YnabError::Config(format!(
            "unable to export transaction at index {index}: missing required string `date`"
        ))
    })?;
    let amount = object.get("amount").and_then(value_to_i64).ok_or_else(|| {
        YnabError::Config(format!(
            "unable to export transaction at index {index}: missing required integer `amount`"
        ))
    })?;

    let mut exported = serde_json::Map::new();
    exported.insert(
        "account_id".to_string(),
        Value::String(account_id.to_string()),
    );
    exported.insert("date".to_string(), Value::String(normalize_iso_date(date)?));
    exported.insert("amount".to_string(), Value::from(amount));

    copy_optional_string_field(&mut exported, object, "import_id");
    copy_optional_string_field(&mut exported, object, "payee_id");
    copy_optional_string_field(&mut exported, object, "payee_name");
    copy_optional_string_field(&mut exported, object, "category_id");
    copy_optional_string_field(&mut exported, object, "memo");
    copy_optional_string_field(&mut exported, object, "cleared");
    copy_optional_bool_field(&mut exported, object, "approved");
    copy_optional_string_field(&mut exported, object, "flag_color");
    copy_optional_string_field(&mut exported, object, "transfer_account_id");

    if let Some(subtransactions) = object.get("subtransactions") {
        let export_subtransactions = export_subtransactions(subtransactions, index)?;
        if !export_subtransactions.is_empty() {
            exported.insert(
                "subtransactions".to_string(),
                Value::Array(export_subtransactions),
            );
        }
    }

    Ok(Some(Value::Object(exported)))
}

fn export_subtransactions(value: &Value, parent_index: usize) -> Result<Vec<Value>, YnabError> {
    let subtransactions = value.as_array().ok_or_else(|| {
        YnabError::Config(format!(
            "unable to export transaction at index {parent_index}: `subtransactions` must be an array"
        ))
    })?;

    let mut exported = Vec::with_capacity(subtransactions.len());
    for (sub_index, subtransaction) in subtransactions.iter().enumerate() {
        let object = subtransaction.as_object().ok_or_else(|| {
            YnabError::Config(format!(
                "unable to export transaction at index {parent_index}: subtransaction at index {sub_index} must be an object"
            ))
        })?;
        if object.get("deleted").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let amount = object.get("amount").and_then(value_to_i64).ok_or_else(|| {
            YnabError::Config(format!(
                "unable to export transaction at index {parent_index}: subtransaction at index {sub_index} is missing required integer `amount`"
            ))
        })?;

        let mut export_subtransaction = serde_json::Map::new();
        export_subtransaction.insert("amount".to_string(), Value::from(amount));
        copy_optional_string_field(&mut export_subtransaction, object, "payee_id");
        copy_optional_string_field(&mut export_subtransaction, object, "payee_name");
        copy_optional_string_field(&mut export_subtransaction, object, "category_id");
        copy_optional_string_field(&mut export_subtransaction, object, "memo");
        copy_optional_string_field(&mut export_subtransaction, object, "transfer_account_id");
        exported.push(Value::Object(export_subtransaction));
    }

    Ok(exported)
}

fn copy_optional_string_field(
    target: &mut serde_json::Map<String, Value>,
    source: &serde_json::Map<String, Value>,
    field: &str,
) {
    if let Some(value) = source.get(field).and_then(Value::as_str) {
        target.insert(field.to_string(), Value::String(value.to_string()));
    }
}

fn copy_optional_bool_field(
    target: &mut serde_json::Map<String, Value>,
    source: &serde_json::Map<String, Value>,
    field: &str,
) {
    if let Some(value) = source.get(field).and_then(Value::as_bool) {
        target.insert(field.to_string(), Value::Bool(value));
    }
}

fn value_to_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|number| i64::try_from(number).ok()))
}

fn write_json_output(path: &str, value: &Value) -> Result<(), YnabError> {
    let raw = serde_json::to_string_pretty(value)?;
    if path == "-" {
        println!("{raw}");
    } else {
        fs::write(path, raw)?;
    }
    Ok(())
}

fn build_create_category_payload(args: CategoryCreateArgs) -> Result<SaveCategory, YnabError> {
    Ok(SaveCategory {
        name: Some(args.name),
        note: args.note,
        category_group_id: Some(args.group_id),
        goal_target: args
            .goal_target
            .as_deref()
            .map(AmountMilliunits::parse)
            .transpose()?
            .map(|amount| amount.0),
        goal_target_date: args
            .goal_target_date
            .as_deref()
            .map(normalize_iso_date)
            .transpose()?,
        goal_needs_whole_amount: args.goal_needs_whole_amount,
    })
}

fn build_update_category_payload(args: &CategoryUpdateArgs) -> Result<SaveCategory, YnabError> {
    let payload = SaveCategory {
        name: args.name.clone(),
        note: args.note.clone(),
        category_group_id: args.group_id.clone(),
        goal_target: args
            .goal_target
            .as_deref()
            .map(AmountMilliunits::parse)
            .transpose()?
            .map(|amount| amount.0),
        goal_target_date: args
            .goal_target_date
            .as_deref()
            .map(normalize_iso_date)
            .transpose()?,
        goal_needs_whole_amount: args.goal_needs_whole_amount,
    };

    if payload.name.is_none()
        && payload.note.is_none()
        && payload.category_group_id.is_none()
        && payload.goal_target.is_none()
        && payload.goal_target_date.is_none()
        && payload.goal_needs_whole_amount.is_none()
    {
        return Err(YnabError::Config(
            "categories update requires at least one field to change".to_string(),
        ));
    }

    Ok(payload)
}

fn normalize_month_arg(input: &str) -> Result<String, YnabError> {
    let trimmed = input.trim();
    if let Ok(month) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        return Ok(month.format("%Y-%m-%d").to_string());
    }
    let normalized = format!("{trimmed}-01");
    let month: NaiveDate = NaiveDate::parse_from_str(&normalized, "%Y-%m-%d").map_err(|_| {
        YnabError::Config(format!(
            "invalid --month value `{trimmed}`. use YYYY-MM or YYYY-MM-01"
        ))
    })?;
    Ok(month.format("%Y-%m-%d").to_string())
}

fn parse_cli_date(input: &str) -> Result<NaiveDate, YnabError> {
    NaiveDate::parse_from_str(input.trim(), "%Y-%m-%d")
        .map_err(|_| YnabError::Config(format!("invalid date `{}`. use YYYY-MM-DD", input.trim())))
}

fn normalize_iso_date(input: &str) -> Result<String, YnabError> {
    Ok(parse_cli_date(input)?.format("%Y-%m-%d").to_string())
}

fn read_stdin_token() -> Result<String, YnabError> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let token = input.trim().to_string();
    if token.is_empty() {
        return Err(YnabError::Config(
            "token was not provided via --token or stdin".to_string(),
        ));
    }
    Ok(token)
}

fn confirm_write(
    options: RunOptions,
    action: &str,
    details: &[(&str, &str)],
) -> Result<(), YnabError> {
    if options.yes {
        return Ok(());
    }
    if !io::stdin().is_terminal() {
        return Err(YnabError::Config(format!(
            "refusing to {action} without confirmation in non-interactive mode; pass --yes to confirm"
        )));
    }

    eprintln!();
    eprintln!("About to {action}:");
    for (label, value) in details {
        eprintln!("  {label}: {value}");
    }
    eprint!("Proceed? [y/N] ");
    io::stderr().flush()?;

    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    match answer.trim().to_lowercase().as_str() {
        "y" | "yes" => Ok(()),
        _ => Err(YnabError::Config("operation cancelled".to_string())),
    }
}

fn print_json(options: &RenderOptions, value: &Value) {
    print!("{}", render_json(options, value));
}

fn render_json(options: &RenderOptions, value: &Value) -> String {
    let value = match options.transform.as_deref() {
        Some(path) => transform_json(value, path),
        None => value.clone(),
    };

    if options.raw_output
        && let Some(raw) = raw_scalar_output(&value)
    {
        return format!("{raw}\n");
    }

    let rendered = match options.format {
        OutputFormat::Json => serde_json::to_string(&value).unwrap(),
        OutputFormat::PrettyJson => serde_json::to_string_pretty(&value).unwrap(),
        OutputFormat::Jsonl => render_jsonl(&value),
    };
    format!("{rendered}\n")
}

fn resolve_mcp_binary_path() -> Result<PathBuf, YnabError> {
    let current_exe = std::env::current_exe()?;
    let current_dir = current_exe.parent().ok_or_else(|| {
        YnabError::Config("unable to determine the current executable directory".to_string())
    })?;
    let binary_name = format!("ynab-mcp{}", std::env::consts::EXE_SUFFIX);
    let sibling = current_dir.join(&binary_name);
    if sibling.exists() {
        return Ok(sibling);
    }

    if let Some(target_dir) = current_dir.parent() {
        let target_fallback = target_dir.join(&binary_name);
        if target_fallback.exists() {
            return Ok(target_fallback);
        }
        return Ok(target_fallback);
    }

    Ok(sibling)
}

fn normalize_project_path(path: &Path) -> Result<String, YnabError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    Ok(absolute.to_string_lossy().into_owned())
}

fn resolve_skill_install_plan(
    target: SkillTargetArg,
    project: Option<&Path>,
) -> Result<SkillInstallPlan, YnabError> {
    let destination = match (target, project) {
        (SkillTargetArg::Codex, Some(_)) => {
            return Err(YnabError::Config(
                "project-scoped Codex skill installs are not currently supported; install to ~/.codex/skills instead".to_string(),
            ));
        }
        (SkillTargetArg::Codex, None) => home_dir_path()?
            .join(".codex/skills")
            .join(BUNDLED_SKILL_NAME),
        (SkillTargetArg::Claude, None) => home_dir_path()?
            .join(".claude/skills")
            .join(BUNDLED_SKILL_NAME),
        (SkillTargetArg::Claude, Some(project)) => absolute_path(project)?
            .join(".claude/skills")
            .join(BUNDLED_SKILL_NAME),
        (SkillTargetArg::Openclaw, None) => home_dir_path()?
            .join(".openclaw/skills")
            .join(BUNDLED_SKILL_NAME),
        (SkillTargetArg::Openclaw, Some(project)) => absolute_path(project)?
            .join("skills")
            .join(BUNDLED_SKILL_NAME),
    };

    Ok(SkillInstallPlan {
        target,
        scope: if project.is_some() { "project" } else { "user" },
        destination,
        project: project.map(normalize_project_path).transpose()?,
    })
}

fn install_bundled_skill(plan: &SkillInstallPlan, force: bool) -> Result<(), YnabError> {
    if plan.destination.exists() {
        if !force {
            return Err(YnabError::Config(format!(
                "skill destination already exists: {} (pass --force to overwrite)",
                plan.destination.display()
            )));
        }
        fs::remove_dir_all(&plan.destination)?;
    }

    fs::create_dir_all(plan.destination.join("agents"))?;
    fs::write(plan.destination.join("SKILL.md"), BUNDLED_SKILL_MARKDOWN)?;
    fs::write(
        plan.destination.join("agents").join("openai.yaml"),
        BUNDLED_SKILL_OPENAI_YAML,
    )?;
    Ok(())
}

fn skill_status_json(plan: SkillInstallPlan) -> Value {
    let skill_md = plan.destination.join("SKILL.md");
    let openai_yaml = plan.destination.join("agents").join("openai.yaml");
    json!({
        "supported": true,
        "scope": plan.scope,
        "project": plan.project,
        "destination": plan.destination,
        "installed": skill_md.is_file(),
        "files": {
            "skill_md": {
                "path": skill_md,
                "exists": skill_md.is_file()
            },
            "openai_yaml": {
                "path": openai_yaml,
                "exists": openai_yaml.is_file()
            }
        }
    })
}

fn skill_install_notes(target: SkillTargetArg, scope: &str) -> Vec<&'static str> {
    match (target, scope) {
        (SkillTargetArg::Codex, "user") => vec![
            "Restart Codex if it does not pick up the new user-level skill automatically.",
            "The agents/openai.yaml file is optional metadata for Codex/OpenAI clients.",
        ],
        (SkillTargetArg::Claude, "user") | (SkillTargetArg::Claude, "project") => vec![
            "Claude Code loads skills from ~/.claude/skills and project-local .claude/skills directories.",
            "The agents/openai.yaml file can remain present and is ignored by Claude Code.",
        ],
        (SkillTargetArg::Openclaw, "user") | (SkillTargetArg::Openclaw, "project") => vec![
            "OpenClaw loads shared skills from ~/.openclaw/skills and workspace skills from <workspace>/skills.",
            "The agents/openai.yaml file can remain present and is ignored by OpenClaw.",
        ],
        _ => vec![],
    }
}

fn home_dir_path() -> Result<PathBuf, YnabError> {
    dirs::home_dir()
        .ok_or_else(|| YnabError::Config("unable to determine home directory".to_string()))
}

fn absolute_path(path: &Path) -> Result<PathBuf, YnabError> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn codex_config_file_path() -> String {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".codex/config.toml")
        .to_string_lossy()
        .into_owned()
}

fn render_codex_project_config(project: &str, binary_path: &Path) -> String {
    let project = toml_basic_string(project);
    let command = toml_basic_string(&binary_path.to_string_lossy());
    format!(
        "[projects.{project}]\ntrust_level = \"trusted\"\n\n[projects.{project}.mcp_servers.ynab]\ncommand = {command}\nargs = [\"--profile\", \"default\"]"
    )
}

fn render_workspace_mcp_json(binary_path: &Path) -> String {
    let escaped_path = json_string(&binary_path.to_string_lossy());
    format!(
        "{{\n  \"mcpServers\": {{\n    \"ynab\": {{\n      \"command\": {escaped_path},\n      \"args\": [\"--profile\", \"default\"]\n    }}\n  }}\n}}"
    )
}

fn toml_basic_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('\"', "\\\""))
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap()
}

fn run_binary_help_check(path: &Path) -> Value {
    match ProcessCommand::new(path).arg("--help").output() {
        Ok(output) => json!({
            "ok": output.status.success(),
            "status": output.status.code(),
            "stderr": String::from_utf8_lossy(&output.stderr).trim(),
            "stdout_preview": String::from_utf8_lossy(&output.stdout)
                .lines()
                .take(3)
                .collect::<Vec<_>>()
                .join("\n")
        }),
        Err(error) => json!({
            "ok": false,
            "error": error.to_string()
        }),
    }
}

fn transform_json(value: &Value, path: &str) -> Value {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "." {
        return value.clone();
    }

    let parts = trimmed
        .trim_start_matches('.')
        .split('.')
        .collect::<Vec<_>>();
    let start = match parts.first().copied() {
        Some("ok" | "data" | "error") => value,
        _ => value.get("data").unwrap_or(value),
    };

    let mut current = start;
    for part in parts {
        if part.is_empty() {
            continue;
        }
        current = match current {
            Value::Object(map) => match map.get(part) {
                Some(value) => value,
                None => return Value::Null,
            },
            Value::Array(values) => match part
                .parse::<usize>()
                .ok()
                .and_then(|index| values.get(index))
            {
                Some(value) => value,
                None => return Value::Null,
            },
            _ => return Value::Null,
        };
    }
    current.clone()
}

fn raw_scalar_output(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Null => Some("null".to_string()),
        Value::Array(_) | Value::Object(_) => None,
    }
}

fn render_jsonl(value: &Value) -> String {
    match value {
        Value::Array(values) => values
            .iter()
            .map(|value| serde_json::to_string(value).unwrap())
            .collect::<Vec<_>>()
            .join("\n"),
        value => serde_json::to_string(value).unwrap(),
    }
}

async fn run_login(app: &mut AppState, args: AuthLoginArgs) -> Result<OutputEnvelope, YnabError> {
    let redirect_uri = app.oauth_redirect_uri()?;
    let callback_config = CallbackConfig::from_redirect_uri(&redirect_uri)?;
    let listener = bind_callback_listener(&callback_config)?;
    let flow = app.start_oauth_flow()?;

    if io::stdin().is_terminal() {
        eprint!("Press Enter to open the browser for YNAB login...");
        io::stderr().flush()?;
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
    }

    eprintln!(
        "Waiting for OAuth callback on {}",
        callback_config.bind_display()
    );
    if args.no_browser {
        eprintln!("Open this URL to continue:\n{}", flow.authorize_url);
    } else if let Err(error) = webbrowser::open(&flow.authorize_url) {
        eprintln!("Browser open failed: {error}");
        eprintln!("Open this URL to continue:\n{}", flow.authorize_url);
    }

    let callback = wait_for_callback(
        &listener,
        &callback_config,
        Duration::from_secs(args.timeout_seconds),
    )?;
    app.exchange_oauth_code(&callback.code, callback.state.as_deref())
        .await
}

#[derive(Debug, Clone)]
struct CallbackConfig {
    bind_host: String,
    port: u16,
    path: String,
}

#[derive(Debug)]
struct OAuthCallback {
    code: String,
    state: Option<String>,
}

impl CallbackConfig {
    fn from_redirect_uri(redirect_uri: &str) -> Result<Self, YnabError> {
        let url = Url::parse(redirect_uri)?;
        if url.scheme() != "http" {
            return Err(YnabError::Config(
                "interactive login requires an http:// loopback redirect URI".to_string(),
            ));
        }

        let host = url
            .host_str()
            .ok_or_else(|| YnabError::Config("redirect URI is missing a host".to_string()))?;
        if !matches!(host, "127.0.0.1" | "localhost" | "::1") {
            return Err(YnabError::Config(
                "interactive login requires a localhost/127.0.0.1/::1 redirect URI".to_string(),
            ));
        }

        let port = url.port().ok_or_else(|| {
            YnabError::Config(
                "interactive login requires an explicit redirect URI port".to_string(),
            )
        })?;

        Ok(Self {
            bind_host: host.to_string(),
            port,
            path: normalize_callback_path(url.path()),
        })
    }

    fn bind_addr(&self) -> String {
        if self.bind_host.contains(':') {
            format!("[{}]:{}", self.bind_host, self.port)
        } else {
            format!("{}:{}", self.bind_host, self.port)
        }
    }

    fn bind_display(&self) -> String {
        format!("{}{}", self.bind_addr(), self.path)
    }
}

fn bind_callback_listener(config: &CallbackConfig) -> Result<TcpListener, YnabError> {
    let listener = TcpListener::bind(config.bind_addr())?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

fn wait_for_callback(
    listener: &TcpListener,
    config: &CallbackConfig,
    timeout: Duration,
) -> Result<OAuthCallback, YnabError> {
    let deadline = Instant::now() + timeout;

    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_read_timeout(Some(Duration::from_secs(10)))?;
                stream.set_write_timeout(Some(Duration::from_secs(10)))?;

                let mut buffer = [0u8; 8192];
                let bytes_read = stream.read(&mut buffer)?;
                if bytes_read == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&buffer[..bytes_read]);
                let response = parse_callback_request(&request, config);
                match response {
                    Ok(callback) => {
                        write_http_response(
                            &mut stream,
                            200,
                            "Login complete. You can return to the terminal.",
                        )?;
                        return Ok(callback);
                    }
                    Err(error) => {
                        write_http_response(
                            &mut stream,
                            400,
                            "OAuth login failed. Check the terminal for details.",
                        )?;
                        return Err(error);
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(YnabError::Config(
                        "timed out waiting for OAuth callback".to_string(),
                    ));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn parse_callback_request(
    request: &str,
    config: &CallbackConfig,
) -> Result<OAuthCallback, YnabError> {
    let request_line = request
        .lines()
        .next()
        .ok_or_else(|| YnabError::Config("malformed OAuth callback request".to_string()))?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();

    if method != "GET" {
        return Err(YnabError::Config(
            "unexpected HTTP method for OAuth callback".to_string(),
        ));
    }

    let callback_url = Url::parse(&format!("http://localhost{target}"))?;
    if normalize_callback_path(callback_url.path()) != config.path {
        return Err(YnabError::Config(
            "received OAuth callback on unexpected path".to_string(),
        ));
    }

    let mut code = None;
    let mut state = None;
    let mut error = None;
    let mut error_description = None;

    for (key, value) in callback_url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            "error" => error = Some(value.into_owned()),
            "error_description" => error_description = Some(value.into_owned()),
            _ => {}
        }
    }

    if let Some(error) = error {
        let message = match error_description {
            Some(detail) => format!("OAuth authorization failed: {error}: {detail}"),
            None => format!("OAuth authorization failed: {error}"),
        };
        return Err(YnabError::Config(message));
    }

    let code = code.ok_or_else(|| {
        YnabError::Config("OAuth callback did not include an authorization code".to_string())
    })?;

    Ok(OAuthCallback { code, state })
}

fn normalize_callback_path(path: &str) -> String {
    if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    }
}

fn write_http_response(
    stream: &mut std::net::TcpStream,
    status: u16,
    message: &str,
) -> Result<(), YnabError> {
    let status_text = if status == 200 { "OK" } else { "Bad Request" };
    let body = format!(
        "<!doctype html><html><body><h1>{}</h1></body></html>",
        html_escape(message)
    );
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    Ok(())
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        CallbackConfig, TransactionFilterArgs, TransactionSearchArgs, build_import_export_payload,
        build_transaction_search_options, parse_callback_request,
    };

    #[test]
    fn parses_loopback_redirect_uri() {
        let config = CallbackConfig::from_redirect_uri("http://127.0.0.1:8765/callback").unwrap();
        assert_eq!(config.bind_host, "127.0.0.1");
        assert_eq!(config.port, 8765);
        assert_eq!(config.path, "/callback");
    }

    #[test]
    fn rejects_non_loopback_redirect_uri() {
        let error = CallbackConfig::from_redirect_uri("https://example.com/callback").unwrap_err();
        assert!(error.to_string().contains("interactive login requires"));
    }

    #[test]
    fn parses_success_callback_request() {
        let config = CallbackConfig::from_redirect_uri("http://127.0.0.1:8765/callback").unwrap();
        let callback = parse_callback_request(
            "GET /callback?code=test-code&state=test-state HTTP/1.1\r\nHost: 127.0.0.1:8765\r\n\r\n",
            &config,
        )
        .unwrap();
        assert_eq!(callback.code, "test-code");
        assert_eq!(callback.state.as_deref(), Some("test-state"));
    }

    #[test]
    fn transaction_search_options_preserve_filters() {
        let args = TransactionSearchArgs {
            filters: TransactionFilterArgs {
                plan: None,
                last_knowledge_of_server: None,
                since_date: None,
                transaction_type: None,
                startdate: None,
                enddate: None,
                cleared_only: false,
                uncleared_only: false,
            },
            month: None,
            query: Some("restock".to_string()),
            payee: Some("amazon".to_string()),
            memo: None,
            account: Some("check".to_string()),
            category: Some("shop".to_string()),
        };

        let options = build_transaction_search_options(&args);
        assert_eq!(options.query.as_deref(), Some("restock"));
        assert_eq!(options.payee.as_deref(), Some("amazon"));
        assert_eq!(options.account.as_deref(), Some("check"));
        assert_eq!(options.category.as_deref(), Some("shop"));
    }

    #[test]
    fn export_payload_is_compatible_with_transactions_import() {
        let payload = build_import_export_payload(&[json!({
            "id": "transaction-1",
            "account_id": "account-1",
            "date": "2026-04-18",
            "amount": -1250,
            "payee_id": "payee-1",
            "memo": "coffee",
            "approved": true,
            "deleted": false,
            "subtransactions": [
                {
                    "amount": -1000,
                    "category_id": "cat-1",
                    "memo": "beans"
                },
                {
                    "amount": -250,
                    "deleted": true
                }
            ]
        })])
        .unwrap();

        assert_eq!(
            payload,
            json!({
                "transactions": [
                    {
                        "account_id": "account-1",
                        "date": "2026-04-18",
                        "amount": -1250,
                        "payee_id": "payee-1",
                        "memo": "coffee",
                        "approved": true,
                        "subtransactions": [
                            {
                                "amount": -1000,
                                "category_id": "cat-1",
                                "memo": "beans"
                            }
                        ]
                    }
                ]
            })
        );
    }

    #[test]
    fn export_payload_skips_deleted_transactions() {
        let payload = build_import_export_payload(&[json!({
            "account_id": "account-1",
            "date": "2026-04-18",
            "amount": 1000,
            "deleted": true
        })])
        .unwrap();

        assert_eq!(payload, json!({ "transactions": [] }));
    }
}
