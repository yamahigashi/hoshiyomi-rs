use std::cmp::Reverse;
use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Utc};
use html_escape::encode_text;
use rss::{ChannelBuilder, GuidBuilder, ItemBuilder};

use crate::db::StarFeedRow;

const CHANNEL_TITLE: &str = "GitHub Followings Stars";
const CHANNEL_LINK: &str = "https://github.com";
const CHANNEL_DESCRIPTION: &str =
    "Aggregated feed of repositories starred by the accounts you follow on GitHub.";

pub fn build_feed(events: &[StarFeedRow], generated_at: DateTime<Utc>) -> Result<String> {
    let mut sorted = events.to_owned();
    sorted.sort_by_key(|event| Reverse(event.starred_at));
    let items = sorted.iter().map(build_item).collect::<Vec<_>>();
    let channel = ChannelBuilder::default()
        .title(CHANNEL_TITLE)
        .link(CHANNEL_LINK)
        .description(CHANNEL_DESCRIPTION)
        .last_build_date(generated_at.to_rfc2822())
        .items(items)
        .build();
    Ok(channel.to_string())
}

fn build_item(event: &StarFeedRow) -> rss::Item {
    let title = format!("{} starred {}", event.login, event.repo_full_name);
    let guid_value = format!(
        "github-star://{}/{}/{}",
        event.login,
        event.repo_full_name,
        event.starred_at.to_rfc3339()
    );
    let guid = GuidBuilder::default()
        .value(guid_value)
        .permalink(false)
        .build();
    let description = event
        .repo_description
        .as_ref()
        .map(|desc| format!("{}\nStarred by https://github.com/{}", desc, event.login))
        .unwrap_or_else(|| format!("Starred by https://github.com/{}", event.login));
    ItemBuilder::default()
        .title(title)
        .link(event.repo_html_url.clone())
        .description(description)
        .guid(guid)
        .pub_date(event.starred_at.to_rfc2822())
        .build()
}

pub fn build_html(_events: &[StarFeedRow], generated_at: DateTime<Utc>) -> String {
    let generated_at_str = generated_at.to_rfc3339();
    let last_updated = encode_text(&generated_at_str);
    if cfg!(debug_assertions)
        && let Some(html) = try_build_html_from_disk(&last_updated)
    {
        return html;
    }
    build_html_from_embedded(&last_updated)
}

fn build_html_from_embedded(last_updated: &str) -> String {
    static EMBEDDED_TEMPLATE: &str = include_str!(concat!(env!("OUT_DIR"), "/frontend_index.html"));
    EMBEDDED_TEMPLATE.replace("__LAST_UPDATED__", last_updated)
}

fn try_build_html_from_disk(last_updated: &str) -> Option<String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let template_path = manifest_dir.join("frontend/index.html");
    let styles_path = manifest_dir.join("frontend/styles.css");
    let script_path = manifest_dir.join("frontend/app.js");

    let template = match fs::read_to_string(&template_path) {
        Ok(contents) => contents,
        Err(err) => {
            eprintln!(
                "hoshiyomi: falling back to embedded frontend (failed to read {}): {}",
                template_path.display(),
                err
            );
            return None;
        }
    };

    let styles = match fs::read_to_string(&styles_path) {
        Ok(contents) => contents,
        Err(err) => {
            eprintln!(
                "hoshiyomi: falling back to embedded frontend (failed to read {}): {}",
                styles_path.display(),
                err
            );
            return None;
        }
    };

    let script = match fs::read_to_string(&script_path) {
        Ok(contents) => contents,
        Err(err) => {
            eprintln!(
                "hoshiyomi: falling back to embedded frontend (failed to read {}): {}",
                script_path.display(),
                err
            );
            return None;
        }
    };

    let bundled = template
        .replace("{{STYLE}}", styles.trim())
        .replace("{{SCRIPT}}", script.trim());

    Some(bundled.replace("__LAST_UPDATED__", last_updated))
}
