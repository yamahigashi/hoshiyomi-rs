use std::collections::HashMap;
use std::fs;
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use clap::parser::ValueSource;
use clap::{ArgMatches, Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use dirs;
use serde::Deserialize;
use url::Url;

type HashMapStrOrigin = HashMap<&'static str, ValueOrigin>;

const DEFAULT_API_BASE: &str = "https://api.github.com";
const DEFAULT_USER_AGENT: &str = "following-stars-rss";
const DEFAULT_DB_PATH: &str = "following-stars.db";
const DEFAULT_MAX_CONCURRENCY: usize = 5;
const DEFAULT_FEED_LENGTH: usize = 100;
const DEFAULT_DEFAULT_INTERVAL: i64 = 60;
const DEFAULT_MIN_INTERVAL: i64 = 10;
const DEFAULT_MAX_INTERVAL: i64 = 7 * 24 * 60;
const DEFAULT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_BIND: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
const DEFAULT_PORT: u16 = 8080;
const DEFAULT_REFRESH_MINUTES: u64 = 15;

const ENV_GITHUB_TOKEN: &str = "GITHUB_TOKEN";
const ENV_DB_PATH: &str = "FOLLOWING_RSS_DB_PATH";
const ENV_MAX_CONCURRENCY: &str = "FOLLOWING_RSS_MAX_CONCURRENCY";
const ENV_FEED_LENGTH: &str = "FOLLOWING_RSS_FEED_LENGTH";
const ENV_DEFAULT_INTERVAL: &str = "FOLLOWING_RSS_DEFAULT_INTERVAL_MINUTES";
const ENV_MIN_INTERVAL: &str = "FOLLOWING_RSS_MIN_INTERVAL_MINUTES";
const ENV_MAX_INTERVAL: &str = "FOLLOWING_RSS_MAX_INTERVAL_MINUTES";
const ENV_API_BASE: &str = "FOLLOWING_RSS_API_BASE";
const ENV_USER_AGENT: &str = "FOLLOWING_RSS_USER_AGENT";
const ENV_TIMEOUT_SECS: &str = "FOLLOWING_RSS_TIMEOUT_SECS";
const ENV_CONFIG_PATH: &str = "FOLLOWING_RSS_CONFIG";
const ENV_SERVE_BIND: &str = "FOLLOWING_RSS_BIND";
const ENV_SERVE_PORT: &str = "FOLLOWING_RSS_PORT";
const ENV_SERVE_REFRESH: &str = "FOLLOWING_RSS_REFRESH_MINUTES";
const ENV_SERVE_PREFIX: &str = "FOLLOWING_RSS_SERVE_PREFIX";

const ARG_GITHUB_TOKEN: &str = "github_token";
const ARG_DB_PATH: &str = "db_path";
const ARG_MAX_CONCURRENCY: &str = "max_concurrency";
const ARG_FEED_LENGTH: &str = "feed_length";
const ARG_DEFAULT_INTERVAL: &str = "default_interval_minutes";
const ARG_MIN_INTERVAL: &str = "min_interval_minutes";
const ARG_MAX_INTERVAL: &str = "max_interval_minutes";
const ARG_API_BASE: &str = "api_base_url";
const ARG_USER_AGENT: &str = "user_agent";
const ARG_TIMEOUT_SECS: &str = "timeout_secs";
const ARG_SERVE_BIND: &str = "bind";
const ARG_SERVE_PORT: &str = "port";
const ARG_SERVE_REFRESH: &str = "refresh_minutes";
const ARG_SERVE_PREFIX: &str = "serve_prefix";

#[derive(Debug, Parser)]
#[command(
    name = "following-stars-rss",
    version,
    about = "Generate an RSS feed of repositories starred by the GitHub accounts you follow."
)]
pub struct Cli {
    #[command(flatten)]
    pub common: CommonArgs,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Args, Clone)]
pub struct CommonArgs {
    /// Path to the configuration file.
    #[arg(long, env = ENV_CONFIG_PATH, value_name = "PATH")]
    pub config_path: Option<PathBuf>,

    /// GitHub personal access token. Falls back to GITHUB_TOKEN env var.
    #[arg(long, env = ENV_GITHUB_TOKEN)]
    pub github_token: Option<String>,

    /// Path to the SQLite database file.
    #[arg(long, env = ENV_DB_PATH, default_value = DEFAULT_DB_PATH)]
    pub db_path: PathBuf,

    /// Maximum concurrent GitHub API requests.
    #[arg(long, env = ENV_MAX_CONCURRENCY, default_value_t = DEFAULT_MAX_CONCURRENCY)]
    pub max_concurrency: usize,

    /// Number of feed items to emit.
    #[arg(long, env = ENV_FEED_LENGTH, default_value_t = DEFAULT_FEED_LENGTH)]
    pub feed_length: usize,

    /// Default polling interval in minutes when no history exists.
    #[arg(long, env = ENV_DEFAULT_INTERVAL, default_value_t = DEFAULT_DEFAULT_INTERVAL)]
    pub default_interval_minutes: i64,

    /// Minimum polling interval in minutes for highly active users.
    #[arg(long, env = ENV_MIN_INTERVAL, default_value_t = DEFAULT_MIN_INTERVAL)]
    pub min_interval_minutes: i64,

    /// Maximum polling interval in minutes for dormant users.
    #[arg(long, env = ENV_MAX_INTERVAL, default_value_t = DEFAULT_MAX_INTERVAL)]
    pub max_interval_minutes: i64,

    /// GitHub REST API base URL (useful for testing).
    #[arg(long, env = ENV_API_BASE, default_value = DEFAULT_API_BASE)]
    pub api_base_url: String,

    /// Custom user-agent header value.
    #[arg(long, env = ENV_USER_AGENT, default_value = DEFAULT_USER_AGENT)]
    pub user_agent: String,

    /// HTTP request timeout in seconds.
    #[arg(long, env = ENV_TIMEOUT_SECS, default_value_t = DEFAULT_TIMEOUT_SECS)]
    pub timeout_secs: u64,
}

#[derive(Debug, Subcommand, Clone)]
pub enum Command {
    /// Run an HTTP server that serves feed.xml and an HTML index, refreshing data periodically.
    Serve(ServeArgs),
}

#[derive(Debug, Args, Clone)]
pub struct ServeArgs {
    /// Address to bind the HTTP server to.
    #[arg(long, env = ENV_SERVE_BIND, default_value = "127.0.0.1")]
    pub bind: IpAddr,

    /// Port to bind the HTTP server to.
    #[arg(long, env = ENV_SERVE_PORT, default_value_t = DEFAULT_PORT)]
    pub port: u16,

    /// Minutes between background refresh cycles.
    #[arg(long, env = ENV_SERVE_REFRESH, default_value_t = DEFAULT_REFRESH_MINUTES)]
    pub refresh_minutes: u64,

    /// Optional path prefix when serving behind a reverse proxy.
    #[arg(long, env = ENV_SERVE_PREFIX, default_value = "")]
    pub serve_prefix: String,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub github_token: String,
    pub db_path: PathBuf,
    pub max_concurrency: usize,
    pub feed_length: usize,
    pub default_interval_minutes: i64,
    pub min_interval_minutes: i64,
    pub max_interval_minutes: i64,
    pub api_base_url: Url,
    pub user_agent: String,
    pub timeout_secs: u64,
    pub mode: Mode,
}

#[derive(Debug, Clone)]
pub enum Mode {
    Once,
    Serve(ServeOptions),
}

#[derive(Debug, Clone)]
pub struct ServeOptions {
    pub bind: IpAddr,
    pub port: u16,
    pub refresh_minutes: u64,
    pub serve_prefix: String,
}

impl Config {
    pub fn from_cli() -> Result<Self> {
        let command = Cli::command();
        let matches = command.clone().get_matches();
        let cli = Cli::from_arg_matches(&matches).expect("validated by clap");
        let loaded_config = load_config_file(cli.common.config_path.as_deref())?;
        Config::from_matches(cli, &matches, loaded_config)
    }

    fn from_matches(cli: Cli, matches: &ArgMatches, loaded: Option<LoadedConfig>) -> Result<Self> {
        let merge_result = merge_configuration(&cli, matches, loaded.as_ref());
        Config::from_parts(
            merge_result.common,
            merge_result.command,
            merge_result.origins,
        )
    }

    fn from_parts(
        common: CommonArgs,
        command: Option<Command>,
        origins: FieldOrigins,
    ) -> Result<Self> {
        let token = common.github_token.ok_or_else(|| {
            anyhow!(
                "GitHub token is required (set via --github-token / {ENV_GITHUB_TOKEN} or config file github.token)"
            )
        })?;

        if common.max_concurrency == 0 {
            let origin = origins.describe("max_concurrency");
            return Err(anyhow!(
                "max concurrency must be greater than zero (source: {origin})"
            ));
        }

        if common.feed_length == 0 {
            let origin = origins.describe("feed_length");
            return Err(anyhow!(
                "feed length must be greater than zero (source: {origin})"
            ));
        }

        if common.min_interval_minutes <= 0 {
            let origin = origins.describe("min_interval_minutes");
            return Err(anyhow!("min interval must be positive (source: {origin})"));
        }

        if common.max_interval_minutes < common.min_interval_minutes {
            let max_origin = origins.describe("max_interval_minutes");
            let min_origin = origins.describe("min_interval_minutes");
            return Err(anyhow!(
                "max interval must be >= min interval (sources: max={max_origin}, min={min_origin})",
            ));
        }

        let api_origin = origins.describe("api_base_url");
        let api_base_url = Url::parse(&common.api_base_url).with_context(|| {
            format!(
                "invalid api base url '{}' (source: {})",
                common.api_base_url, api_origin
            )
        })?;

        let mode = match command {
            Some(Command::Serve(args)) => {
                let origin = origins.describe("refresh_minutes");
                let refresh_minutes = validate_refresh_minutes(args.refresh_minutes, &origin)?;
                let serve_prefix = canonicalize_prefix(&args.serve_prefix).with_context(|| {
                    let prefix_origin = origins.describe("serve_prefix");
                    format!("invalid serve prefix (source: {prefix_origin})")
                })?;
                Mode::Serve(ServeOptions {
                    bind: args.bind,
                    port: args.port,
                    refresh_minutes,
                    serve_prefix,
                })
            }
            None => Mode::Once,
        };

        Ok(Self {
            github_token: token,
            db_path: common.db_path,
            max_concurrency: common.max_concurrency,
            feed_length: common.feed_length,
            default_interval_minutes: common.default_interval_minutes,
            min_interval_minutes: common.min_interval_minutes,
            max_interval_minutes: common.max_interval_minutes,
            api_base_url,
            user_agent: common.user_agent,
            timeout_secs: common.timeout_secs,
            mode,
        })
    }

    pub fn serve_options(&self) -> Option<&ServeOptions> {
        if let Mode::Serve(opts) = &self.mode {
            Some(opts)
        } else {
            None
        }
    }
}

fn validate_refresh_minutes(minutes: u64, origin: &str) -> Result<u64> {
    if minutes == 0 {
        Err(anyhow!(
            "refresh minutes must be greater than zero (source: {origin})"
        ))
    } else {
        Ok(minutes)
    }
}

pub fn canonicalize_prefix(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.contains(char::is_whitespace) {
        return Err(anyhow!("serve prefix must not contain whitespace"));
    }
    let cleaned_segments: Vec<&str> = trimmed.split('/').filter(|s| !s.is_empty()).collect();
    if cleaned_segments.is_empty() {
        return Ok(String::new());
    }
    let mut normalized = String::new();
    for segment in cleaned_segments {
        normalized.push('/');
        normalized.push_str(segment);
    }
    if normalized == "/" {
        Ok(String::new())
    } else {
        Ok(normalized)
    }
}

struct MergeResult {
    common: CommonArgs,
    command: Option<Command>,
    origins: FieldOrigins,
}

fn merge_configuration(
    cli: &Cli,
    matches: &ArgMatches,
    loaded: Option<&LoadedConfig>,
) -> MergeResult {
    let mut origins = FieldOrigins::default();
    let mut common = cli.common.clone();
    let mut command = cli.command.clone();

    let github_cfg = loaded.and_then(|cfg| cfg.values.github.as_ref());
    let polling_cfg = loaded.and_then(|cfg| cfg.values.polling.as_ref());
    let app_cfg = loaded.and_then(|cfg| cfg.values.app.as_ref());
    let server_cfg = loaded.and_then(|cfg| cfg.values.server.as_ref());

    // github token
    let file_github_token = github_cfg.and_then(|g| g.token.clone());
    let (github_token, used_config_github) = merge_option(
        matches,
        ARG_GITHUB_TOKEN,
        common.github_token.clone(),
        file_github_token,
    );
    common.github_token = github_token;
    origins.set(
        "github_token",
        determine_origin(
            matches,
            ARG_GITHUB_TOKEN,
            "--github-token",
            Some(ENV_GITHUB_TOKEN),
            used_config_github,
            loaded,
            "github.token",
        ),
    );

    // db path
    let file_db_path = app_cfg.and_then(|a| a.db_path.clone());
    let (db_path, used_config_db) =
        merge_scalar(matches, ARG_DB_PATH, common.db_path.clone(), file_db_path);
    common.db_path = db_path;
    origins.set(
        "db_path",
        determine_origin(
            matches,
            ARG_DB_PATH,
            "--db-path",
            Some(ENV_DB_PATH),
            used_config_db,
            loaded,
            "app.db_path",
        ),
    );

    // max concurrency
    let file_max_concurrency = app_cfg.and_then(|a| a.max_concurrency);
    let (max_concurrency, used_config_max_concurrency) = merge_scalar(
        matches,
        ARG_MAX_CONCURRENCY,
        common.max_concurrency,
        file_max_concurrency,
    );
    common.max_concurrency = max_concurrency;
    origins.set(
        "max_concurrency",
        determine_origin(
            matches,
            ARG_MAX_CONCURRENCY,
            "--max-concurrency",
            Some(ENV_MAX_CONCURRENCY),
            used_config_max_concurrency,
            loaded,
            "app.max_concurrency",
        ),
    );

    // feed length
    let file_feed_length = polling_cfg.and_then(|p| p.feed_length);
    let (feed_length, used_config_feed_length) = merge_scalar(
        matches,
        ARG_FEED_LENGTH,
        common.feed_length,
        file_feed_length,
    );
    common.feed_length = feed_length;
    origins.set(
        "feed_length",
        determine_origin(
            matches,
            ARG_FEED_LENGTH,
            "--feed-length",
            Some(ENV_FEED_LENGTH),
            used_config_feed_length,
            loaded,
            "polling.feed_length",
        ),
    );

    // default interval
    let file_default_interval = polling_cfg.and_then(|p| p.default_interval_minutes);
    let (default_interval, used_config_default_interval) = merge_scalar(
        matches,
        ARG_DEFAULT_INTERVAL,
        common.default_interval_minutes,
        file_default_interval,
    );
    common.default_interval_minutes = default_interval;
    origins.set(
        "default_interval_minutes",
        determine_origin(
            matches,
            ARG_DEFAULT_INTERVAL,
            "--default-interval-minutes",
            Some(ENV_DEFAULT_INTERVAL),
            used_config_default_interval,
            loaded,
            "polling.default_interval_minutes",
        ),
    );

    // min interval
    let file_min_interval = polling_cfg.and_then(|p| p.min_interval_minutes);
    let (min_interval, used_config_min_interval) = merge_scalar(
        matches,
        ARG_MIN_INTERVAL,
        common.min_interval_minutes,
        file_min_interval,
    );
    common.min_interval_minutes = min_interval;
    origins.set(
        "min_interval_minutes",
        determine_origin(
            matches,
            ARG_MIN_INTERVAL,
            "--min-interval-minutes",
            Some(ENV_MIN_INTERVAL),
            used_config_min_interval,
            loaded,
            "polling.min_interval_minutes",
        ),
    );

    // max interval
    let file_max_interval = polling_cfg.and_then(|p| p.max_interval_minutes);
    let (max_interval, used_config_max_interval) = merge_scalar(
        matches,
        ARG_MAX_INTERVAL,
        common.max_interval_minutes,
        file_max_interval,
    );
    common.max_interval_minutes = max_interval;
    origins.set(
        "max_interval_minutes",
        determine_origin(
            matches,
            ARG_MAX_INTERVAL,
            "--max-interval-minutes",
            Some(ENV_MAX_INTERVAL),
            used_config_max_interval,
            loaded,
            "polling.max_interval_minutes",
        ),
    );

    // api base url
    let file_api_base = app_cfg.and_then(|a| a.api_base_url.clone());
    let (api_base_url, used_config_api_base) = merge_scalar(
        matches,
        ARG_API_BASE,
        common.api_base_url.clone(),
        file_api_base,
    );
    common.api_base_url = api_base_url;
    origins.set(
        "api_base_url",
        determine_origin(
            matches,
            ARG_API_BASE,
            "--api-base-url",
            Some(ENV_API_BASE),
            used_config_api_base,
            loaded,
            "app.api_base_url",
        ),
    );

    // user agent
    let file_user_agent = app_cfg.and_then(|a| a.user_agent.clone());
    let (user_agent, used_config_user_agent) = merge_scalar(
        matches,
        ARG_USER_AGENT,
        common.user_agent.clone(),
        file_user_agent,
    );
    common.user_agent = user_agent;
    origins.set(
        "user_agent",
        determine_origin(
            matches,
            ARG_USER_AGENT,
            "--user-agent",
            Some(ENV_USER_AGENT),
            used_config_user_agent,
            loaded,
            "app.user_agent",
        ),
    );

    // timeout
    let file_timeout = app_cfg.and_then(|a| a.timeout_secs);
    let (timeout_secs, used_config_timeout) =
        merge_scalar(matches, ARG_TIMEOUT_SECS, common.timeout_secs, file_timeout);
    common.timeout_secs = timeout_secs;
    origins.set(
        "timeout_secs",
        determine_origin(
            matches,
            ARG_TIMEOUT_SECS,
            "--timeout-secs",
            Some(ENV_TIMEOUT_SECS),
            used_config_timeout,
            loaded,
            "app.timeout_secs",
        ),
    );

    // server configuration
    let serve_matches = matches.subcommand_matches("serve");
    match command {
        Some(Command::Serve(mut serve_args)) => {
            let file_bind = server_cfg.and_then(|s| s.bind);
            let (bind, _used_config_bind) =
                merge_scalar_subcommand(serve_matches, ARG_SERVE_BIND, serve_args.bind, file_bind);
            serve_args.bind = bind;

            let file_port = server_cfg.and_then(|s| s.port);
            let (port, _used_config_port) =
                merge_scalar_subcommand(serve_matches, ARG_SERVE_PORT, serve_args.port, file_port);
            serve_args.port = port;

            let file_refresh = server_cfg.and_then(|s| s.refresh_minutes);
            let (refresh_minutes, used_config_refresh) = merge_scalar_subcommand(
                serve_matches,
                ARG_SERVE_REFRESH,
                serve_args.refresh_minutes,
                file_refresh,
            );
            serve_args.refresh_minutes = refresh_minutes;

            origins.set(
                "refresh_minutes",
                determine_origin_subcommand(
                    serve_matches,
                    ARG_SERVE_REFRESH,
                    "serve --refresh-minutes",
                    Some(ENV_SERVE_REFRESH),
                    used_config_refresh,
                    loaded,
                    "server.refresh_minutes",
                ),
            );

            let file_prefix = server_cfg.and_then(|s| s.prefix.clone());
            let (serve_prefix, used_config_prefix) = merge_scalar_subcommand(
                serve_matches,
                ARG_SERVE_PREFIX,
                serve_args.serve_prefix.clone(),
                file_prefix,
            );
            serve_args.serve_prefix = serve_prefix;
            origins.set(
                "serve_prefix",
                determine_origin_subcommand(
                    serve_matches,
                    ARG_SERVE_PREFIX,
                    "serve --serve-prefix",
                    Some(ENV_SERVE_PREFIX),
                    used_config_prefix,
                    loaded,
                    "server.prefix",
                ),
            );

            command = Some(Command::Serve(serve_args));
        }
        None => {
            if let Some(server) = server_cfg
                && server.enable.unwrap_or(false)
            {
                let bind = server.bind.unwrap_or(DEFAULT_BIND);
                let port = server.port.unwrap_or(DEFAULT_PORT);
                let refresh_minutes = server.refresh_minutes.unwrap_or(DEFAULT_REFRESH_MINUTES);
                let serve_prefix = server.prefix.clone().unwrap_or_else(String::new);
                origins.set(
                    "refresh_minutes",
                    loaded
                        .map(|cfg| ValueOrigin::Config {
                            path: cfg.path.clone(),
                            key: "server.refresh_minutes",
                        })
                        .unwrap_or(ValueOrigin::Default),
                );
                command = Some(Command::Serve(ServeArgs {
                    bind,
                    port,
                    refresh_minutes,
                    serve_prefix,
                }));
            }
        }
    }

    MergeResult {
        common,
        command,
        origins,
    }
}

fn merge_scalar<T: Clone>(
    matches: &ArgMatches,
    arg_name: &'static str,
    current: T,
    config_value: Option<T>,
) -> (T, bool) {
    match matches.value_source(arg_name) {
        Some(ValueSource::CommandLine) | Some(ValueSource::EnvVariable) => (current, false),
        _ => config_value.map(|v| (v, true)).unwrap_or((current, false)),
    }
}

fn merge_option<T: Clone>(
    matches: &ArgMatches,
    arg_name: &'static str,
    current: Option<T>,
    config_value: Option<T>,
) -> (Option<T>, bool) {
    match matches.value_source(arg_name) {
        Some(ValueSource::CommandLine) | Some(ValueSource::EnvVariable) => (current, false),
        _ => {
            if let Some(value) = config_value {
                (Some(value), true)
            } else {
                (current, false)
            }
        }
    }
}

fn merge_scalar_subcommand<T: Clone>(
    sub_matches: Option<&ArgMatches>,
    arg_name: &'static str,
    current: T,
    config_value: Option<T>,
) -> (T, bool) {
    match sub_matches.and_then(|m| m.value_source(arg_name)) {
        Some(ValueSource::CommandLine) | Some(ValueSource::EnvVariable) => (current, false),
        _ => config_value.map(|v| (v, true)).unwrap_or((current, false)),
    }
}

fn determine_origin(
    matches: &ArgMatches,
    arg_name: &'static str,
    flag_repr: &'static str,
    env_var: Option<&'static str>,
    used_config: bool,
    loaded: Option<&LoadedConfig>,
    config_key: &'static str,
) -> ValueOrigin {
    match matches.value_source(arg_name) {
        Some(ValueSource::CommandLine) => ValueOrigin::Flag(flag_repr),
        Some(ValueSource::EnvVariable) => ValueOrigin::Env(env_var.unwrap_or("")),
        _ => {
            if used_config {
                if let Some(cfg) = loaded {
                    ValueOrigin::Config {
                        path: cfg.path.clone(),
                        key: config_key,
                    }
                } else {
                    ValueOrigin::Default
                }
            } else {
                ValueOrigin::Default
            }
        }
    }
}

fn determine_origin_subcommand(
    sub_matches: Option<&ArgMatches>,
    arg_name: &'static str,
    flag_repr: &'static str,
    env_var: Option<&'static str>,
    used_config: bool,
    loaded: Option<&LoadedConfig>,
    config_key: &'static str,
) -> ValueOrigin {
    match sub_matches.and_then(|m| m.value_source(arg_name)) {
        Some(ValueSource::CommandLine) => ValueOrigin::Flag(flag_repr),
        Some(ValueSource::EnvVariable) => ValueOrigin::Env(env_var.unwrap_or("")),
        _ => {
            if used_config {
                if let Some(cfg) = loaded {
                    ValueOrigin::Config {
                        path: cfg.path.clone(),
                        key: config_key,
                    }
                } else {
                    ValueOrigin::Default
                }
            } else {
                ValueOrigin::Default
            }
        }
    }
}

#[derive(Debug, Default)]
struct FieldOrigins {
    map: HashMapStrOrigin,
}

impl FieldOrigins {
    fn set(&mut self, key: &'static str, origin: ValueOrigin) {
        self.map.insert(key, origin);
    }

    fn describe(&self, key: &'static str) -> String {
        self.map
            .get(key)
            .map(|origin| origin.describe())
            .unwrap_or_else(|| "default value".to_string())
    }
}

#[derive(Debug, Clone)]
enum ValueOrigin {
    Flag(&'static str),
    Env(&'static str),
    Config { path: PathBuf, key: &'static str },
    Default,
}

impl ValueOrigin {
    fn describe(&self) -> String {
        match self {
            ValueOrigin::Flag(flag) => format!("flag {flag}"),
            ValueOrigin::Env(var) => format!("environment variable {var}"),
            ValueOrigin::Config { path, key } => {
                format!("config file {} (key {})", path.display(), key)
            }
            ValueOrigin::Default => "default value".to_string(),
        }
    }
}

fn load_config_file(path: Option<&Path>) -> Result<Option<LoadedConfig>> {
    if let Some(explicit) = path {
        let config = parse_config_file(explicit)
            .with_context(|| format!("failed to load config file at {}", explicit.display()))?;
        Ok(Some(config))
    } else {
        for candidate in default_config_paths() {
            if candidate.exists() {
                return parse_config_file(&candidate)
                    .with_context(|| {
                        format!("failed to load config file at {}", candidate.display())
                    })
                    .map(Some);
            }
        }
        Ok(None)
    }
}

fn default_config_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(current_dir) = std::env::current_dir() {
        paths.push(current_dir.join("hoshiyomi.toml"));
    }
    if let Some(mut config_dir) = dirs::config_dir() {
        config_dir.push("hoshiyomi");
        paths.push(config_dir.join("config.toml"));
    }
    paths
}

fn parse_config_file(path: &Path) -> Result<LoadedConfig> {
    if !path.exists() {
        return Err(anyhow!("config file not found at {}", path.display()));
    }
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    let values: FileConfig = toml::from_str(&contents)
        .with_context(|| format!("failed to parse config file {}", path.display()))?;
    Ok(LoadedConfig {
        path: path.to_path_buf(),
        values,
    })
}

struct LoadedConfig {
    path: PathBuf,
    values: FileConfig,
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    #[serde(default)]
    app: Option<AppSection>,
    #[serde(default)]
    github: Option<GithubSection>,
    #[serde(default)]
    polling: Option<PollingSection>,
    #[serde(default)]
    server: Option<ServerSection>,
}

#[derive(Debug, Default, Deserialize)]
struct AppSection {
    db_path: Option<PathBuf>,
    max_concurrency: Option<usize>,
    api_base_url: Option<String>,
    user_agent: Option<String>,
    timeout_secs: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct GithubSection {
    token: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct PollingSection {
    feed_length: Option<usize>,
    default_interval_minutes: Option<i64>,
    min_interval_minutes: Option<i64>,
    max_interval_minutes: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
struct ServerSection {
    enable: Option<bool>,
    bind: Option<IpAddr>,
    port: Option<u16>,
    refresh_minutes: Option<u64>,
    prefix: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    struct EnvGuard {
        key: &'static str,
        original: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let original = std::env::var(key).ok();
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, original }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(ref value) = self.original {
                unsafe {
                    std::env::set_var(self.key, value);
                }
            } else {
                unsafe {
                    std::env::remove_var(self.key);
                }
            }
        }
    }

    fn build_config_from_args(args: &[&str]) -> Result<Config> {
        let command = Cli::command();
        let matches = command.clone().try_get_matches_from(args)?;
        let cli = Cli::from_arg_matches(&matches).expect("validated by clap");
        let loaded = load_config_file(cli.common.config_path.as_deref())?;
        Config::from_matches(cli, &matches, loaded)
    }

    fn create_config_file(contents: &str) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().expect("tmp file");
        let mut handle = File::create(file.path()).expect("open tmp");
        handle.write_all(contents.as_bytes()).expect("write tmp");
        file
    }

    #[test]
    fn flag_overrides_config_file() {
        let cfg = create_config_file(
            r#"
            [github]
            token = "file-token"

            [polling]
            feed_length = 50
            "#,
        );
        let cfg_path = cfg.path().to_str().unwrap();
        let args = [
            "hoshiyomi",
            "--config-path",
            cfg_path,
            "--github-token",
            "flag-token",
            "--feed-length",
            "25",
        ];

        let config = build_config_from_args(&args).expect("config");
        assert_eq!(config.github_token, "flag-token");
        assert_eq!(config.feed_length, 25);
    }

    #[test]
    fn env_overrides_config_file() {
        let cfg = create_config_file(
            r#"
            [github]
            token = "file-token"

            [polling]
            feed_length = 50
            "#,
        );
        let cfg_path = cfg.path().to_str().unwrap();
        let guard = EnvGuard::set(ENV_FEED_LENGTH, "40");

        let args = [
            "hoshiyomi",
            "--config-path",
            cfg_path,
            "--github-token",
            "flag-token",
        ];
        let config = build_config_from_args(&args).expect("config");
        assert_eq!(config.feed_length, 40);
        drop(guard);
    }

    #[test]
    fn invalid_value_reports_config_source() {
        let cfg = create_config_file(
            r#"
            [github]
            token = "file-token"

            [polling]
            min_interval_minutes = 0
            "#,
        );
        let cfg_path = cfg.path().to_str().unwrap();
        let args = ["hoshiyomi", "--config-path", cfg_path];

        let err = build_config_from_args(&args).unwrap_err();
        let message = format!("{err}");
        assert!(message.contains("min interval must be positive"));
        assert!(message.contains(cfg_path));
    }
}
