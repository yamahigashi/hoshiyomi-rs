use std::net::IpAddr;
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use clap::{Args, Parser, Subcommand};
use url::Url;

const DEFAULT_API_BASE: &str = "https://api.github.com";
const DEFAULT_USER_AGENT: &str = "following-stars-rss";

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

#[derive(Debug, Args)]
pub struct CommonArgs {
    /// GitHub personal access token. Falls back to GITHUB_TOKEN env var.
    #[arg(long, env = "GITHUB_TOKEN")]
    pub github_token: Option<String>,

    /// Path to the SQLite database file.
    #[arg(
        long,
        env = "FOLLOWING_RSS_DB_PATH",
        default_value = "following-stars.db"
    )]
    pub db_path: PathBuf,

    /// Maximum concurrent GitHub API requests.
    #[arg(long, env = "FOLLOWING_RSS_MAX_CONCURRENCY", default_value_t = 5)]
    pub max_concurrency: usize,

    /// Number of feed items to emit.
    #[arg(long, env = "FOLLOWING_RSS_FEED_LENGTH", default_value_t = 100)]
    pub feed_length: usize,

    /// Default polling interval in minutes when no history exists.
    #[arg(
        long,
        env = "FOLLOWING_RSS_DEFAULT_INTERVAL_MINUTES",
        default_value_t = 60
    )]
    pub default_interval_minutes: i64,

    /// Minimum polling interval in minutes for highly active users.
    #[arg(long, env = "FOLLOWING_RSS_MIN_INTERVAL_MINUTES", default_value_t = 10)]
    pub min_interval_minutes: i64,

    /// Maximum polling interval in minutes for dormant users.
    #[arg(
        long,
        env = "FOLLOWING_RSS_MAX_INTERVAL_MINUTES",
        default_value_t = 7 * 24 * 60
    )]
    pub max_interval_minutes: i64,

    /// GitHub REST API base URL (useful for testing).
    #[arg(long, env = "FOLLOWING_RSS_API_BASE", default_value = DEFAULT_API_BASE)]
    pub api_base_url: String,

    /// Custom user-agent header value.
    #[arg(long, env = "FOLLOWING_RSS_USER_AGENT", default_value = DEFAULT_USER_AGENT)]
    pub user_agent: String,

    /// HTTP request timeout in seconds.
    #[arg(long, env = "FOLLOWING_RSS_TIMEOUT_SECS", default_value_t = 30)]
    pub timeout_secs: u64,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run an HTTP server that serves feed.xml and an HTML index, refreshing data periodically.
    Serve(ServeArgs),
}

#[derive(Debug, Args, Clone)]
pub struct ServeArgs {
    /// Address to bind the HTTP server to.
    #[arg(long, env = "FOLLOWING_RSS_BIND", default_value = "127.0.0.1")]
    pub bind: IpAddr,

    /// Port to bind the HTTP server to.
    #[arg(long, env = "FOLLOWING_RSS_PORT", default_value_t = 8080)]
    pub port: u16,

    /// Minutes between background refresh cycles.
    #[arg(long, env = "FOLLOWING_RSS_REFRESH_MINUTES", default_value_t = 15)]
    pub refresh_minutes: u64,
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
}

impl Config {
    pub fn from_cli() -> Result<Self> {
        let cli = Cli::parse();
        Config::from_parts(cli.common, cli.command)
    }

    fn from_parts(common: CommonArgs, command: Option<Command>) -> Result<Self> {
        let token = common.github_token.ok_or_else(|| {
            anyhow!("GitHub token is required (pass --github-token or set GITHUB_TOKEN)")
        })?;

        if common.max_concurrency == 0 {
            return Err(anyhow!("max concurrency must be greater than zero"));
        }

        if common.feed_length == 0 {
            return Err(anyhow!("feed length must be greater than zero"));
        }

        if common.min_interval_minutes <= 0 {
            return Err(anyhow!("min interval must be positive"));
        }

        if common.max_interval_minutes < common.min_interval_minutes {
            return Err(anyhow!("max interval must be >= min interval"));
        }

        let api_base_url = Url::parse(&common.api_base_url)
            .with_context(|| format!("invalid api base url: {}", common.api_base_url))?;

        let mode = match command {
            Some(Command::Serve(args)) => Mode::Serve(ServeOptions {
                bind: args.bind,
                port: args.port,
                refresh_minutes: validate_refresh_minutes(args.refresh_minutes)?,
            }),
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

fn validate_refresh_minutes(minutes: u64) -> Result<u64> {
    if minutes == 0 {
        Err(anyhow!("refresh minutes must be greater than zero"))
    } else {
        Ok(minutes)
    }
}
