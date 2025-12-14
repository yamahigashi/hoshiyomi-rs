use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, params_from_iter};

use super::{StarFeedRow, parse_datetime_sql, parse_topics};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StarSort {
    Newest,
    Alpha,
}

impl StarSort {
    pub fn as_str(self) -> &'static str {
        match self {
            StarSort::Newest => "newest",
            StarSort::Alpha => "alpha",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserFilterMode {
    All,
    Pin,
    Exclude,
}

impl UserFilterMode {
    pub fn as_str(self) -> &'static str {
        match self {
            UserFilterMode::All => "all",
            UserFilterMode::Pin => "pin",
            UserFilterMode::Exclude => "exclude",
        }
    }
}

#[derive(Debug, Clone)]
pub struct StarQuery {
    pub search: Option<String>,
    pub language: Option<String>,
    pub activity: Option<String>,
    pub user: Option<String>,
    pub user_mode: UserFilterMode,
    pub sort: StarSort,
    pub page: usize,
    pub page_size: usize,
}

impl Default for StarQuery {
    fn default() -> Self {
        Self {
            search: None,
            language: None,
            activity: None,
            user: None,
            user_mode: UserFilterMode::All,
            sort: StarSort::Newest,
            page: 1,
            page_size: 25,
        }
    }
}

impl StarQuery {
    pub fn normalized_key(&self) -> String {
        let mut parts = BTreeMap::new();
        if let Some(value) = self
            .search
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
        {
            parts.insert("q", value.to_string());
        }
        if let Some(value) = self
            .language
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
        {
            parts.insert("language", value.to_string());
        }
        if let Some(value) = self
            .activity
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
        {
            parts.insert("activity", value.to_string());
        }
        if let Some(value) = self
            .user
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
        {
            parts.insert("user", value.to_string());
        }
        parts.insert("user_mode", self.user_mode.as_str().to_string());
        parts.insert("sort", self.sort.as_str().to_string());
        parts.insert("page", self.page().to_string());
        parts.insert("page_size", self.page_size().to_string());
        parts
            .into_iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&")
    }

    pub fn page(&self) -> usize {
        self.page.max(1)
    }

    pub fn page_size(&self) -> usize {
        self.page_size.max(1)
    }
}

#[derive(Debug, Clone)]
pub struct StarQueryResult {
    pub items: Vec<StarFeedRow>,
    pub total: usize,
    pub newest_fetched_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct OptionsSnapshot {
    pub languages: Vec<LanguageStat>,
    pub activity: Vec<ActivityTierStat>,
    pub users: Vec<UserStat>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl OptionsSnapshot {
    pub fn fingerprint(&self) -> String {
        let mut parts = Vec::new();
        for lang in &self.languages {
            parts.push(format!("lang:{}={}", lang.name, lang.count));
        }
        for tier in &self.activity {
            parts.push(format!("activity:{}={}", tier.tier, tier.count));
        }
        for user in &self.users {
            parts.push(format!("user:{}={}", user.login, user.count));
        }
        if let Some(updated) = self.updated_at {
            parts.push(format!("updated={}", updated.to_rfc3339()));
        }
        parts.join("|")
    }
}

#[derive(Debug, Clone)]
pub struct LanguageStat {
    pub name: String,
    pub count: u32,
}

#[derive(Debug, Clone)]
pub struct ActivityTierStat {
    pub tier: String,
    pub count: u32,
}

#[derive(Debug, Clone)]
pub struct UserStat {
    pub login: String,
    pub display_name: String,
    pub count: u32,
}

#[derive(Debug, Clone, Default)]
pub struct NextCheckSummary {
    pub high: Option<DateTime<Utc>>,
    pub medium: Option<DateTime<Utc>>,
    pub low: Option<DateTime<Utc>>,
    pub unknown: Option<DateTime<Utc>>,
}

pub async fn query_stars(db_path: &Path, query: &StarQuery) -> Result<StarQueryResult> {
    let path = db_path.to_path_buf();
    let query = query.clone();
    let result = tokio::task::spawn_blocking(move || -> rusqlite::Result<StarQueryResult> {
        let conn = Connection::open(path)?;
        let builder = QueryBuilder::new(&query);

        let total = builder.count(&conn)?;
        let newest_fetched_at = builder.max_fetched(&conn)?;
        let rows = builder.fetch_rows(&conn)?;

        Ok(StarQueryResult {
            items: rows,
            total,
            newest_fetched_at,
        })
    })
    .await??;
    Ok(result)
}

pub async fn options_snapshot(db_path: &Path) -> Result<OptionsSnapshot> {
    let path = db_path.to_path_buf();
    let snapshot = tokio::task::spawn_blocking(move || -> rusqlite::Result<OptionsSnapshot> {
        let conn = Connection::open(path)?;

        let mut languages_stmt = conn.prepare(
            "SELECT repo_language, COUNT(*) as count
             FROM stars
             WHERE repo_language IS NOT NULL AND repo_language != ''
             GROUP BY repo_language
             ORDER BY count DESC, repo_language ASC",
        )?;
        let languages = languages_stmt
            .query_map([], |row| {
                Ok(LanguageStat {
                    name: row.get::<_, String>(0)?,
                    count: row.get::<_, i64>(1)? as u32,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut activity_stmt = conn.prepare(
            "SELECT COALESCE(activity_tier, 'unknown') as tier, COUNT(*) as count
             FROM users
             GROUP BY tier
             ORDER BY count DESC, tier ASC",
        )?;
        let activity = activity_stmt
            .query_map([], |row| {
                Ok(ActivityTierStat {
                    tier: row.get::<_, String>(0)?,
                    count: row.get::<_, i64>(1)? as u32,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let mut users_stmt = conn.prepare(
            "SELECT u.login, COUNT(*) as count
             FROM stars s
             INNER JOIN users u ON u.user_id = s.user_id
             GROUP BY u.user_id, u.login
             ORDER BY count DESC, u.login ASC",
        )?;
        let users = users_stmt
            .query_map([], |row| {
                let login: String = row.get(0)?;
                Ok(UserStat {
                    display_name: login.clone(),
                    login,
                    count: row.get::<_, i64>(1)? as u32,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let newest_fetched = conn
            .query_row("SELECT MAX(fetched_at) FROM stars", [], |row| {
                row.get::<_, Option<String>>(0)
            })
            .optional()?
            .flatten()
            .map(|ts| parse_datetime_sql(&ts, 0))
            .transpose()?;

        Ok(OptionsSnapshot {
            languages,
            activity,
            users,
            updated_at: newest_fetched,
        })
    })
    .await??;
    Ok(snapshot)
}

pub async fn next_check_summary(db_path: &Path) -> Result<NextCheckSummary> {
    let path = db_path.to_path_buf();
    let summary = tokio::task::spawn_blocking(move || -> rusqlite::Result<NextCheckSummary> {
        let conn = Connection::open(path)?;
        let mut stmt = conn.prepare(
            "SELECT COALESCE(activity_tier, 'unknown') as tier, MIN(next_check_at)
             FROM users
             WHERE next_check_at IS NOT NULL
             GROUP BY tier",
        )?;
        let mut next = NextCheckSummary::default();
        let rows = stmt.query_map([], |row| {
            let tier: String = row.get(0)?;
            let ts: Option<String> = row.get(1)?;
            Ok((tier, ts))
        })?;
        for row in rows {
            let (tier, ts) = row?;
            let parsed = ts.map(|value| parse_datetime_sql(&value, 1)).transpose()?;
            match tier.as_str() {
                "high" => next.high = parsed,
                "medium" => next.medium = parsed,
                "low" => next.low = parsed,
                _ => next.unknown = parsed,
            }
        }
        Ok(next)
    })
    .await??;
    Ok(summary)
}

struct QueryBuilder {
    base_where: String,
    bindings: Vec<Value>,
    query: StarQuery,
}

impl QueryBuilder {
    fn new(query: &StarQuery) -> Self {
        let sanitized = StarQuery {
            page: query.page(),
            page_size: query.page_size(),
            ..query.clone()
        };
        let mut clauses = Vec::new();
        let mut bindings = Vec::new();

        if let Some(search) = sanitized
            .search
            .as_ref()
            .map(|v| v.trim().to_lowercase())
            .filter(|v| !v.is_empty())
        {
            let pattern = format!("%{search}%");
            clauses.push("(LOWER(s.repo_full_name) LIKE ? OR LOWER(COALESCE(s.repo_description, '')) LIKE ? )".to_string());
            bindings.push(Value::from(pattern.clone()));
            bindings.push(Value::from(pattern));
        }

        if let Some(language) = sanitized
            .language
            .as_ref()
            .map(|v| v.trim().to_lowercase())
            .filter(|v| !v.is_empty())
        {
            clauses.push("LOWER(COALESCE(s.repo_language, '')) = ?".to_string());
            bindings.push(Value::from(language));
        }

        if let Some(activity) = sanitized
            .activity
            .as_ref()
            .map(|v| v.trim().to_lowercase())
            .filter(|v| !v.is_empty())
        {
            if activity == "unknown" {
                clauses.push("u.activity_tier IS NULL".to_string());
            } else {
                clauses.push("LOWER(COALESCE(u.activity_tier, '')) = ?".to_string());
                bindings.push(Value::from(activity));
            }
        }

        if let Some(user) = sanitized
            .user
            .as_ref()
            .map(|v| v.trim().to_lowercase())
            .filter(|v| !v.is_empty())
        {
            match sanitized.user_mode {
                UserFilterMode::Pin => {
                    clauses.push("LOWER(u.login) = ?".to_string());
                    bindings.push(Value::from(user));
                }
                UserFilterMode::Exclude => {
                    clauses.push("LOWER(u.login) != ?".to_string());
                    bindings.push(Value::from(user));
                }
                UserFilterMode::All => {}
            }
        }

        let base_where = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };

        Self {
            base_where,
            bindings,
            query: sanitized,
        }
    }

    fn count(&self, conn: &Connection) -> rusqlite::Result<usize> {
        let sql = format!(
            "SELECT COUNT(*) FROM stars s INNER JOIN users u ON u.user_id = s.user_id {}",
            self.base_where
        );
        conn.query_row(
            sql.as_str(),
            params_from_iter(self.bindings.iter()),
            |row| row.get::<_, i64>(0).map(|v| v as usize),
        )
    }

    fn max_fetched(&self, conn: &Connection) -> rusqlite::Result<Option<DateTime<Utc>>> {
        let sql = format!(
            "SELECT MAX(s.fetched_at) FROM stars s INNER JOIN users u ON u.user_id = s.user_id {}",
            self.base_where
        );
        let newest = conn
            .query_row(
                sql.as_str(),
                params_from_iter(self.bindings.iter()),
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?;
        newest
            .flatten()
            .map(|ts| parse_datetime_sql(&ts, 0))
            .transpose()
    }

    fn fetch_rows(&self, conn: &Connection) -> rusqlite::Result<Vec<StarFeedRow>> {
        let order_clause = match self.query.sort {
            StarSort::Newest => "ORDER BY s.fetched_at DESC, s.id DESC",
            StarSort::Alpha => "ORDER BY LOWER(s.repo_full_name) ASC, s.fetched_at DESC, s.id DESC",
        };
        let offset = (self.query.page - 1) * self.query.page_size;
        let sql = format!(
            "SELECT u.login, s.repo_full_name, s.repo_description, s.repo_language, s.repo_topics, s.repo_html_url, s.starred_at, s.fetched_at, u.activity_tier, s.id
             FROM stars s
             INNER JOIN users u ON u.user_id = s.user_id
             {where_clause}
             {order_clause}
             LIMIT ? OFFSET ?",
            where_clause = self.base_where,
            order_clause = order_clause
        );

        let mut params = self.bindings.clone();
        params.push(Value::from(self.query.page_size as i64));
        params.push(Value::from(offset as i64));

        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(params.iter()), |row| {
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
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use rusqlite::{Connection, params};
    use tempfile::NamedTempFile;

    use crate::db::init;

    use super::*;

    #[tokio::test]
    async fn query_filters_and_paginates() {
        let temp = NamedTempFile::new().unwrap();
        init(temp.path()).await.unwrap();

        let now = Utc::now();
        let conn = Connection::open(temp.path()).unwrap();
        conn.execute(
            "INSERT INTO users (user_id, login, last_starred_at, last_fetched_at, fetch_interval_minutes, next_check_at, activity_tier) VALUES (?1, ?2, ?3, ?3, 30, ?3, 'high')",
            params![1, "alice", now.to_rfc3339()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (user_id, login, last_starred_at, last_fetched_at, fetch_interval_minutes, next_check_at, activity_tier) VALUES (?1, ?2, ?3, ?3, 60, ?3, 'medium')",
            params![2, "bob", now.to_rfc3339()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO stars (user_id, repo_full_name, repo_description, repo_language, repo_topics, repo_html_url, starred_at, fetched_at)
             VALUES (?1, 'rust-lang/rust', 'Rust compiler', 'Rust', NULL, 'https://example.com/rust', ?2, ?2)",
            params![1, now.to_rfc3339()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO stars (user_id, repo_full_name, repo_description, repo_language, repo_topics, repo_html_url, starred_at, fetched_at)
             VALUES (?1, 'rust-lang/cargo', 'Rust package manager', 'Rust', NULL, 'https://example.com/cargo', ?2, ?2)",
            params![1, (now - Duration::minutes(5)).to_rfc3339()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO stars (user_id, repo_full_name, repo_description, repo_language, repo_topics, repo_html_url, starred_at, fetched_at)
             VALUES (?1, 'golang/go', 'Go repo', 'Go', NULL, 'https://example.com/go', ?2, ?2)",
            params![2, (now - Duration::minutes(10)).to_rfc3339()],
        )
        .unwrap();

        let query = StarQuery {
            language: Some("Rust".to_string()),
            user: Some("alice".to_string()),
            user_mode: UserFilterMode::Pin,
            page_size: 1,
            ..StarQuery::default()
        };
        let result = query_stars(temp.path(), &query).await.unwrap();
        assert_eq!(result.total, 2);
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].login, "alice");
        assert!(result.newest_fetched_at.is_some());
        let mut second_page_query = query.clone();
        second_page_query.page = 2;
        let second_result = query_stars(temp.path(), &second_page_query).await.unwrap();
        assert_eq!(second_result.items.len(), 1);
        assert_ne!(
            second_result.items[0].repo_full_name,
            result.items[0].repo_full_name
        );
    }

    #[tokio::test]
    async fn options_snapshot_counts_entities() {
        let temp = NamedTempFile::new().unwrap();
        init(temp.path()).await.unwrap();
        let now = Utc::now();
        let conn = Connection::open(temp.path()).unwrap();
        conn.execute(
            "INSERT INTO users (user_id, login, last_starred_at, last_fetched_at, fetch_interval_minutes, next_check_at, activity_tier) VALUES (?1, ?2, ?3, ?3, 30, ?3, 'high')",
            params![1, "alice", now.to_rfc3339()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO stars (user_id, repo_full_name, repo_description, repo_language, repo_topics, repo_html_url, starred_at, fetched_at)
             VALUES (?1, 'rust-lang/rust', 'Rust compiler', 'Rust', NULL, 'https://example.com/rust', ?2, ?2)",
            params![1, now.to_rfc3339()],
        )
        .unwrap();

        let snapshot = options_snapshot(temp.path()).await.unwrap();
        assert_eq!(snapshot.languages.len(), 1);
        assert_eq!(snapshot.languages[0].name, "Rust");
        assert_eq!(snapshot.languages[0].count, 1);
        assert_eq!(snapshot.users[0].login, "alice");
        assert!(snapshot.updated_at.is_some());
    }

    #[tokio::test]
    async fn next_check_summary_groups_by_tier() {
        let temp = NamedTempFile::new().unwrap();
        init(temp.path()).await.unwrap();
        let now = Utc::now();
        let conn = Connection::open(temp.path()).unwrap();
        conn.execute(
            "INSERT INTO users (user_id, login, last_starred_at, last_fetched_at, fetch_interval_minutes, next_check_at, activity_tier) VALUES (?1, ?2, ?3, ?3, 30, ?4, 'high')",
            params![1, "alice", now.to_rfc3339(), (now + Duration::minutes(30)).to_rfc3339()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (user_id, login, last_starred_at, last_fetched_at, fetch_interval_minutes, next_check_at, activity_tier) VALUES (?1, ?2, ?3, ?3, 60, ?4, NULL)",
            params![2, "bob", now.to_rfc3339(), (now + Duration::minutes(60)).to_rfc3339()],
        )
        .unwrap();

        let summary = next_check_summary(temp.path()).await.unwrap();
        assert!(summary.high.is_some());
        assert!(summary.unknown.is_some());
    }
}
