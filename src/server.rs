use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use chrono::Utc;
use serde::Serialize;
use tokio::sync::Notify;
use warp::http::{HeaderValue, Response, StatusCode, header};
use warp::hyper::Body;
use warp::{Filter, Reply};

use crate::config::Mode;
use crate::db::init;
use crate::github::GitHubClient;
use crate::pipeline::{build_feed_xml, poll_once};
use crate::{Config, feed};

#[derive(Clone)]
pub struct AppState {
    config: Arc<Config>,
}

impl AppState {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config }
    }

    pub async fn feed_xml(&self) -> Result<String> {
        build_feed_xml(self.config.as_ref()).await
    }

    pub async fn html_page(&self) -> Result<String> {
        let events = self.recent_events().await?;
        let html = feed::build_html(&events, Utc::now());
        Ok(html)
    }

    pub async fn recent_events(&self) -> Result<Vec<crate::db::StarFeedRow>> {
        crate::db::recent_events_for_feed(&self.config.db_path, self.config.feed_length).await
    }

    pub fn config(&self) -> &Config {
        self.config.as_ref()
    }
}

pub async fn run_server(config: Config) -> Result<()> {
    let serve_options = match &config.mode {
        Mode::Serve(opts) => opts.clone(),
        _ => return Err(anyhow!("server mode requires --serve")),
    };

    init(&config.db_path).await?;
    let config = Arc::new(config);
    let client = Arc::new(GitHubClient::new(config.as_ref())?);

    poll_once(config.as_ref(), client.clone()).await?;

    let state = Arc::new(AppState::new(Arc::clone(&config)));

    let notify = Arc::new(Notify::new());

    let routes = routes(state.clone());
    let addr_tuple = (serve_options.bind, serve_options.port);
    let (listening_addr, server) = warp::serve(routes)
        .try_bind_with_graceful_shutdown(addr_tuple, shutdown_future(notify.clone()))?;

    println!(
        "Serving feed at http://{}:{}/ (feed.xml)",
        listening_addr.ip(),
        listening_addr.port()
    );

    let poller_config = Arc::clone(&config);
    let poller_client = client.clone();
    let poller_notify = notify.clone();
    let refresh_interval = Duration::from_secs(serve_options.refresh_minutes * 60);

    let poller = tokio::spawn(async move {
        let mut interval = tokio::time::interval(refresh_interval);
        interval.tick().await; // consume the immediate tick
        loop {
            tokio::select! {
                _ = poller_notify.notified() => break,
                _ = interval.tick() => {
                    if let Err(err) = poll_once(poller_config.as_ref(), poller_client.clone()).await {
                        eprintln!("Polling error: {err:?}");
                    }
                }
            }
        }
    });

    server.await;
    poller.await.ok();
    Ok(())
}

async fn shutdown_future(notify: Arc<Notify>) {
    if let Err(err) = tokio::signal::ctrl_c().await {
        eprintln!("Failed to listen for shutdown signal: {err}");
    }
    notify.notify_waiters();
}

pub fn routes(
    state: Arc<AppState>,
) -> impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone {
    let feed_route = warp::path("feed.xml")
        .and(warp::path::end())
        .and(with_state(state.clone()))
        .and_then(feed_handler);

    let index_route = warp::path::end()
        .and(with_state(state.clone()))
        .and_then(index_handler);

    let api_route = warp::path("api")
        .and(warp::path("stars"))
        .and(warp::path::end())
        .and(with_state(state.clone()))
        .and_then(stars_handler);

    feed_route.or(index_route).or(api_route)
}

fn with_state(
    state: Arc<AppState>,
) -> impl Filter<Extract = (Arc<AppState>,), Error = Infallible> + Clone {
    warp::any().map(move || state.clone())
}

async fn feed_handler(state: Arc<AppState>) -> Result<Response<Body>, Infallible> {
    match state.feed_xml().await {
        Ok(xml) => Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/rss+xml")
            .header("Cache-Control", "no-store")
            .body(Body::from(xml))
            .unwrap()),
        Err(err) => {
            eprintln!("Failed to render feed: {err:?}");
            Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(Body::from("Internal Server Error"))
                .unwrap())
        }
    }
}

async fn index_handler(state: Arc<AppState>) -> Result<Response<Body>, Infallible> {
    match state.html_page().await {
        Ok(html) => Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .header("Cache-Control", "no-store")
            .body(Body::from(html))
            .unwrap()),
        Err(err) => {
            eprintln!("Failed to render HTML: {err:?}");
            Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(Body::from("Internal Server Error"))
                .unwrap())
        }
    }
}

async fn stars_handler(state: Arc<AppState>) -> Result<Response<Body>, Infallible> {
    match state.recent_events().await {
        Ok(events) => {
            let data: Vec<StarEventResponse> =
                events.into_iter().map(StarEventResponse::from).collect();
            let reply = warp::reply::json(&data);
            let mut response = reply.into_response();
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            Ok(response)
        }
        Err(err) => {
            eprintln!("Failed to load star events: {err:?}");
            Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(Body::from("Internal Server Error"))
                .unwrap())
        }
    }
}

#[derive(Debug, Serialize)]
struct StarEventResponse {
    login: String,
    repo_full_name: String,
    repo_html_url: String,
    repo_description: Option<String>,
    repo_language: Option<String>,
    repo_topics: Vec<String>,
    starred_at: String,
    user_activity_tier: Option<String>,
}

impl From<crate::db::StarFeedRow> for StarEventResponse {
    fn from(row: crate::db::StarFeedRow) -> Self {
        Self {
            login: row.login,
            repo_full_name: row.repo_full_name,
            repo_html_url: row.repo_html_url,
            repo_description: row.repo_description,
            repo_language: row.repo_language,
            repo_topics: row.repo_topics,
            starred_at: row.starred_at.to_rfc3339(),
            user_activity_tier: row.user_activity_tier,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration as ChronoDuration, Utc};
    use rusqlite::{Connection, params};
    use serde_json::Value;
    use tempfile::NamedTempFile;
    use url::Url;

    #[tokio::test]
    async fn feed_handler_returns_xml() {
        let temp = NamedTempFile::new().unwrap();
        init(temp.path()).await.unwrap();
        let config = Arc::new(Config {
            github_token: "token".into(),
            db_path: temp.path().to_path_buf(),
            max_concurrency: 1,
            feed_length: 10,
            default_interval_minutes: 60,
            min_interval_minutes: 10,
            max_interval_minutes: 60,
            api_base_url: Url::parse("https://example.com").unwrap(),
            user_agent: "ua".into(),
            timeout_secs: 10,
            mode: Mode::Once,
        });
        let state = Arc::new(AppState::new(config));
        let routes = routes(state);
        let resp = warp::test::request().path("/feed.xml").reply(&routes).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn stars_endpoint_returns_json_payload() {
        let temp = NamedTempFile::new().unwrap();
        init(temp.path()).await.unwrap();

        let now = Utc::now();
        let conn = Connection::open(temp.path()).unwrap();
        conn.execute(
            "INSERT INTO users (user_id, login, last_starred_at, last_fetched_at, etag, last_modified, fetch_interval_minutes, next_check_at, activity_tier)
             VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5, ?6, ?7)",
            params![
                1,
                "alice",
                now.to_rfc3339(),
                now.to_rfc3339(),
                30,
                (now + ChronoDuration::minutes(30)).to_rfc3339(),
                "high"
            ],
        )
        .unwrap();
        let topics = serde_json::to_string(&vec!["rust", "cli"]).unwrap();
        conn.execute(
            "INSERT INTO stars (user_id, repo_full_name, repo_description, repo_language, repo_topics, repo_html_url, starred_at, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                1,
                "rust-lang/rust",
                "The Rust compiler",
                "Rust",
                topics,
                "https://github.com/rust-lang/rust",
                now.to_rfc3339(),
                now.to_rfc3339(),
            ],
        )
        .unwrap();

        let config = Arc::new(Config {
            github_token: "token".into(),
            db_path: temp.path().to_path_buf(),
            max_concurrency: 1,
            feed_length: 10,
            default_interval_minutes: 60,
            min_interval_minutes: 10,
            max_interval_minutes: 60 * 24,
            api_base_url: Url::parse("https://example.com").unwrap(),
            user_agent: "ua".into(),
            timeout_secs: 10,
            mode: Mode::Once,
        });
        let state = Arc::new(AppState::new(config));
        let routes = routes(state);
        let resp = warp::test::request()
            .path("/api/stars")
            .reply(&routes)
            .await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.body();
        let payload: Value = serde_json::from_slice(body).unwrap();
        assert!(payload.is_array());
        let first = payload.as_array().unwrap().first().unwrap();
        assert_eq!(first.get("login").unwrap(), "alice");
        assert_eq!(first.get("repo_full_name").unwrap(), "rust-lang/rust");
        assert_eq!(first.get("repo_language").unwrap(), "Rust");
        assert_eq!(first.get("user_activity_tier").unwrap(), "high");
        let topics = first.get("repo_topics").unwrap().as_array().unwrap();
        assert_eq!(topics.len(), 2);
    }
}
