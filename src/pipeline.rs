use std::sync::Arc;

use anyhow::{Result, anyhow};
use chrono::Utc;
use futures::StreamExt;
use tokio::sync::Semaphore;

use crate::config::Config;
use crate::db::{
    UserRecord, defer_user, due_users, insert_star_events, recent_events_for_feed,
    record_not_modified, upsert_followings,
};
use crate::feed;
use crate::github::{self, GitHubApiError, GitHubClient, StarFetchOutcome};

pub async fn poll_once(config: &Config, client: Arc<GitHubClient>) -> Result<()> {
    let followings = fetch_followings_with_retry(client.clone()).await?;
    upsert_followings(&config.db_path, &followings, config.max_interval_minutes).await?;

    let due = due_users(&config.db_path, Utc::now()).await?;
    if due.is_empty() {
        return Ok(());
    }

    let semaphore = Arc::new(Semaphore::new(config.max_concurrency));
    let mut handles = futures::stream::FuturesUnordered::new();
    for user in due {
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");
        let client_clone = client.clone();
        let config_clone = config.clone();
        let db_path = config.db_path.clone();
        handles.push(tokio::spawn(async move {
            let result = process_user(client_clone, &config_clone, &db_path, user).await;
            drop(permit);
            result
        }));
    }

    while let Some(result) = handles.next().await {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(err),
            Err(join_err) => return Err(join_err.into()),
        }
    }

    Ok(())
}

pub async fn build_feed_xml(config: &Config) -> Result<String> {
    let events = recent_events_for_feed(&config.db_path, config.feed_length).await?;
    let xml = feed::build_feed(&events, Utc::now())?;
    Ok(xml)
}

pub async fn fetch_followings_with_retry(
    client: Arc<GitHubClient>,
) -> Result<Vec<github::FollowingUser>> {
    loop {
        match client.fetch_followings().await {
            Ok(users) => return Ok(users),
            Err(GitHubApiError::RateLimited(wait)) => {
                eprintln!(
                    "Rate limited while fetching followings, sleeping {} seconds",
                    wait.as_secs()
                );
                tokio::time::sleep(wait).await;
            }
            Err(GitHubApiError::Auth) => {
                return Err(anyhow!("GitHub authentication failed. Check your token."));
            }
            Err(GitHubApiError::Forbidden) => {
                return Err(anyhow!("GitHub API access forbidden."));
            }
            Err(GitHubApiError::Other(err)) => return Err(err),
        }
    }
}

pub async fn process_user(
    client: Arc<GitHubClient>,
    config: &Config,
    db_path: &std::path::Path,
    user: UserRecord,
) -> Result<()> {
    let known_latest = user.last_starred_at;
    let outcome = client
        .fetch_starred(
            &user.login,
            user.etag.as_deref(),
            user.last_modified.as_deref(),
            known_latest,
        )
        .await;

    match outcome {
        Ok(StarFetchOutcome::NotModified { fetched_at }) => {
            record_not_modified(
                db_path,
                user.user_id,
                fetched_at,
                user.fetch_interval_minutes,
            )
            .await?;
        }
        Ok(StarFetchOutcome::Modified {
            fetched_at,
            etag,
            last_modified,
            events,
        }) => {
            let new_interval = insert_star_events(
                db_path,
                &user,
                &events,
                fetched_at,
                etag,
                last_modified,
                config,
            )
            .await?;
            println!(
                "{} new events for {} (next fetch in {} minutes)",
                events.len(),
                user.login,
                new_interval
            );
        }
        Err(GitHubApiError::RateLimited(wait)) => {
            eprintln!(
                "Rate limited while fetching stars for {}. Pausing {} seconds.",
                user.login,
                wait.as_secs()
            );
            defer_user(db_path, user.user_id, wait).await?;
            tokio::time::sleep(wait).await;
        }
        Err(GitHubApiError::Auth) => {
            return Err(anyhow!(
                "GitHub authentication failed while fetching stars for {}",
                user.login
            ));
        }
        Err(GitHubApiError::Forbidden) => {
            return Err(anyhow!(
                "GitHub API access forbidden for user {}",
                user.login
            ));
        }
        Err(GitHubApiError::Other(err)) => return Err(err),
    }
    Ok(())
}
