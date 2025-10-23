use std::cmp::Reverse;

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
    static TEMPLATE: &str = include_str!(concat!(env!("OUT_DIR"), "/frontend_index.html"));
    let timestamp = generated_at.to_rfc3339();
    let last = encode_text(&timestamp);
    TEMPLATE.replace("__LAST_UPDATED__", &last)
}
