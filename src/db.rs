use std::path::Path;

use anyhow::{Result, anyhow};
use chrono::{DateTime, Duration, Utc};
use rand::Rng;
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
    pub ema_minutes: Option<f64>,
    pub star_count: i64,
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
                activity_tier TEXT,
                ema_minutes REAL,
                star_count INTEGER NOT NULL DEFAULT 0
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
        ensure_column(&conn, "users", "ema_minutes", "REAL")?;
        ensure_column(&conn, "users", "star_count", "INTEGER")?;
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
        conn.execute(
            "UPDATE users SET star_count = 0 WHERE star_count IS NULL",
            [],
        )?;
        conn.execute(
            "UPDATE users SET star_count = (
                 SELECT COUNT(*) FROM stars WHERE stars.user_id = users.user_id
             )",
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
    initial_interval_minutes: i64,
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
                "INSERT INTO users (user_id, login, last_starred_at, last_fetched_at, etag, last_modified, fetch_interval_minutes, next_check_at, activity_tier, ema_minutes, star_count)
                 VALUES (?1, ?2, NULL, NULL, NULL, NULL, ?3, ?4, 'low', NULL, 0)
                 ON CONFLICT(user_id) DO UPDATE SET login = excluded.login",
                params![user.id, user.login, initial_interval_minutes, now],
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
            "SELECT user_id, login, last_starred_at, last_fetched_at, etag, last_modified, fetch_interval_minutes, next_check_at, activity_tier, ema_minutes, star_count
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
                ema_minutes: row.get(9)?,
                star_count: row.get(10)?,
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
    let next = next_check_with_jitter(fetched_at, interval_minutes).to_rfc3339();
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
            user,
            user.last_starred_at,
            fetched_at,
            etag,
            last_modified,
            config,
            0,
            &[],
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
    let inserted_count = tokio::task::spawn_blocking(move || -> rusqlite::Result<i64> {
        let mut conn = Connection::open(path)?;
        let tx = conn.transaction()?;
        let mut inserted = 0i64;
        for event in &events_vec {
            let topics_json = if event.repo_topics.is_empty() {
                None
            } else {
                serde_json::to_string(&event.repo_topics).ok()
            };
            inserted += tx.execute(
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
            )? as i64;
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
        Ok(inserted)
    })
    .await??;

    let mut sorted_events = events.to_vec();
    sorted_events.sort_by_key(|e| e.starred_at);
    let gaps = compute_gap_minutes(&sorted_events, user.last_starred_at);

    update_after_events(
        db_path,
        user,
        None,
        fetched_at,
        etag,
        last_modified,
        config,
        inserted_count,
        &gaps,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn update_after_events(
    db_path: &Path,
    user: &UserRecord,
    cached_last_starred: Option<DateTime<Utc>>,
    fetched_at: DateTime<Utc>,
    etag: Option<String>,
    last_modified: Option<String>,
    config: &Config,
    inserted_count: i64,
    gaps: &[i64],
) -> Result<i64> {
    let min_interval = config.min_interval_minutes;
    let max_interval = config.max_interval_minutes;
    let default_interval = config.default_interval_minutes;
    let previous_interval = user.fetch_interval_minutes;
    let previous_star_count = user.star_count;
    let previous_ema = user.ema_minutes;
    let new_star_count = previous_star_count + inserted_count;

    let activity = recompute_interval(
        db_path,
        user.user_id,
        min_interval,
        max_interval,
        default_interval,
        previous_interval,
        previous_star_count,
        previous_ema,
        new_star_count,
        gaps.to_vec(),
    )
    .await?;
    let next_check = next_check_with_jitter(fetched_at, activity.interval_minutes);
    let next = next_check.to_rfc3339();
    let fetched = fetched_at.to_rfc3339();
    let etag_val = etag;
    let last_mod_val = last_modified;
    let activity_tier = activity.activity_tier.clone();
    let ema_value = activity.ema_minutes;
    let user_id = user.user_id;
    let path = db_path.to_path_buf();
    tokio::task::spawn_blocking(move || -> rusqlite::Result<()> {
        let conn = Connection::open(path)?;
        conn.execute(
            "UPDATE users SET next_check_at = ?1, fetch_interval_minutes = ?2, last_fetched_at = ?3,
             etag = COALESCE(?4, etag), last_modified = COALESCE(?5, last_modified), activity_tier = ?6,
             ema_minutes = ?7, star_count = ?8
             WHERE user_id = ?9",
            params![
                next,
                activity.interval_minutes,
                fetched,
                etag_val,
                last_mod_val,
                activity_tier,
                ema_value,
                new_star_count,
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
    pub ema_minutes: Option<f64>,
}

#[allow(clippy::too_many_arguments)]
pub async fn recompute_interval(
    db_path: &Path,
    user_id: i64,
    min_interval: i64,
    max_interval: i64,
    default_interval: i64,
    previous_interval: i64,
    previous_star_count: i64,
    previous_ema: Option<f64>,
    new_star_count: i64,
    gaps: Vec<i64>,
) -> Result<ActivityProfile> {
    let path = db_path.to_path_buf();
    let profile = tokio::task::spawn_blocking(move || -> rusqlite::Result<ActivityProfile> {
        let mut conn = Connection::open(path)?;
        let min_clamped = min_interval.max(1);
        let max_clamped = max_interval.max(min_clamped);
        let fallback_default = default_interval.clamp(min_clamped, max_clamped);
        let fallback_zero = max_clamped;
        let mut interval_minutes = previous_interval.clamp(min_clamped, max_clamped);
        let mut ema = previous_ema;
        let alpha = 0.3f64;
        let min_f = min_clamped as f64;
        let max_f = max_clamped as f64;

        let mut star_count = previous_star_count;
        for gap in &gaps {
            star_count += 1;
            let gap_minutes = (*gap).max(1) as f64;

            if star_count < 3 {
                ema = None;
                interval_minutes = fallback_default;
                continue;
            }

            if ema.is_none() {
                let avg = compute_average_gap_minutes(&mut conn, user_id)?
                    .unwrap_or(fallback_default as f64);
                let clamped = avg.clamp(min_f, max_f);
                ema = Some(clamped);
            }

            if let Some(current) = ema {
                let mut new_ema = alpha * gap_minutes + (1.0 - alpha) * current;
                new_ema = new_ema.clamp(min_f, max_f);
                ema = Some(new_ema);
                interval_minutes = new_ema.round() as i64;
            }
        }

        star_count = new_star_count;
        if star_count == 0 {
            ema = None;
            interval_minutes = fallback_zero;
        } else if star_count < 3 {
            ema = None;
            interval_minutes = fallback_default;
        } else if gaps.is_empty() {
            if let Some(current) = ema {
                interval_minutes = current.round() as i64;
            } else if let Some(avg) = compute_average_gap_minutes(&mut conn, user_id)? {
                let clamped = avg.clamp(min_f, max_f);
                ema = Some(clamped);
                interval_minutes = clamped.round() as i64;
            } else {
                interval_minutes = fallback_default;
            }
        }

        interval_minutes = interval_minutes.clamp(min_clamped, max_clamped);
        let activity_tier = derive_activity_tier(interval_minutes);

        Ok(ActivityProfile {
            interval_minutes,
            activity_tier: Some(activity_tier),
            ema_minutes: ema,
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
            "SELECT u.login, s.repo_full_name, s.repo_description, s.repo_language, s.repo_topics, s.repo_html_url, s.starred_at, s.fetched_at, u.activity_tier, s.id
             FROM stars s
             INNER JOIN users u ON u.user_id = s.user_id
             ORDER BY s.fetched_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |row| {
            let starred_at_str: String = row.get(6)?;
            let starred_at = parse_datetime_sql(&starred_at_str, 6)?;
            let fetched_at_str: String = row.get(7)?;
            let fetched_at = parse_datetime_sql(&fetched_at_str, 7)?;
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
                fetched_at,
                user_activity_tier: row.get(8)?,
                ingest_sequence: row.get(9)?,
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
    pub fetched_at: DateTime<Utc>,
    pub user_activity_tier: Option<String>,
    pub ingest_sequence: i64,
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

fn next_check_with_jitter(base: DateTime<Utc>, interval_minutes: i64) -> DateTime<Utc> {
    if interval_minutes <= 0 {
        return base + Duration::minutes(1);
    }

    let jitter_cap = ((interval_minutes as f64) * 0.1).ceil() as i64;
    let jitter_cap = jitter_cap.clamp(1, 30);
    let mut rng = rand::thread_rng();
    let jitter = if jitter_cap > 0 {
        rng.gen_range(-jitter_cap..=jitter_cap)
    } else {
        0
    };

    let total_minutes = (interval_minutes + jitter).max(1);
    base + Duration::minutes(total_minutes)
}

fn derive_activity_tier(interval_minutes: i64) -> String {
    match interval_minutes {
        n if n <= 60 => "high".to_string(),
        n if n <= 24 * 60 => "medium".to_string(),
        _ => "low".to_string(),
    }
}

fn compute_gap_minutes(
    events: &[StarEvent],
    previous_last_starred: Option<DateTime<Utc>>,
) -> Vec<i64> {
    let mut gaps = Vec::new();
    let mut prev = previous_last_starred;
    for event in events {
        if let Some(prev_ts) = prev {
            let gap = (event.starred_at - prev_ts).num_minutes();
            if gap > 0 {
                gaps.push(gap);
            }
        }
        prev = Some(event.starred_at);
    }
    gaps
}

fn compute_average_gap_minutes(
    conn: &mut Connection,
    user_id: i64,
) -> rusqlite::Result<Option<f64>> {
    let mut stmt =
        conn.prepare("SELECT starred_at FROM stars WHERE user_id = ?1 ORDER BY starred_at ASC")?;
    let mut rows = stmt.query([user_id])?;
    let mut prev: Option<DateTime<Utc>> = None;
    let mut total = 0i64;
    let mut count = 0i64;
    while let Some(row) = rows.next()? {
        let starred_at_str: String = row.get(0)?;
        let ts = parse_datetime_sql(&starred_at_str, 0)?;
        if let Some(prev_ts) = prev {
            let gap = (ts - prev_ts).num_minutes();
            if gap > 0 {
                total += gap;
                count += 1;
            }
        }
        prev = Some(ts);
    }
    if count == 0 {
        Ok(None)
    } else {
        Ok(Some(total as f64 / count as f64))
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn ema_fallback_for_sparse_history() {
        let temp = NamedTempFile::new().unwrap();
        init(temp.path()).await.unwrap();

        let profile = recompute_interval(
            temp.path(),
            1,
            10,
            7 * 24 * 60,
            60,
            60,
            1,
            None,
            2,
            vec![30],
        )
        .await
        .unwrap();

        assert_eq!(profile.interval_minutes, 60);
        assert_eq!(profile.activity_tier.as_deref(), Some("high"));
        assert!(profile.ema_minutes.is_none());
    }

    #[tokio::test]
    async fn ema_updates_with_smoothing() {
        let temp = NamedTempFile::new().unwrap();
        init(temp.path()).await.unwrap();

        let profile = recompute_interval(
            temp.path(),
            1,
            10,
            7 * 24 * 60,
            60,
            90,
            3,
            Some(90.0),
            4,
            vec![30],
        )
        .await
        .unwrap();

        assert_eq!(profile.interval_minutes, 72);
        assert_eq!(profile.activity_tier.as_deref(), Some("medium"));
        assert_eq!(profile.ema_minutes.unwrap(), 72.0);
    }

    #[tokio::test]
    async fn ema_bootstrap_on_third_event() {
        let temp = NamedTempFile::new().unwrap();
        init(temp.path()).await.unwrap();

        let conn = Connection::open(temp.path()).unwrap();
        conn.execute(
            "INSERT INTO users (user_id, login, fetch_interval_minutes, next_check_at, activity_tier, ema_minutes, star_count)
             VALUES (?1, ?2, ?3, ?4, 'medium', NULL, 3)",
            params![1, "alice", 60, Utc::now().to_rfc3339()],
        )
        .unwrap();

        let t0 = Utc.with_ymd_and_hms(2025, 10, 20, 0, 0, 0).unwrap();
        let t1 = Utc.with_ymd_and_hms(2025, 10, 21, 0, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2025, 10, 21, 12, 0, 0).unwrap();

        for ts in [t0, t1, t2] {
            conn.execute(
                "INSERT INTO stars (user_id, repo_full_name, repo_description, repo_language, repo_topics, repo_html_url, starred_at, fetched_at)
                 VALUES (?1, ?2, NULL, NULL, NULL, ?3, ?4, ?4)",
                params![1, "example/repo", "https://example.com", ts.to_rfc3339()],
            )
            .unwrap();
        }

        drop(conn);

        let profile = recompute_interval(
            temp.path(),
            1,
            10,
            7 * 24 * 60,
            60,
            60,
            2,
            None,
            3,
            vec![(t2 - t1).num_minutes()],
        )
        .await
        .unwrap();

        assert_eq!(profile.interval_minutes, 972);
        assert_eq!(profile.activity_tier.as_deref(), Some("medium"));
        assert!((profile.ema_minutes.unwrap() - 972.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn zero_star_users_use_max_interval() {
        let temp = NamedTempFile::new().unwrap();
        init(temp.path()).await.unwrap();

        let profile = recompute_interval(
            temp.path(),
            1,
            10,
            7 * 24 * 60,
            60,
            60,
            0,
            None,
            0,
            Vec::new(),
        )
        .await
        .unwrap();

        assert_eq!(profile.interval_minutes, 7 * 24 * 60);
        assert_eq!(profile.activity_tier.as_deref(), Some("low"));
        assert!(profile.ema_minutes.is_none());
    }

    #[test]
    fn jitter_respects_bounds() {
        let base = Utc::now();
        let interval = 120;
        let jitter_cap = ((interval as f64) * 0.1).ceil() as i64;
        let jitter_cap = jitter_cap.clamp(1, 30);
        let min_delay = (interval - jitter_cap).max(1);
        let max_delay = interval + jitter_cap;

        for _ in 0..100 {
            let next = next_check_with_jitter(base, interval);
            let delta = (next - base).num_minutes();
            assert!(delta >= min_delay, "delta {} below {}", delta, min_delay);
            assert!(delta <= max_delay, "delta {} above {}", delta, max_delay);
        }
    }

    #[test]
    fn activity_tier_thresholds() {
        assert_eq!(derive_activity_tier(10), "high");
        assert_eq!(derive_activity_tier(60), "high");
        assert_eq!(derive_activity_tier(61), "medium");
        assert_eq!(derive_activity_tier(1440), "medium");
        assert_eq!(derive_activity_tier(1441), "low");
    }
}
