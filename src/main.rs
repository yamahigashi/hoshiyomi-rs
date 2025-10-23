use anyhow::Result;
use starchaser::Config;
use starchaser::config::Mode;
use starchaser::db::init;
use starchaser::github::GitHubClient;
use starchaser::pipeline::{build_feed_xml, poll_once};
use starchaser::server;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_cli()?;
    match &config.mode {
        Mode::Once => {
            let feed = run_once(&config).await?;
            println!("{}", feed);
            Ok(())
        }
        Mode::Serve(_) => server::run_server(config).await,
    }
}

async fn run_once(config: &Config) -> Result<String> {
    init(&config.db_path).await?;
    let client = Arc::new(GitHubClient::new(config)?);
    poll_once(config, client).await?;
    build_feed_xml(config).await
}
