use std::path::Path;

use anyhow::{Result, anyhow};
use chrono::{DateTime, Duration, Utc};
use rusqlite::types::Type;
use rusqlite::{Connection, Error, OptionalExtension, params};

use crate::{
    config::Config,
    github::{FollowingUser, StarEvent},
};

#[derive(Debug, Clone)]
pub struct UserRecord {
    pub user_id: i64,
    pub login: String,
    pub last_starred_at: Option<DateTime<Utc>>,
    pub last_fetched_at: Option<DateTime<Utc>>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub fetch_interval_minutes: i64,
    pub next_check_at: DateTime<Utc>,
    pub activity_tier: Option<String>,
}

pub async fn init(db_path: &Path) -> Result<()> {
    let path = db_path.to_path_buf();
    tokio::task::spawn_blocking(move || -> rusqlite::Result<()> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS users (
                user_id INTEGER PRIMARY KEY,
                login TEXT NOT NULL UNIQUE,
                last_starred_at TEXT,
                last_fetched_at TEXT,
                etag TEXT,
                last_modified TEXT,
                fetch_interval_minutes INTEGER NOT NULL,
                next_check_at TEXT NOT NULL,
                activity_tier TEXT
            );

            CREATE TABLE IF NOT EXISTS stars (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
                repo_full_name TEXT NOT NULL,
                repo_description TEXT,
                repo_language TEXT,
                repo_topics TEXT,
                repo_html_url TEXT NOT NULL,
                starred_at TEXT NOT NULL,
                fetched_at TEXT NOT NULL,
                UNIQUE(user_id, repo_full_name, starred_at)
            );

            CREATE INDEX IF NOT EXISTS idx_stars_user_starred_at ON stars(user_id, starred_at DESC);
            CREATE INDEX IF NOT EXISTS idx_stars_starred_at ON stars(starred_at DESC);
            "#,
        )?;

        ensure_column(&conn, "users", "activity_tier", "TEXT")?;
        ensure_column(&conn, "stars", "repo_language", "TEXT")?;
        ensure_column(&conn, "stars", "repo_topics", "TEXT")?;

        // Backfill activity tiers for existing records using current fetch intervals.
        conn.execute(
            "UPDATE users SET activity_tier = 'high' WHERE activity_tier IS NULL AND fetch_interval_minutes <= 60",
            [],
        )?;
        conn.execute(
            "UPDATE users SET activity_tier = 'medium' WHERE activity_tier IS NULL AND fetch_interval_minutes > 60 AND fetch_interval_minutes <= 1440",
            [],
        )?;
        conn.execute(
            "UPDATE users SET activity_tier = 'low' WHERE activity_tier IS NULL AND fetch_interval_minutes > 1440",
            [],
        )?;
        Ok(())
    })
    .await??;
    Ok(())
}

pub async fn upsert_followings(
    db_path: &Path,
    users: &[FollowingUser],
    default_interval_minutes: i64,
) -> Result<()> {
    if users.is_empty() {
        return Ok(());
    }
    let path = db_path.to_path_buf();
    let users = users.to_owned();
    tokio::task::spawn_blocking(move || -> rusqlite::Result<()> {
        let mut conn = Connection::open(path)?;
        let now = Utc::now().to_rfc3339();
        let tx = conn.transaction()?;
        for user in users {
            tx.execute(
                "INSERT INTO users (user_id, login, last_starred_at, last_fetched_at, etag, last_modified, fetch_interval_minutes, next_check_at)
                 VALUES (?1, ?2, NULL, NULL, NULL, NULL, ?3, ?4)
                 ON CONFLICT(user_id) DO UPDATE SET login = excluded.login",
                params![user.id, user.login, default_interval_minutes, now],
            )?;
        }
        tx.commit()?;
        Ok(())
    })
    .await??;
    Ok(())
}

pub async fn due_users(db_path: &Path, now: DateTime<Utc>) -> Result<Vec<UserRecord>> {
    let path = db_path.to_path_buf();
    let now_string = now.to_rfc3339();
    let users = tokio::task::spawn_blocking(move || -> rusqlite::Result<Vec<UserRecord>> {
        let conn = Connection::open(path)?;
        let mut stmt = conn.prepare(
            "SELECT user_id, login, last_starred_at, last_fetched_at, etag, last_modified, fetch_interval_minutes, next_check_at, activity_tier
             FROM users
             WHERE next_check_at <= ?1
             ORDER BY next_check_at ASC",
        )?;
        let rows = stmt.query_map([now_string], |row| {
            let next_check_at_raw: String = row.get(7)?;
            let last_starred_at_raw: Option<String> = row.get(2)?;
            let last_fetched_at_raw: Option<String> = row.get(3)?;
            let last_starred_at = parse_optional_datetime_sql(last_starred_at_raw, 2)?;
            let last_fetched_at = parse_optional_datetime_sql(last_fetched_at_raw, 3)?;
            let next_check_at = parse_datetime_sql(&next_check_at_raw, 7)?;
            Ok(UserRecord {
                user_id: row.get(0)?,
                login: row.get(1)?,
                last_starred_at,
                last_fetched_at,
                etag: row.get(4)?,
                last_modified: row.get(5)?,
                fetch_interval_minutes: row.get(6)?,
                next_check_at,
                activity_tier: row.get(8)?,
            })
        })?;
        let mut users = Vec::new();
        for record in rows {
            users.push(record?);
        }
        Ok(users)
    })
    .await??;
    Ok(users)
}

pub async fn record_not_modified(
    db_path: &Path,
    user_id: i64,
    fetched_at: DateTime<Utc>,
    interval_minutes: i64,
) -> Result<()> {
    let path = db_path.to_path_buf();
    let fetched = fetched_at.to_rfc3339();
    let next = (fetched_at + Duration::minutes(interval_minutes)).to_rfc3339();
    tokio::task::spawn_blocking(move || -> rusqlite::Result<()> {
        let conn = Connection::open(path)?;
        conn.execute(
            "UPDATE users SET last_fetched_at = ?1, next_check_at = ?2 WHERE user_id = ?3",
            params![fetched, next, user_id],
        )?;
        Ok(())
    })
    .await??;
    Ok(())
}

pub async fn defer_user(db_path: &Path, user_id: i64, wait: std::time::Duration) -> Result<()> {
    let path = db_path.to_path_buf();
    let chrono_wait =
        Duration::from_std(wait).map_err(|e| anyhow!("invalid wait duration: {e}"))?;
    tokio::task::spawn_blocking(move || -> rusqlite::Result<()> {
        let conn = Connection::open(path)?;
        let mut stmt = conn
            .prepare("SELECT COALESCE(fetch_interval_minutes, 0) FROM users WHERE user_id = ?1")?;
        let interval: Option<i64> = stmt.query_row([user_id], |row| row.get(0)).optional()?;
        let current_fetch_interval = interval.unwrap_or(0);
        let now = Utc::now();
        let next = (now + chrono_wait).to_rfc3339();
        conn.execute(
            "UPDATE users SET next_check_at = ?1, last_fetched_at = ?2 WHERE user_id = ?3",
            params![next, now.to_rfc3339(), user_id],
        )?;
        if current_fetch_interval == 0 {
            conn.execute(
                "UPDATE users SET fetch_interval_minutes = ?1 WHERE user_id = ?2",
                params![chrono_wait.num_minutes().max(1), user_id],
            )?;
        }
        Ok(())
    })
    .await??;
    Ok(())
}

pub async fn insert_star_events(
    db_path: &Path,
    user: &UserRecord,
    events: &[StarEvent],
    fetched_at: DateTime<Utc>,
    etag: Option<String>,
    last_modified: Option<String>,
    config: &Config,
) -> Result<i64> {
    if events.is_empty() {
        // Even if there are no events, update metadata to refresh next_check_at
        update_after_events(
            db_path,
            user.user_id,
            user.last_starred_at,
            fetched_at,
            etag,
            last_modified,
            config,
        )
        .await?;
        return Ok(user.fetch_interval_minutes);
    }

    let path = db_path.to_path_buf();
    let user_id = user.user_id;
    let fetched = fetched_at.to_rfc3339();
    let events_vec = events.to_owned();
    let etag_clone = etag.clone();
    let last_modified_clone = last_modified.clone();
    tokio::task::spawn_blocking(move || -> rusqlite::Result<()> {
        let mut conn = Connection::open(path)?;
        let tx = conn.transaction()?;
        for event in &events_vec {
            let topics_json = if event.repo_topics.is_empty() {
                None
            } else {
                serde_json::to_string(&event.repo_topics).ok()
            };
            tx.execute(
                "INSERT OR IGNORE INTO stars (user_id, repo_full_name, repo_description, repo_language, repo_topics, repo_html_url, starred_at, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    user_id,
                    event.repo_full_name,
                    event.repo_description,
                    event.repo_language,
                    topics_json,
                    event.repo_html_url,
                    event.starred_at.to_rfc3339(),
                    fetched
                ],
            )?;
        }
        if let Some(max_starred) = events_vec.iter().map(|e| e.starred_at).max() {
            tx.execute(
                "UPDATE users SET last_starred_at = ?1 WHERE user_id = ?2 AND (
                     last_starred_at IS NULL OR last_starred_at < ?1
                 )",
                params![max_starred.to_rfc3339(), user_id],
            )?;
        }
        if let Some(tag) = etag_clone {
            tx.execute(
                "UPDATE users SET etag = ?1 WHERE user_id = ?2",
                params![tag, user_id],
            )?;
        }
        if let Some(modified) = last_modified_clone {
            tx.execute(
                "UPDATE users SET last_modified = ?1 WHERE user_id = ?2",
                params![modified, user_id],
            )?;
        }
        tx.execute(
            "UPDATE users SET last_fetched_at = ?1 WHERE user_id = ?2",
            params![fetched, user_id],
        )?;
        tx.commit()?;
        Ok(())
    })
    .await??;

    update_after_events(
        db_path,
        user.user_id,
        None,
        fetched_at,
        etag,
        last_modified,
        config,
    )
    .await
}

async fn update_after_events(
    db_path: &Path,
    user_id: i64,
    cached_last_starred: Option<DateTime<Utc>>,
    fetched_at: DateTime<Utc>,
    etag: Option<String>,
    last_modified: Option<String>,
    config: &Config,
) -> Result<i64> {
    let activity = recompute_interval(db_path, user_id, config).await?;
    let next = (fetched_at + Duration::minutes(activity.interval_minutes)).to_rfc3339();
    let fetched = fetched_at.to_rfc3339();
    let etag_val = etag;
    let last_mod_val = last_modified;
    let path = db_path.to_path_buf();
    tokio::task::spawn_blocking(move || -> rusqlite::Result<()> {
        let conn = Connection::open(path)?;
        conn.execute(
            "UPDATE users SET next_check_at = ?1, fetch_interval_minutes = ?2, last_fetched_at = ?3,
             etag = COALESCE(?4, etag), last_modified = COALESCE(?5, last_modified), activity_tier = ?6
             WHERE user_id = ?7",
            params![
                next,
                activity.interval_minutes,
                fetched,
                etag_val,
                last_mod_val,
                activity.activity_tier,
                user_id
            ],
        )?;
        if let Some(last_starred) = cached_last_starred {
            conn.execute(
                "UPDATE users SET last_starred_at = COALESCE(last_starred_at, ?1) WHERE user_id = ?2",
                params![last_starred.to_rfc3339(), user_id],
            )?;
        }
        Ok(())
    })
    .await??;
    Ok(activity.interval_minutes)
}

#[derive(Debug, Clone)]
pub struct ActivityProfile {
    pub interval_minutes: i64,
    pub activity_tier: Option<String>,
}

pub async fn recompute_interval(
    db_path: &Path,
    user_id: i64,
    config: &Config,
) -> Result<ActivityProfile> {
    let path = db_path.to_path_buf();
    let default_interval = config.default_interval_minutes;
    let min_interval = config.min_interval_minutes;
    let max_interval = config.max_interval_minutes;
    let profile = tokio::task::spawn_blocking(move || -> rusqlite::Result<ActivityProfile> {
        let conn = Connection::open(path)?;
        let mut stmt = conn.prepare(
            "SELECT starred_at FROM stars WHERE user_id = ?1 ORDER BY starred_at DESC LIMIT 5",
        )?;
        let rows = stmt.query_map([user_id], |row| {
            let starred_at: String = row.get(0)?;
            parse_datetime_sql(&starred_at, 0)
        })?;
        let mut timestamps = Vec::new();
        for ts in rows {
            timestamps.push(ts?);
        }
        if timestamps.len() < 2 {
            return Ok(ActivityProfile {
                interval_minutes: default_interval,
                activity_tier: Some(derive_activity_tier(default_interval)),
            });
        }
        let mut diffs = Vec::new();
        for window in timestamps.windows(2) {
            let first = window[0];
            let second = window[1];
            let diff = first - second;
            diffs.push(diff);
        }
        let total_minutes: i64 = diffs.into_iter().map(|d| d.num_minutes().max(1)).sum();
        let avg_minutes = total_minutes as f64 / (timestamps.len() as f64 - 1.0);
        let mut interval = avg_minutes.round() as i64;
        if interval <= 0 {
            interval = min_interval;
        }
        let interval = interval.clamp(min_interval, max_interval);
        let tier = derive_activity_tier(interval);
        Ok(ActivityProfile {
            interval_minutes: interval,
            activity_tier: Some(tier),
        })
    })
    .await??;
    Ok(profile)
}

pub async fn recent_events_for_feed(db_path: &Path, limit: usize) -> Result<Vec<StarFeedRow>> {
    let path = db_path.to_path_buf();
    let events = tokio::task::spawn_blocking(move || -> rusqlite::Result<Vec<StarFeedRow>> {
        let conn = Connection::open(path)?;
        let mut stmt = conn.prepare(
            "SELECT u.login, s.repo_full_name, s.repo_description, s.repo_language, s.repo_topics, s.repo_html_url, s.starred_at, u.activity_tier
             FROM stars s
             INNER JOIN users u ON u.user_id = s.user_id
             ORDER BY s.starred_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |row| {
            let starred_at_str: String = row.get(6)?;
            let starred_at = parse_datetime_sql(&starred_at_str, 6)?;
            let topics_json: Option<String> = row.get(4)?;
            let topics = parse_topics(topics_json)?;
            Ok(StarFeedRow {
                login: row.get(0)?,
                repo_full_name: row.get(1)?,
                repo_description: row.get(2)?,
                repo_language: row.get(3)?,
                repo_topics: topics,
                repo_html_url: row.get(5)?,
                starred_at,
                user_activity_tier: row.get(7)?,
            })
        })?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    })
    .await??;
    Ok(events)
}

#[derive(Debug, Clone)]
pub struct StarFeedRow {
    pub login: String,
    pub repo_full_name: String,
    pub repo_description: Option<String>,
    pub repo_language: Option<String>,
    pub repo_topics: Vec<String>,
    pub repo_html_url: String,
    pub starred_at: DateTime<Utc>,
    pub user_activity_tier: Option<String>,
}

fn parse_datetime_sql(value: &str, index: usize) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| Error::FromSqlConversionFailure(index, Type::Text, Box::new(e)))
}

fn parse_optional_datetime_sql(
    value: Option<String>,
    index: usize,
) -> rusqlite::Result<Option<DateTime<Utc>>> {
    match value {
        Some(v) => Ok(Some(parse_datetime_sql(&v, index)?)),
        None => Ok(None),
    }
}

fn parse_topics(value: Option<String>) -> rusqlite::Result<Vec<String>> {
    if let Some(raw) = value {
        match serde_json::from_str::<Vec<String>>(&raw) {
            Ok(list) => Ok(list),
            Err(_) => Ok(Vec::new()),
        }
    } else {
        Ok(Vec::new())
    }
}

fn derive_activity_tier(interval_minutes: i64) -> String {
    match interval_minutes {
        n if n <= 60 => "high".to_string(),
        n if n <= 24 * 60 => "medium".to_string(),
        _ => "low".to_string(),
    }
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    column_type: &str,
) -> rusqlite::Result<()> {
    if column_exists(conn, table, column)? {
        return Ok(());
    }
    let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {column_type}");
    conn.execute(&sql, [])?;
    Ok(())
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    let sql = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}
