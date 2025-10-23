use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::Notify;
use warp::http::{HeaderValue, StatusCode, header};
use warp::reply::Response as WarpResponse;
use warp::{Filter, Reply};

use crate::config::Mode;
use crate::db::init;
use crate::github::GitHubClient;
use crate::pipeline::{build_feed_xml, poll_once};
use crate::{Config, feed};

const DEFAULT_PAGE_SIZE: u32 = 25;
const MAX_PAGE_SIZE: u32 = 100;
const CACHE_CONTROL_STARS: &str = "private, max-age=0";
const CACHE_CONTROL_STATUS: &str = "private, max-age=30, stale-while-revalidate=30";
const CACHE_CONTROL_OPTIONS: &str = "public, max-age=300";

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

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SortOrder {
    Newest,
    Alpha,
}

impl Default for SortOrder {
    fn default() -> Self {
        SortOrder::Newest
    }
}

impl SortOrder {
    fn as_str(&self) -> &'static str {
        match self {
            SortOrder::Newest => "newest",
            SortOrder::Alpha => "alpha",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum UserMode {
    All,
    Pin,
    Exclude,
}

impl Default for UserMode {
    fn default() -> Self {
        UserMode::All
    }
}

impl UserMode {
    fn as_str(&self) -> &'static str {
        match self {
            UserMode::All => "all",
            UserMode::Pin => "pin",
            UserMode::Exclude => "exclude",
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct StarQueryParams {
    q: Option<String>,
    language: Option<String>,
    activity: Option<String>,
    #[serde(default)]
    user_mode: UserMode,
    user: Option<String>,
    #[serde(default)]
    sort: SortOrder,
    #[serde(default = "default_page")]
    page: u32,
    #[serde(default = "default_page_size")]
    page_size: u32,
}

impl StarQueryParams {
    fn page(&self) -> u32 {
        self.page.max(1)
    }

    fn page_size(&self) -> u32 {
        self.page_size.clamp(1, MAX_PAGE_SIZE)
    }

    fn normalized_key(&self) -> String {
        // Build a stable ordering for all provided filters so cache keys are deterministic.
        let mut parts = BTreeMap::new();
        if let Some(value) = self.q.as_ref().filter(|v| !v.is_empty()) {
            parts.insert("q", value.trim().to_string());
        }
        if let Some(value) = self.language.as_ref().filter(|v| !v.is_empty()) {
            parts.insert("language", value.trim().to_string());
        }
        if let Some(value) = self.activity.as_ref().filter(|v| !v.is_empty()) {
            parts.insert("activity", value.trim().to_string());
        }
        if let Some(value) = self.user.as_ref().filter(|v| !v.is_empty()) {
            parts.insert("user", value.trim().to_string());
        }
        parts.insert("user_mode", self.user_mode.as_str().to_string());
        parts.insert("sort", self.sort.as_str().to_string());
        parts.insert("page", self.page().to_string());
        parts.insert("page_size", self.page_size().to_string());
        parts
            .into_iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&")
    }
}

fn default_page() -> u32 {
    1
}

fn default_page_size() -> u32 {
    DEFAULT_PAGE_SIZE
}

#[derive(Debug, Serialize)]
struct StarListMeta {
    page: u32,
    page_size: u32,
    total: usize,
    has_next: bool,
    has_prev: bool,
    etag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_modified: Option<String>,
}

#[derive(Debug, Serialize)]
struct StarListResponse {
    items: Vec<StarEventResponse>,
    meta: StarListMeta,
}

#[derive(Debug, Default, Serialize)]
struct NextCheckAt {
    #[serde(skip_serializing_if = "Option::is_none")]
    high: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    medium: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    low: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unknown: Option<String>,
}

#[derive(Debug, Serialize)]
struct StatusResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    last_poll_started: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_poll_finished: Option<String>,
    is_stale: bool,
    next_check_at: NextCheckAt,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rate_limit_remaining: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rate_limit_reset: Option<String>,
}

impl Default for StatusResponse {
    fn default() -> Self {
        Self {
            last_poll_started: None,
            last_poll_finished: None,
            is_stale: false,
            next_check_at: NextCheckAt::default(),
            last_error: None,
            rate_limit_remaining: None,
            rate_limit_reset: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct OptionsResponse {
    languages: Vec<LanguageOption>,
    activity_tiers: Vec<ActivityTierOption>,
    users: Vec<UserOption>,
    meta: OptionsMeta,
}

#[derive(Debug, Serialize)]
struct LanguageOption {
    name: String,
    count: u32,
}

#[derive(Debug, Serialize)]
struct ActivityTierOption {
    tier: String,
    count: u32,
}

#[derive(Debug, Serialize)]
struct UserOption {
    login: String,
    display_name: String,
    count: u32,
}

#[derive(Debug, Serialize)]
struct OptionsMeta {
    etag: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_modified: Option<String>,
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
    let listener = TcpListener::bind(addr_tuple).await?;
    let listening_addr = listener.local_addr()?;
    let server_future = warp::serve(routes)
        .incoming(listener)
        .graceful(shutdown_future(notify.clone()))
        .run();

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

    server_future.await;
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

    let stars_route = warp::path("api")
        .and(warp::path("stars"))
        .and(warp::path::end())
        .and(warp::query::<StarQueryParams>())
        .and(warp::header::optional::<String>("if-none-match"))
        .and(with_state(state.clone()))
        .and_then(stars_handler);

    let status_route = warp::path("api")
        .and(warp::path("status"))
        .and(warp::path::end())
        .and(warp::header::optional::<String>("if-none-match"))
        .and(with_state(state.clone()))
        .and_then(status_handler);

    let options_route = warp::path("api")
        .and(warp::path("options"))
        .and(warp::path::end())
        .and(warp::header::optional::<String>("if-none-match"))
        .and(with_state(state))
        .and_then(options_handler);

    feed_route
        .or(index_route)
        .or(stars_route)
        .or(status_route)
        .or(options_route)
}

fn with_state(
    state: Arc<AppState>,
) -> impl Filter<Extract = (Arc<AppState>,), Error = Infallible> + Clone {
    warp::any().map(move || state.clone())
}

async fn feed_handler(state: Arc<AppState>) -> Result<WarpResponse, Infallible> {
    match state.feed_xml().await {
        Ok(xml) => {
            let mut response = WarpResponse::new(xml.into());
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/rss+xml"),
            );
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            Ok(response)
        }
        Err(err) => {
            eprintln!("Failed to render feed: {err:?}");
            let mut response = WarpResponse::new("Internal Server Error".to_string().into());
            *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            );
            Ok(response)
        }
    }
}

async fn index_handler(state: Arc<AppState>) -> Result<WarpResponse, Infallible> {
    match state.html_page().await {
        Ok(html) => {
            let mut response = WarpResponse::new(html.into());
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            );
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            Ok(response)
        }
        Err(err) => {
            eprintln!("Failed to render HTML: {err:?}");
            let mut response = WarpResponse::new("Internal Server Error".to_string().into());
            *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            );
            Ok(response)
        }
    }
}

async fn stars_handler(
    params: StarQueryParams,
    if_none_match: Option<String>,
    state: Arc<AppState>,
) -> Result<WarpResponse, Infallible> {
    match state.recent_events().await {
        Ok(events) => {
            let total = events.len();
            let page = params.page();
            let page_size = params.page_size();
            let start = ((page - 1) as usize).saturating_mul(page_size as usize);
            let end = start.saturating_add(page_size as usize);
            let newest_fetched = events.first().map(|e| e.fetched_at);
            let etag_value = compute_stars_etag(&params, newest_fetched, total);

            if should_return_not_modified(if_none_match.as_deref(), &etag_value) {
                let mut response = WarpResponse::new(Vec::<u8>::new().into());
                *response.status_mut() = StatusCode::NOT_MODIFIED;
                insert_cache_headers(
                    &mut response,
                    &etag_value,
                    newest_fetched,
                    CACHE_CONTROL_STARS,
                );
                return Ok(response);
            }

            let page_events = events
                .into_iter()
                .skip(start)
                .take(page_size as usize)
                .map(StarEventResponse::from)
                .collect::<Vec<_>>();
            let has_next = total > end;
            let has_prev = page > 1 && total > 0;
            let last_modified = newest_fetched.map(|ts| ts.to_rfc2822());

            let response_body = StarListResponse {
                items: page_events,
                meta: StarListMeta {
                    page,
                    page_size,
                    total,
                    has_next,
                    has_prev,
                    etag: etag_value.clone(),
                    last_modified,
                },
            };
            let reply = warp::reply::json(&response_body);
            let mut response = reply.into_response();
            insert_cache_headers(
                &mut response,
                &etag_value,
                newest_fetched,
                CACHE_CONTROL_STARS,
            );
            Ok(response)
        }
        Err(err) => {
            eprintln!("Failed to load star events: {err:?}");
            let mut response = WarpResponse::new("Internal Server Error".to_string().into());
            *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            );
            Ok(response)
        }
    }
}

async fn status_handler(
    if_none_match: Option<String>,
    _state: Arc<AppState>,
) -> Result<WarpResponse, Infallible> {
    let status_body = StatusResponse::default();
    let fingerprint = serde_json::to_string(&status_body).unwrap_or_default();
    let etag_value = compute_hashed_etag("status", &fingerprint);

    if should_return_not_modified(if_none_match.as_deref(), &etag_value) {
        let mut response = WarpResponse::new(Vec::<u8>::new().into());
        *response.status_mut() = StatusCode::NOT_MODIFIED;
        insert_cache_headers(&mut response, &etag_value, None, CACHE_CONTROL_STATUS);
        return Ok(response);
    }

    let reply = warp::reply::json(&status_body);
    let mut response = reply.into_response();
    insert_cache_headers(&mut response, &etag_value, None, CACHE_CONTROL_STATUS);
    Ok(response)
}

async fn options_handler(
    if_none_match: Option<String>,
    _state: Arc<AppState>,
) -> Result<WarpResponse, Infallible> {
    let fingerprint = "languages=0|activity=0|users=0";
    let etag_value = compute_hashed_etag("options", fingerprint);
    let response_body = OptionsResponse {
        languages: Vec::new(),
        activity_tiers: Vec::new(),
        users: Vec::new(),
        meta: OptionsMeta {
            etag: etag_value.clone(),
            last_modified: None,
        },
    };

    if should_return_not_modified(if_none_match.as_deref(), &etag_value) {
        let mut response = WarpResponse::new(Vec::<u8>::new().into());
        *response.status_mut() = StatusCode::NOT_MODIFIED;
        insert_cache_headers(&mut response, &etag_value, None, CACHE_CONTROL_OPTIONS);
        return Ok(response);
    }

    let reply = warp::reply::json(&response_body);
    let mut response = reply.into_response();
    insert_cache_headers(&mut response, &etag_value, None, CACHE_CONTROL_OPTIONS);
    Ok(response)
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
    fetched_at: String,
    user_activity_tier: Option<String>,
    ingest_sequence: i64,
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
            fetched_at: row.fetched_at.to_rfc3339(),
            user_activity_tier: row.user_activity_tier,
            ingest_sequence: row.ingest_sequence,
        }
    }
}

fn compute_stars_etag(
    params: &StarQueryParams,
    newest_fetched: Option<DateTime<Utc>>,
    total: usize,
) -> String {
    let newest_fragment = newest_fetched
        .map(|ts| ts.timestamp_millis().to_string())
        .unwrap_or_else(|| "none".to_string());
    let key = format!("{}|{}|{}", params.normalized_key(), newest_fragment, total);
    compute_hashed_etag("stars", &key)
}

fn compute_hashed_etag(label: &str, payload: &str) -> String {
    let mut material = String::with_capacity(label.len() + payload.len() + 1);
    material.push_str(label);
    material.push('|');
    material.push_str(payload);
    let hash = fnv1a64(material.as_bytes());
    format!("W/\"{}-{:016x}\"", label, hash)
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut hash = OFFSET;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

fn should_return_not_modified(if_none_match: Option<&str>, etag: &str) -> bool {
    if let Some(header_value) = if_none_match {
        let trimmed = header_value.trim();
        if trimmed == "*" {
            true
        } else {
            trimmed
                .split(',')
                .map(|token| token.trim())
                .any(|candidate| candidate == etag)
        }
    } else {
        false
    }
}

fn insert_cache_headers(
    response: &mut WarpResponse,
    etag_value: &str,
    newest_fetched: Option<DateTime<Utc>>,
    cache_control: &'static str,
) {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );
    if let Ok(etag_header) = HeaderValue::from_str(etag_value) {
        response.headers_mut().insert(header::ETAG, etag_header);
    }
    if let Some(ts) = newest_fetched {
        if let Ok(last_modified) = HeaderValue::from_str(&ts.to_rfc2822()) {
            response
                .headers_mut()
                .insert(header::LAST_MODIFIED, last_modified);
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
        assert_eq!(
            resp.headers()
                .get(header::CACHE_CONTROL)
                .and_then(|v| v.to_str().ok()),
            Some(CACHE_CONTROL_STARS)
        );
        let etag = resp
            .headers()
            .get(header::ETAG)
            .and_then(|v| v.to_str().ok())
            .expect("etag present");
        assert!(etag.starts_with("W/"));
        assert!(resp.headers().get(header::LAST_MODIFIED).is_some());

        let body = resp.body();
        let payload: Value = serde_json::from_slice(body).unwrap();
        let items = payload
            .get("items")
            .and_then(|v| v.as_array())
            .expect("items array present");
        let meta = payload
            .get("meta")
            .and_then(|v| v.as_object())
            .expect("meta object present");
        assert_eq!(items.len(), 1);
        let first = items.first().unwrap();
        assert_eq!(first.get("login").unwrap(), "alice");
        assert_eq!(first.get("repo_full_name").unwrap(), "rust-lang/rust");
        assert_eq!(first.get("repo_language").unwrap(), "Rust");
        assert!(first.get("fetched_at").is_some());
        assert_eq!(first.get("user_activity_tier").unwrap(), "high");
        let topics = first.get("repo_topics").unwrap().as_array().unwrap();
        assert_eq!(topics.len(), 2);
        assert_eq!(first.get("ingest_sequence").unwrap().as_i64(), Some(1));
        assert_eq!(meta.get("page").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(
            meta.get("page_size").and_then(|v| v.as_u64()),
            Some(DEFAULT_PAGE_SIZE as u64)
        );
        assert_eq!(meta.get("total").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(meta.get("has_next").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(meta.get("has_prev").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(
            meta.get("etag")
                .and_then(|v| v.as_str())
                .expect("meta etag present"),
            etag
        );
        assert!(meta.get("last_modified").is_some());

        let resp_304 = warp::test::request()
            .path("/api/stars")
            .header("if-none-match", etag)
            .reply(&routes)
            .await;
        assert_eq!(resp_304.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(
            resp_304
                .headers()
                .get(header::CACHE_CONTROL)
                .and_then(|v| v.to_str().ok()),
            Some(CACHE_CONTROL_STARS)
        );
        assert!(resp_304.body().is_empty());
        assert_eq!(
            resp_304
                .headers()
                .get(header::ETAG)
                .and_then(|v| v.to_str().ok()),
            resp.headers()
                .get(header::ETAG)
                .and_then(|v| v.to_str().ok())
        );
    }

    #[tokio::test]
    async fn status_endpoint_returns_placeholder_payload() {
        let temp = NamedTempFile::new().unwrap();
        init(temp.path()).await.unwrap();

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
            .path("/api/status")
            .reply(&routes)
            .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get(header::CACHE_CONTROL)
                .and_then(|v| v.to_str().ok()),
            Some(CACHE_CONTROL_STATUS)
        );
        let etag = resp
            .headers()
            .get(header::ETAG)
            .and_then(|v| v.to_str().ok())
            .expect("etag present");
        let body: Value = serde_json::from_slice(resp.body()).unwrap();
        assert_eq!(body.get("is_stale").and_then(|v| v.as_bool()), Some(false));

        let resp_304 = warp::test::request()
            .path("/api/status")
            .header("if-none-match", etag)
            .reply(&routes)
            .await;
        assert_eq!(resp_304.status(), StatusCode::NOT_MODIFIED);
        assert!(resp_304.body().is_empty());
    }

    #[tokio::test]
    async fn options_endpoint_returns_placeholder_payload() {
        let temp = NamedTempFile::new().unwrap();
        init(temp.path()).await.unwrap();

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
            .path("/api/options")
            .reply(&routes)
            .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get(header::CACHE_CONTROL)
                .and_then(|v| v.to_str().ok()),
            Some(CACHE_CONTROL_OPTIONS)
        );
        let etag = resp
            .headers()
            .get(header::ETAG)
            .and_then(|v| v.to_str().ok())
            .expect("etag present");
        let body: Value = serde_json::from_slice(resp.body()).unwrap();
        let meta = body
            .get("meta")
            .and_then(|v| v.as_object())
            .expect("options meta present");
        assert_eq!(meta.get("etag").and_then(|v| v.as_str()), Some(etag));
        assert!(
            body.get("languages")
                .and_then(|v| v.as_array())
                .map(|arr| arr.is_empty())
                .unwrap_or(false)
        );

        let resp_304 = warp::test::request()
            .path("/api/options")
            .header("if-none-match", etag)
            .reply(&routes)
            .await;
        assert_eq!(resp_304.status(), StatusCode::NOT_MODIFIED);
        assert!(resp_304.body().is_empty());
    }
}
