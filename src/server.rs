use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::{Notify, RwLock};
use warp::http::{HeaderValue, StatusCode, header};
use warp::reply::Response as WarpResponse;
use warp::{Filter, Reply};

use crate::config::Mode;
use crate::db::init;
use crate::db::star_query::{
    self, NextCheckSummary, OptionsSnapshot, StarQuery, StarQueryResult, StarSort,
    UserFilterMode as DbUserFilterMode,
};
use crate::github::{GitHubClient, RateLimitSnapshot};
use crate::pipeline::{build_feed_xml, poll_once};
use crate::{Config, feed};

const DEFAULT_PAGE_SIZE: u32 = 25;
const MAX_PAGE_SIZE: u32 = 100;
const CACHE_CONTROL_STARS: &str = "private, max-age=0";
const CACHE_CONTROL_STATUS: &str = "private, max-age=30, stale-while-revalidate=30";
const CACHE_CONTROL_OPTIONS: &str = "public, max-age=300";

#[derive(Debug, Clone, Default)]
pub(crate) struct SchedulerSnapshot {
    last_poll_started: Option<DateTime<Utc>>,
    last_poll_finished: Option<DateTime<Utc>>,
    last_error: Option<String>,
}

#[derive(Clone)]
pub struct SchedulerState {
    refresh_interval: ChronoDuration,
    inner: Arc<RwLock<SchedulerSnapshot>>,
}

impl SchedulerState {
    pub fn new(refresh_minutes: u64) -> Self {
        let minutes = refresh_minutes.max(1);
        Self {
            refresh_interval: ChronoDuration::minutes(minutes as i64),
            inner: Arc::new(RwLock::new(SchedulerSnapshot::default())),
        }
    }

    pub async fn record_start(&self, at: DateTime<Utc>) {
        let mut guard = self.inner.write().await;
        guard.last_poll_started = Some(at);
    }

    pub async fn record_finish(&self, finished: DateTime<Utc>, error: Option<String>) {
        let mut guard = self.inner.write().await;
        guard.last_poll_finished = Some(finished);
        guard.last_error = error;
    }

    pub(crate) async fn snapshot(&self) -> SchedulerSnapshot {
        self.inner.read().await.clone()
    }

    pub(crate) fn is_stale(&self, now: DateTime<Utc>, snapshot: &SchedulerSnapshot) -> bool {
        match snapshot.last_poll_finished {
            Some(finished) => now - finished > self.refresh_interval * 2,
            None => false,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    config: Arc<Config>,
    scheduler: Arc<SchedulerState>,
    github_client: Option<Arc<GitHubClient>>,
}

impl AppState {
    pub fn new(
        config: Arc<Config>,
        scheduler: Arc<SchedulerState>,
        github_client: Option<Arc<GitHubClient>>,
    ) -> Self {
        Self {
            config,
            scheduler,
            github_client,
        }
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

    pub async fn star_list(&self, query: &StarQuery) -> Result<StarQueryResult> {
        star_query::query_stars(&self.config.db_path, query).await
    }

    pub async fn options_snapshot(&self) -> Result<OptionsSnapshot> {
        star_query::options_snapshot(&self.config.db_path).await
    }

    pub async fn next_check_summary(&self) -> Result<NextCheckSummary> {
        star_query::next_check_summary(&self.config.db_path).await
    }

    pub fn config(&self) -> &Config {
        self.config.as_ref()
    }

    pub fn scheduler(&self) -> Arc<SchedulerState> {
        Arc::clone(&self.scheduler)
    }

    pub fn rate_limit_snapshot(&self) -> Option<RateLimitSnapshot> {
        self.github_client
            .as_ref()
            .map(|client| client.rate_limit_snapshot())
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SortOrder {
    #[default]
    Newest,
    Alpha,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum UserMode {
    #[default]
    All,
    Pin,
    Exclude,
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

    fn to_star_query(&self) -> StarQuery {
        StarQuery {
            search: self
                .q
                .as_ref()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            language: self
                .language
                .as_ref()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            activity: self
                .activity
                .as_ref()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            user: self
                .user
                .as_ref()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            user_mode: match self.user_mode {
                UserMode::All => DbUserFilterMode::All,
                UserMode::Pin => DbUserFilterMode::Pin,
                UserMode::Exclude => DbUserFilterMode::Exclude,
            },
            sort: match self.sort {
                SortOrder::Newest => StarSort::Newest,
                SortOrder::Alpha => StarSort::Alpha,
            },
            page: self.page() as usize,
            page_size: self.page_size() as usize,
        }
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

impl From<NextCheckSummary> for NextCheckAt {
    fn from(summary: NextCheckSummary) -> Self {
        Self {
            high: summary.high.map(|dt| dt.to_rfc3339()),
            medium: summary.medium.map(|dt| dt.to_rfc3339()),
            low: summary.low.map(|dt| dt.to_rfc3339()),
            unknown: summary.unknown.map(|dt| dt.to_rfc3339()),
        }
    }
}

#[derive(Debug, Default, Serialize)]
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
    let scheduler = Arc::new(SchedulerState::new(serve_options.refresh_minutes));

    scheduler.record_start(Utc::now()).await;
    match poll_once(config.as_ref(), client.clone()).await {
        Ok(_) => scheduler.record_finish(Utc::now(), None).await,
        Err(err) => {
            scheduler
                .record_finish(Utc::now(), Some(err.to_string()))
                .await;
            return Err(err);
        }
    }

    let state = Arc::new(AppState::new(
        Arc::clone(&config),
        Arc::clone(&scheduler),
        Some(client.clone()),
    ));

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
    let poller_scheduler = Arc::clone(&scheduler);

    let poller = tokio::spawn(async move {
        let mut interval = tokio::time::interval(refresh_interval);
        interval.tick().await; // consume the immediate tick
        loop {
            tokio::select! {
                _ = poller_notify.notified() => break,
                _ = interval.tick() => {
                    poller_scheduler.record_start(Utc::now()).await;
                    if let Err(err) = poll_once(poller_config.as_ref(), poller_client.clone()).await {
                        eprintln!("Polling error: {err:?}");
                        poller_scheduler.record_finish(Utc::now(), Some(err.to_string())).await;
                    } else {
                        poller_scheduler.record_finish(Utc::now(), None).await;
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
    let query = params.to_star_query();
    match state.star_list(&query).await {
        Ok(result) => {
            let newest_fetched = result.newest_fetched_at;
            let total = result.total;
            let etag_value = compute_stars_etag(&query.normalized_key(), newest_fetched, total);

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

            let has_next = query.page() * query.page_size() < total;
            let has_prev = query.page() > 1 && total > 0;
            let last_modified = newest_fetched.map(|ts| ts.to_rfc2822());
            let items = result
                .items
                .into_iter()
                .map(StarEventResponse::from)
                .collect::<Vec<_>>();

            let response_body = StarListResponse {
                items,
                meta: StarListMeta {
                    page: query.page() as u32,
                    page_size: query.page_size() as u32,
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
    state: Arc<AppState>,
) -> Result<WarpResponse, Infallible> {
    let snapshot = state.scheduler().snapshot().await;
    let next_check = match state.next_check_summary().await {
        Ok(summary) => summary,
        Err(err) => {
            eprintln!("Failed to load next check summary: {err:?}");
            NextCheckSummary::default()
        }
    };
    let rate_limit = state.rate_limit_snapshot().unwrap_or_default();
    let now = Utc::now();
    let is_stale = state.scheduler().is_stale(now, &snapshot);

    let status_body = StatusResponse {
        last_poll_started: snapshot.last_poll_started.map(|dt| dt.to_rfc3339()),
        last_poll_finished: snapshot.last_poll_finished.map(|dt| dt.to_rfc3339()),
        is_stale,
        next_check_at: NextCheckAt::from(next_check),
        last_error: snapshot.last_error,
        rate_limit_remaining: rate_limit.remaining,
        rate_limit_reset: rate_limit.reset_at.map(|dt| dt.to_rfc3339()),
    };
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
    state: Arc<AppState>,
) -> Result<WarpResponse, Infallible> {
    let snapshot = match state.options_snapshot().await {
        Ok(snapshot) => snapshot,
        Err(err) => {
            eprintln!("Failed to load options snapshot: {err:?}");
            OptionsSnapshot {
                languages: Vec::new(),
                activity: Vec::new(),
                users: Vec::new(),
                updated_at: None,
            }
        }
    };
    let fingerprint = snapshot.fingerprint();
    let etag_value = compute_hashed_etag("options", &fingerprint);
    let response_body = OptionsResponse {
        languages: snapshot
            .languages
            .into_iter()
            .map(|lang| LanguageOption {
                name: lang.name,
                count: lang.count,
            })
            .collect(),
        activity_tiers: snapshot
            .activity
            .into_iter()
            .map(|tier| ActivityTierOption {
                tier: tier.tier,
                count: tier.count,
            })
            .collect(),
        users: snapshot
            .users
            .into_iter()
            .map(|user| UserOption {
                login: user.login,
                display_name: user.display_name,
                count: user.count,
            })
            .collect(),
        meta: OptionsMeta {
            etag: etag_value.clone(),
            last_modified: snapshot.updated_at.map(|dt| dt.to_rfc2822()),
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
    fingerprint: &str,
    newest_fetched: Option<DateTime<Utc>>,
    total: usize,
) -> String {
    let newest_fragment = newest_fetched
        .map(|ts| ts.timestamp_millis().to_string())
        .unwrap_or_else(|| "none".to_string());
    let key = format!("{}|{}|{}", fingerprint, newest_fragment, total);
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
    if let Some(ts) = newest_fetched
        && let Ok(last_modified) = HeaderValue::from_str(&ts.to_rfc2822())
    {
        response
            .headers_mut()
            .insert(header::LAST_MODIFIED, last_modified);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration as ChronoDuration, Utc};
    use rusqlite::{Connection, params};
    use serde_json::Value;
    use std::path::Path;
    use tempfile::NamedTempFile;
    use url::Url;

    #[tokio::test]
    async fn feed_handler_returns_xml() {
        let temp = NamedTempFile::new().unwrap();
        init(temp.path()).await.unwrap();
        let (state, _) = build_state(temp.path(), 10);
        let routes = routes(state);
        let resp = warp::test::request().path("/feed.xml").reply(&routes).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn stars_endpoint_paginates_and_filters() {
        let temp = NamedTempFile::new().unwrap();
        init(temp.path()).await.unwrap();
        seed_user_with_star(temp.path(), 1, "alice", "rust-lang/rust", "Rust", "high").unwrap();
        seed_user_with_star(temp.path(), 1, "alice", "rust-lang/cargo", "Rust", "high").unwrap();
        seed_user_with_star(temp.path(), 2, "bob", "golang/go", "Go", "medium").unwrap();

        let (state, _) = build_state(temp.path(), 10);
        let routes = routes(state);
        let resp = warp::test::request()
            .path("/api/stars?language=Rust&user_mode=pin&user=alice&page_size=1")
            .reply(&routes)
            .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value = serde_json::from_slice(resp.body()).unwrap();
        let meta = body.get("meta").unwrap();
        assert_eq!(meta.get("total").and_then(|v| v.as_u64()), Some(2));
        assert_eq!(meta.get("has_next").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(meta.get("page_size").and_then(|v| v.as_u64()), Some(1));
        let etag = resp.headers().get(header::ETAG).unwrap().to_str().unwrap();
        let resp_304 = warp::test::request()
            .path("/api/stars?language=Rust&user_mode=pin&user=alice&page_size=1")
            .header("if-none-match", etag)
            .reply(&routes)
            .await;
        assert_eq!(resp_304.status(), StatusCode::NOT_MODIFIED);

        let empty_resp = warp::test::request()
            .path("/api/stars?language=Elixir")
            .reply(&routes)
            .await;
        let empty_body: Value = serde_json::from_slice(empty_resp.body()).unwrap();
        assert_eq!(
            empty_body
                .get("meta")
                .and_then(|m| m.get("total"))
                .and_then(|v| v.as_u64()),
            Some(0)
        );
    }

    #[tokio::test]
    async fn status_endpoint_reports_scheduler_and_next_checks() {
        let temp = NamedTempFile::new().unwrap();
        init(temp.path()).await.unwrap();
        let now = Utc::now();
        let conn = Connection::open(temp.path()).unwrap();
        conn.execute(
            "INSERT INTO users (user_id, login, last_starred_at, last_fetched_at, fetch_interval_minutes, next_check_at, activity_tier)
             VALUES (?1, ?2, ?3, ?3, 30, ?4, 'high')",
            params![1, "alice", now.to_rfc3339(), (now + ChronoDuration::minutes(30)).to_rfc3339()],
        )
        .unwrap();

        let (state, scheduler) = build_state(temp.path(), 10);
        let routes = routes(state);
        let stale_time = Utc::now() - ChronoDuration::minutes(120);
        scheduler.record_start(stale_time).await;
        scheduler
            .record_finish(stale_time, Some("network error".into()))
            .await;

        let resp = warp::test::request()
            .path("/api/status")
            .reply(&routes)
            .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body: Value = serde_json::from_slice(resp.body()).unwrap();
        assert_eq!(body.get("is_stale").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            body.get("last_error").and_then(|v| v.as_str()),
            Some("network error")
        );
        assert!(
            body.get("next_check_at")
                .and_then(|v| v.get("high"))
                .is_some()
        );
    }

    #[tokio::test]
    async fn options_endpoint_returns_counts_and_cache_headers() {
        let temp = NamedTempFile::new().unwrap();
        init(temp.path()).await.unwrap();
        seed_user_with_star(temp.path(), 1, "alice", "rust-lang/rust", "Rust", "high").unwrap();
        seed_user_with_star(temp.path(), 2, "bob", "golang/go", "Go", "medium").unwrap();

        let (state, _) = build_state(temp.path(), 10);
        let routes = routes(state);
        let resp = warp::test::request()
            .path("/api/options")
            .reply(&routes)
            .await;
        assert_eq!(resp.status(), StatusCode::OK);
        let etag = resp.headers().get(header::ETAG).unwrap().to_str().unwrap();
        let body: Value = serde_json::from_slice(resp.body()).unwrap();
        let languages = body.get("languages").unwrap().as_array().unwrap();
        assert_eq!(languages.len(), 2);
        let resp_304 = warp::test::request()
            .path("/api/options")
            .header("if-none-match", etag)
            .reply(&routes)
            .await;
        assert_eq!(resp_304.status(), StatusCode::NOT_MODIFIED);
    }

    fn build_state(db_path: &Path, feed_length: usize) -> (Arc<AppState>, Arc<SchedulerState>) {
        let config = Arc::new(test_config(db_path, feed_length));
        let scheduler = Arc::new(SchedulerState::new(15));
        let state = Arc::new(AppState::new(
            Arc::clone(&config),
            Arc::clone(&scheduler),
            None,
        ));
        (state, scheduler)
    }

    fn test_config(db_path: &Path, feed_length: usize) -> Config {
        Config {
            github_token: "token".into(),
            db_path: db_path.to_path_buf(),
            max_concurrency: 1,
            feed_length,
            default_interval_minutes: 60,
            min_interval_minutes: 10,
            max_interval_minutes: 60 * 24,
            api_base_url: Url::parse("https://example.com").unwrap(),
            user_agent: "ua".into(),
            timeout_secs: 10,
            mode: Mode::Once,
        }
    }

    fn seed_user_with_star(
        db_path: &Path,
        user_id: i64,
        login: &str,
        repo_full_name: &str,
        language: &str,
        tier: &str,
    ) -> rusqlite::Result<()> {
        let now = Utc::now();
        let conn = Connection::open(db_path)?;
        conn.execute(
            "INSERT OR IGNORE INTO users (user_id, login, last_starred_at, last_fetched_at, fetch_interval_minutes, next_check_at, activity_tier)
             VALUES (?1, ?2, ?3, ?3, 30, ?3, ?4)",
            params![user_id, login, now.to_rfc3339(), tier],
        )?;
        conn.execute(
            "INSERT INTO stars (user_id, repo_full_name, repo_description, repo_language, repo_topics, repo_html_url, starred_at, fetched_at)
             VALUES (?1, ?2, NULL, ?3, NULL, 'https://example.com/repo', ?4, ?4)",
            params![user_id, repo_full_name, language, now.to_rfc3339()],
        )?;
        Ok(())
    }
}
