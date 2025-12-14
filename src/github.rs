use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, TimeZone, Utc};
use reqwest::{Client, StatusCode, Url, header};
use serde::Deserialize;
use thiserror::Error;

use crate::config::Config;

const PER_PAGE: usize = 100;
const STAR_ACCEPT_HEADER: &str =
    "application/vnd.github.star+json, application/vnd.github.mercy-preview+json";

#[derive(Debug, Clone)]
pub struct GitHubClient {
    client: Client,
    base_url: Url,
    rate_limit: Arc<RateLimitState>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RateLimitSnapshot {
    pub remaining: Option<u32>,
    pub reset_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Default)]
struct RateLimitState {
    inner: Mutex<RateLimitSnapshot>,
}

#[derive(Debug, Clone)]
pub struct FollowingUser {
    pub id: i64,
    pub login: String,
}

#[derive(Debug, Clone)]
pub struct StarEvent {
    pub repo_full_name: String,
    pub repo_description: Option<String>,
    pub repo_html_url: String,
    pub starred_at: DateTime<Utc>,
    pub repo_language: Option<String>,
    pub repo_topics: Vec<String>,
}

#[derive(Debug)]
pub enum StarFetchOutcome {
    NotModified {
        fetched_at: DateTime<Utc>,
    },
    Modified {
        fetched_at: DateTime<Utc>,
        etag: Option<String>,
        last_modified: Option<String>,
        events: Vec<StarEvent>,
    },
}

#[derive(Debug, Error)]
pub enum GitHubApiError {
    #[error("rate limited, retry after {0:?}")]
    RateLimited(Duration),
    #[error("authentication failed")]
    Auth,
    #[error("access forbidden")]
    Forbidden,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Deserialize)]
struct ApiUser {
    login: String,
    id: i64,
}

#[derive(Debug, Deserialize)]
struct ApiStarredRepo {
    starred_at: DateTime<Utc>,
    repo: ApiRepo,
}

#[derive(Debug, Deserialize)]
struct ApiRepo {
    full_name: String,
    description: Option<String>,
    html_url: String,
    language: Option<String>,
    #[serde(default)]
    topics: Vec<String>,
}

impl GitHubClient {
    pub fn new(config: &Config) -> Result<Self> {
        let mut default_headers = header::HeaderMap::new();
        default_headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_str(&config.user_agent)
                .context("invalid user agent header value")?,
        );
        default_headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/vnd.github+json"),
        );
        let bearer = format!("Bearer {}", config.github_token);
        default_headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&bearer).context("invalid token header value")?,
        );

        let client = Client::builder()
            .default_headers(default_headers)
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .context("failed to build reqwest client")?;

        Ok(Self {
            client,
            base_url: config.api_base_url.clone(),
            rate_limit: Arc::new(RateLimitState::default()),
        })
    }

    pub async fn fetch_followings(&self) -> Result<Vec<FollowingUser>, GitHubApiError> {
        let mut results = Vec::new();
        let mut page = 1usize;
        loop {
            let mut url = self
                .base_url
                .join("user/following")
                .map_err(|e| anyhow!(e))?;
            url.query_pairs_mut()
                .append_pair("per_page", &PER_PAGE.to_string())
                .append_pair("page", &page.to_string());

            let response = self.client.get(url).send().await.map_err(|e| anyhow!(e))?;
            self.rate_limit.update(response.headers());
            match response.status() {
                StatusCode::OK => {
                    let body: Vec<ApiUser> = response
                        .json()
                        .await
                        .map_err(|e| anyhow!("failed to parse followings: {e}"))?;
                    let page_len = body.len();
                    if page_len == 0 {
                        break;
                    }
                    for user in body {
                        results.push(FollowingUser {
                            id: user.id,
                            login: user.login,
                        });
                    }
                    if page_len < PER_PAGE {
                        break;
                    }
                    page += 1;
                }
                StatusCode::UNAUTHORIZED => return Err(GitHubApiError::Auth),
                StatusCode::FORBIDDEN => {
                    if let Some(wait) = parse_retry_after(&response) {
                        return Err(GitHubApiError::RateLimited(wait));
                    }
                    return Err(GitHubApiError::Forbidden);
                }
                other => {
                    let text = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "<unavailable>".to_string());
                    return Err(anyhow!("unexpected status {other}: {text}").into());
                }
            }
        }
        Ok(results)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn fetch_starred(
        &self,
        login: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
        known_latest: Option<DateTime<Utc>>,
    ) -> Result<StarFetchOutcome, GitHubApiError> {
        let mut events = Vec::new();
        let mut page = 1usize;
        let mut newest_etag: Option<String> = None;
        let mut newest_last_modified: Option<String> = None;
        let mut first_request = true;
        let mut continue_paging = true;

        while continue_paging {
            let mut url = self
                .base_url
                .join(&format!("users/{login}/starred"))
                .map_err(|e| anyhow!(e))?;
            url.query_pairs_mut()
                .append_pair("per_page", &PER_PAGE.to_string())
                .append_pair("page", &page.to_string());

            let mut request = self.client.get(url);
            request = request.header(header::ACCEPT, STAR_ACCEPT_HEADER);
            if first_request {
                if let Some(tag) = etag {
                    request = request.header(header::IF_NONE_MATCH, tag);
                }
                if let Some(modified) = last_modified {
                    request = request.header(header::IF_MODIFIED_SINCE, modified);
                }
            }

            let response = request.send().await.map_err(|e| anyhow!(e))?;
            self.rate_limit.update(response.headers());
            match response.status() {
                StatusCode::OK => {
                    let headers = response.headers().clone();
                    if first_request {
                        newest_etag = headers
                            .get(header::ETAG)
                            .and_then(|h| h.to_str().ok())
                            .map(ToOwned::to_owned);
                        newest_last_modified = headers
                            .get(header::LAST_MODIFIED)
                            .and_then(|h| h.to_str().ok())
                            .map(ToOwned::to_owned);
                    }
                    let body: Vec<ApiStarredRepo> = response
                        .json()
                        .await
                        .map_err(|e| anyhow!("failed to parse starred repos: {e}"))?;
                    if body.is_empty() {
                        break;
                    }
                    let mut page_new_events = Vec::new();
                    for item in body {
                        if let Some(latest) = known_latest
                            && item.starred_at <= latest
                        {
                            continue_paging = false;
                            break;
                        }
                        page_new_events.push(StarEvent {
                            repo_full_name: item.repo.full_name,
                            repo_description: item.repo.description,
                            repo_html_url: item.repo.html_url,
                            starred_at: item.starred_at,
                            repo_language: item.repo.language,
                            repo_topics: item.repo.topics,
                        });
                    }
                    let added_count = page_new_events.len();
                    events.extend(page_new_events);
                    if !continue_paging {
                        break;
                    }
                    if added_count < PER_PAGE {
                        break;
                    }
                    page += 1;
                }
                StatusCode::NOT_MODIFIED => {
                    let fetched_at = Utc::now();
                    return Ok(StarFetchOutcome::NotModified { fetched_at });
                }
                StatusCode::UNAUTHORIZED => return Err(GitHubApiError::Auth),
                StatusCode::FORBIDDEN => {
                    if let Some(wait) = parse_retry_after(&response) {
                        return Err(GitHubApiError::RateLimited(wait));
                    }
                    return Err(GitHubApiError::Forbidden);
                }
                other => {
                    let text = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "<unavailable>".to_string());
                    return Err(anyhow!("unexpected status {other}: {text}").into());
                }
            }
            first_request = false;
        }

        let fetched_at = Utc::now();
        Ok(StarFetchOutcome::Modified {
            fetched_at,
            etag: newest_etag,
            last_modified: newest_last_modified,
            events,
        })
    }

    pub fn rate_limit_snapshot(&self) -> RateLimitSnapshot {
        self.rate_limit.snapshot()
    }
}

impl RateLimitState {
    fn update(&self, headers: &header::HeaderMap) {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(remaining) = headers
            .get("x-ratelimit-remaining")
            .and_then(|value| value.to_str().ok())
            .and_then(|raw| raw.parse::<u32>().ok())
        {
            guard.remaining = Some(remaining);
        }
        if let Some(reset) = headers
            .get("x-ratelimit-reset")
            .and_then(|value| value.to_str().ok())
            .and_then(|raw| raw.parse::<i64>().ok())
        {
            guard.reset_at = Utc.timestamp_opt(reset, 0).single();
        }
    }

    fn snapshot(&self) -> RateLimitSnapshot {
        *self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

fn parse_retry_after(response: &reqwest::Response) -> Option<Duration> {
    response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
}
