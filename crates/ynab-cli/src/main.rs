use std::{
    io::{self, IsTerminal, Read, Write},
    net::TcpListener,
    process::ExitCode,
    time::{Duration, Instant},
};

use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use serde_json::Value;
use url::Url;
use ynab_core::{
    AmountMilliunits, AppState, OAuthAppInput, OAuthScope, OutputEnvelope, OutputFormat,
    ResolveByNameKind, ResourceListOptions, RuntimeOptions, TransactionCreateInput,
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
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    no_keyring: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputMode {
    Json,
    PrettyJson,
}

impl From<OutputMode> for OutputFormat {
    fn from(value: OutputMode) -> Self {
        match value {
            OutputMode::Json => OutputFormat::Json,
            OutputMode::PrettyJson => OutputFormat::PrettyJson,
        }
    }
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(subcommand)]
    Auth(AuthCommands),
    #[command(visible_alias = "budgets")]
    #[command(subcommand)]
    Plans(PlansCommands),
    #[command(subcommand)]
    Accounts(ResourceCommands),
    #[command(subcommand)]
    Categories(ResourceCommands),
    #[command(subcommand)]
    Payees(ResourceCommands),
    #[command(subcommand)]
    Transactions(Box<TransactionsCommands>),
}

#[derive(Debug, Subcommand)]
enum AuthCommands {
    Login(AuthLoginArgs),
    #[command(subcommand)]
    Token(TokenCommands),
    #[command(subcommand)]
    Oauth(OAuthCommands),
    Whoami,
    Logout,
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
    List(ListArgs),
    Get(PlanGetArgs),
    SetDefault(PlanDefaultArgs),
}

#[derive(Debug, Args)]
struct ListArgs {
    #[arg(long)]
    last_knowledge_of_server: Option<u64>,
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
enum ResourceCommands {
    List(ResourceListArgs),
}

#[derive(Debug, Args)]
struct ResourceListArgs {
    #[arg(long)]
    plan: Option<String>,
    #[arg(long)]
    last_knowledge_of_server: Option<u64>,
}

#[derive(Debug, Subcommand)]
enum TransactionsCommands {
    List(ResourceListArgs),
    Create(TransactionCreateArgs),
    Update(TransactionUpdateArgs),
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

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = run(cli).await;
    match result {
        Ok((format, envelope)) => {
            print_json(format, &serde_json::to_value(envelope).unwrap());
            ExitCode::SUCCESS
        }
        Err((format, error)) => {
            let value = serde_json::to_value(error.to_cli_envelope()).unwrap();
            eprintln!("{}", render_json(format, &value));
            ExitCode::from(1)
        }
    }
}

async fn run(cli: Cli) -> Result<(OutputFormat, OutputEnvelope), (OutputFormat, YnabError)> {
    let output_format: OutputFormat = cli.output.into();
    let mut app = AppState::load(RuntimeOptions {
        profile: cli.profile,
        use_keyring: !cli.no_keyring,
        base_url_override: cli.base_url,
        output_format,
    })
    .map_err(|error| (output_format, error))?;

    let result = match cli.command {
        Commands::Auth(command) => run_auth(&mut app, command).await,
        Commands::Plans(command) => run_plans(&mut app, command).await,
        Commands::Accounts(ResourceCommands::List(args)) => {
            let plan_id = app
                .resolve_plan_argument(args.plan)
                .map_err(|error| (app.output_format(), error))?;
            app.list_accounts(
                &plan_id,
                ResourceListOptions {
                    last_knowledge_of_server: args.last_knowledge_of_server,
                },
            )
            .await
        }
        Commands::Categories(ResourceCommands::List(args)) => {
            let plan_id = app
                .resolve_plan_argument(args.plan)
                .map_err(|error| (app.output_format(), error))?;
            app.list_categories(
                &plan_id,
                ResourceListOptions {
                    last_knowledge_of_server: args.last_knowledge_of_server,
                },
            )
            .await
        }
        Commands::Payees(ResourceCommands::List(args)) => {
            let plan_id = app
                .resolve_plan_argument(args.plan)
                .map_err(|error| (app.output_format(), error))?;
            app.list_payees(
                &plan_id,
                ResourceListOptions {
                    last_knowledge_of_server: args.last_knowledge_of_server,
                },
            )
            .await
        }
        Commands::Transactions(command) => run_transactions(&mut app, *command).await,
    };

    result
        .map(|value| (app.output_format(), value))
        .map_err(|error| (app.output_format(), error))
}

async fn run_auth(app: &mut AppState, command: AuthCommands) -> Result<OutputEnvelope, YnabError> {
    match command {
        AuthCommands::Login(args) => run_login(app, args).await,
        AuthCommands::Token(TokenCommands::Set(args)) => {
            let token = match args.token {
                Some(token) => token,
                None => read_stdin_token()?,
            };
            app.set_personal_access_token(token)
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
        AuthCommands::Logout => app.clear_session(),
    }
}

async fn run_plans(
    app: &mut AppState,
    command: PlansCommands,
) -> Result<OutputEnvelope, YnabError> {
    match command {
        PlansCommands::List(args) => {
            app.list_plans(ResourceListOptions {
                last_knowledge_of_server: args.last_knowledge_of_server,
            })
            .await
        }
        PlansCommands::Get(args) => app.get_plan(&args.plan_id).await,
        PlansCommands::SetDefault(args) => app.set_default_plan(&args.plan_id),
    }
}

async fn run_transactions(
    app: &mut AppState,
    command: TransactionsCommands,
) -> Result<OutputEnvelope, YnabError> {
    match command {
        TransactionsCommands::List(args) => {
            let plan_id = app.resolve_plan_argument(args.plan)?;
            app.list_transactions(
                &plan_id,
                ResourceListOptions {
                    last_knowledge_of_server: args.last_knowledge_of_server,
                },
            )
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

            app.create_transaction(TransactionCreateInput {
                plan_id,
                account_id,
                date: args.date,
                amount: AmountMilliunits::parse(&args.amount)?,
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
        TransactionsCommands::Update(args) => {
            let plan_id = resolve_plan_id(app, args.plan).await?;
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

            app.update_transaction(TransactionUpdateInput {
                plan_id,
                transaction_id: args.transaction_id,
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
    }
}

async fn resolve_plan_id(app: &mut AppState, plan: Option<String>) -> Result<String, YnabError> {
    app.resolve_plan_argument(plan)
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

fn print_json(format: OutputFormat, value: &Value) {
    println!("{}", render_json(format, value));
}

fn render_json(format: OutputFormat, value: &Value) -> String {
    match format {
        OutputFormat::Json => serde_json::to_string(value).unwrap(),
        OutputFormat::PrettyJson => serde_json::to_string_pretty(value).unwrap(),
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
    use super::{CallbackConfig, parse_callback_request};

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
}
