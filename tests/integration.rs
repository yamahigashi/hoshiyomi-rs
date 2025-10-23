use std::path::PathBuf;

use chrono::{TimeZone, Utc};
use httpmock::prelude::*;
use rusqlite::Connection;
use std::sync::Arc;
use url::Url;
use warp::http::StatusCode;

use starchaser::config::{Config, Mode};
use starchaser::db::{self, StarFeedRow};
use starchaser::feed;
use starchaser::github::{GitHubApiError, GitHubClient};
use starchaser::server::{self, AppState};

#[tokio::test]
async fn github_client_returns_rate_limited_error() {
    let server = MockServer::start_async().await;

    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/users/alice/starred")
                .query_param("per_page", "100")
                .query_param("page", "1");
            then.status(403).header("Retry-After", "60");
        })
        .await;

    let config = Config {
        github_token: "test-token".into(),
        db_path: PathBuf::from("/tmp/ignored.db"),
        max_concurrency: 1,
        feed_length: 10,
        default_interval_minutes: 60,
        min_interval_minutes: 10,
        max_interval_minutes: 7 * 24 * 60,
        api_base_url: Url::parse(&server.base_url()).unwrap(),
        user_agent: "following-stars-rss-test".into(),
        timeout_secs: 5,
        mode: Mode::Once,
    };

    let client = GitHubClient::new(&config).unwrap();
    let err = client
        .fetch_starred("alice", None, None, None)
        .await
        .expect_err("expected rate limit error");

    match err {
        GitHubApiError::RateLimited(wait) => assert_eq!(wait.as_secs(), 60),
        other => panic!("expected rate limited error, got {other:?}"),
    }
}

#[test]
fn feed_builder_includes_expected_fields() {
    let events = vec![StarFeedRow {
        login: "alice".into(),
        repo_full_name: "rust-lang/rust".into(),
        repo_description: Some("Rust programming language".into()),
        repo_language: Some("Rust".into()),
        repo_topics: vec!["compiler".into()],
        repo_html_url: "https://github.com/rust-lang/rust".into(),
        starred_at: Utc.with_ymd_and_hms(2025, 10, 18, 4, 15, 0).unwrap(),
        user_activity_tier: Some("high".into()),
    }];

    let xml = feed::build_feed(
        &events,
        Utc.with_ymd_and_hms(2025, 10, 18, 5, 0, 0).unwrap(),
    )
    .expect("feed build");

    assert!(xml.contains("GitHub Followings Stars"));
    assert!(xml.contains("alice starred rust-lang/rust"));
    assert!(xml.contains("github-star://alice/rust-lang/rust"));
    assert!(xml.contains("Rust programming language"));

    let html = feed::build_html(
        &events,
        Utc.with_ymd_and_hms(2025, 10, 18, 5, 0, 0).unwrap(),
    );
    assert!(html.contains("GitHub Followings Stars"));
    assert!(html.contains("id=\"search-input\""));
    assert!(html.contains("id=\"language-filter\""));
    assert!(html.contains("Sort: Newest"));
    assert!(html.contains("Last updated"));
}

#[tokio::test]
async fn server_routes_serve_feed_and_html() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    db::init(temp.path()).await.unwrap();

    // Seed sample data
    let conn = Connection::open(temp.path()).unwrap();
    conn.execute(
        "INSERT INTO users (user_id, login, last_starred_at, last_fetched_at, etag, last_modified, fetch_interval_minutes, next_check_at)
         VALUES (1, 'alice', '2025-10-18T04:15:00Z', '2025-10-18T04:16:00Z', NULL, NULL, 60, '2025-10-18T05:16:00Z')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO stars (user_id, repo_full_name, repo_description, repo_html_url, starred_at, fetched_at)
         VALUES (1, 'rust-lang/rust', 'Rust programming language', 'https://github.com/rust-lang/rust', '2025-10-18T04:15:00Z', '2025-10-18T04:16:00Z')",
        [],
    )
    .unwrap();

    let config = Arc::new(Config {
        github_token: "token".into(),
        db_path: temp.path().to_path_buf(),
        max_concurrency: 1,
        feed_length: 10,
        default_interval_minutes: 60,
        min_interval_minutes: 10,
        max_interval_minutes: 7 * 24 * 60,
        api_base_url: Url::parse("https://api.github.com").unwrap(),
        user_agent: "following-stars-rss-test".into(),
        timeout_secs: 5,
        mode: Mode::Once,
    });

    let state = Arc::new(AppState::new(config));
    let routes = server::routes(state);

    let feed_resp = warp::test::request().path("/feed.xml").reply(&routes).await;
    assert_eq!(feed_resp.status(), StatusCode::OK);
    assert_eq!(
        feed_resp.headers().get("content-type").unwrap(),
        "application/rss+xml"
    );
    let feed_body = String::from_utf8(feed_resp.body().to_vec()).unwrap();
    assert!(feed_body.contains("rust-lang/rust"));

    let html_resp = warp::test::request().path("/").reply(&routes).await;
    assert_eq!(html_resp.status(), StatusCode::OK);
    assert_eq!(
        html_resp.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );
    let html_body = String::from_utf8(html_resp.body().to_vec()).unwrap();
    assert!(html_body.contains("GitHub Followings Stars"));
    assert!(html_body.contains("id=\"search-input\""));
    assert!(html_body.contains("id=\"activity-filter\""));
    assert!(html_body.contains("Sort: Newest"));
}
