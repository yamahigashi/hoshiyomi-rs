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
    let last_updated = generated_at.to_rfc3339();
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>GitHub Followings Stars</title>
    <style>
      :root {{
        color-scheme: light dark;
        --bg: #ffffff;
        --fg: #1b1f23;
        --muted: #586069;
        --border: #d0d7de;
        --accent: #0366d6;
        --badge-bg: #ddeeff;
        --badge-fg: #054da7;
      }}
      body {{
        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        margin: 0 auto;
        padding: 1.5rem;
        max-width: 960px;
        background: var(--bg);
        color: var(--fg);
      }}
      header {{
        display: flex;
        flex-direction: column;
        gap: 0.25rem;
        margin-bottom: 1.5rem;
      }}
      h1 {{
        margin: 0;
        font-size: 1.8rem;
      }}
      .timestamp {{
        color: var(--muted);
        font-size: 0.9rem;
      }}
      .controls {{
        display: grid;
        gap: 0.75rem;
        grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
        margin-bottom: 1rem;
      }}
      label.control {{
        display: flex;
        flex-direction: column;
        gap: 0.35rem;
        font-size: 0.85rem;
        text-transform: uppercase;
        letter-spacing: 0.04em;
        color: var(--muted);
      }}
      .controls input,
      .controls select,
      .controls button {{
        font: inherit;
        padding: 0.45rem 0.6rem;
        border-radius: 6px;
        border: 1px solid var(--border);
        background: var(--bg);
        color: var(--fg);
      }}
      .controls button {{
        cursor: pointer;
        justify-self: start;
      }}
      .status-line {{
        font-size: 0.9rem;
        color: var(--muted);
        margin-bottom: 0.75rem;
      }}
      .error-banner {{
        background: #fdecea;
        color: #611a15;
        border: 1px solid #f5c6cb;
        padding: 0.75rem;
        border-radius: 6px;
        margin-bottom: 1rem;
        display: none;
      }}
      .error-banner[aria-hidden="false"] {{
        display: block;
      }}
      .star-list {{
        list-style: none;
        padding: 0;
        margin: 0;
        display: flex;
        flex-direction: column;
        gap: 1rem;
      }}
      .star-item {{
        border: 1px solid var(--border);
        border-radius: 10px;
        padding: 1rem;
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
      }}
      .star-header {{
        display: flex;
        justify-content: space-between;
        align-items: center;
        gap: 0.5rem;
      }}
      .star-user {{
        font-weight: 600;
      }}
      .activity-tag {{
        font-size: 0.75rem;
        padding: 0.15rem 0.45rem;
        border-radius: 999px;
        text-transform: uppercase;
        letter-spacing: 0.05em;
        background: var(--badge-bg);
        color: var(--badge-fg);
      }}
      .repo-link {{
        color: var(--accent);
        font-size: 1.05rem;
        font-weight: 600;
        text-decoration: none;
      }}
      .repo-link:hover {{
        text-decoration: underline;
      }}
      .star-description {{
        margin: 0;
        font-size: 0.95rem;
      }}
      .star-meta {{
        display: flex;
        flex-wrap: wrap;
        gap: 0.5rem;
        font-size: 0.85rem;
        color: var(--muted);
        align-items: center;
      }}
      .fetch-time {{
        font-size: 0.8rem;
      }}
      .topic-tag {{
        background: rgba(3, 102, 214, 0.1);
        color: var(--accent);
        padding: 0.2rem 0.5rem;
        border-radius: 999px;
        font-size: 0.75rem;
      }}
      .empty-state {{
        text-align: center;
        padding: 2rem;
        border: 1px dashed var(--border);
        border-radius: 10px;
        color: var(--muted);
      }}
      @media (prefers-color-scheme: dark) {{
        :root {{
          --bg: #0d1117;
          --fg: #e6edf3;
          --muted: #8b949e;
          --border: #30363d;
          --accent: #58a6ff;
          --badge-bg: rgba(88, 166, 255, 0.15);
          --badge-fg: #58a6ff;
        }}
        .error-banner {{
          background: rgba(248, 81, 73, 0.15);
          color: #ffa198;
          border-color: rgba(248, 81, 73, 0.4);
        }}
        .topic-tag {{
          background: rgba(88, 166, 255, 0.18);
        }}
      }}
    </style>
  </head>
  <body>
    <header>
      <h1>GitHub Followings Stars</h1>
      <p class="timestamp">Last updated: {last}</p>
    </header>
    <section class="controls" aria-label="Filtering controls">
      <label class="control" for="search-input">Search
        <input id="search-input" type="search" placeholder="Search by repo, user, description…" autocomplete="off">
      </label>
      <label class="control" for="language-filter">Language
        <select id="language-filter">
          <option value="all">All languages</option>
        </select>
      </label>
      <label class="control" for="activity-filter">Activity
        <select id="activity-filter">
          <option value="all">All activity levels</option>
        </select>
      </label>
      <button id="sort-toggle" type="button" aria-pressed="false">Sort: Newest</button>
    </section>
    <div id="status-line" class="status-line">Loading…</div>
    <div id="error-banner" class="error-banner" role="alert" aria-hidden="true"></div>
    <div id="result-count" class="status-line" hidden></div>
    <ul id="star-list" class="star-list" aria-live="polite"></ul>
    <noscript>
      <p class="empty-state">JavaScript is required to explore stars interactively. Enable it and reload to search and filter.</p>
    </noscript>
    <script>
      (() => {{
        const state = {{
          items: [],
          search: "",
          language: "all",
          activity: "all",
          sort: "newest"
        }};
        const tierLabels = {{
          high: "High activity",
          medium: "Medium activity",
          low: "Low activity",
          unknown: "Unclassified"
        }};

        const searchInput = document.getElementById("search-input");
        const languageFilter = document.getElementById("language-filter");
        const activityFilter = document.getElementById("activity-filter");
        const sortToggle = document.getElementById("sort-toggle");
        const statusLine = document.getElementById("status-line");
        const errorBanner = document.getElementById("error-banner");
        const resultCount = document.getElementById("result-count");
        const list = document.getElementById("star-list");

        const setStatus = (message) => {{
          if (message) {{
            statusLine.textContent = message;
            statusLine.hidden = false;
          }} else {{
            statusLine.textContent = "";
            statusLine.hidden = true;
          }}
        }};

        const showError = (message) => {{
          errorBanner.textContent = message;
          errorBanner.setAttribute("aria-hidden", "false");
        }};

        const clearError = () => {{
          errorBanner.textContent = "";
          errorBanner.setAttribute("aria-hidden", "true");
        }};

        const populateFilters = () => {{
          const languages = new Set();
          const tiers = new Set();
          for (const item of state.items) {{
            if (item.repo_language) {{
              languages.add(item.repo_language);
            }}
            if (item.normalizedTier) {{
              tiers.add(item.normalizedTier);
            }}
          }}

          const sortedLangs = Array.from(languages).sort((a, b) => a.localeCompare(b));
          languageFilter.innerHTML = '<option value="all">All languages</option>';
          for (const lang of sortedLangs) {{
            const option = document.createElement("option");
            option.value = lang;
            option.textContent = lang;
            languageFilter.appendChild(option);
          }}

          const sortedTiers = Array.from(tiers).sort();
          activityFilter.innerHTML = '<option value="all">All activity levels</option>';
          for (const tier of sortedTiers) {{
            const option = document.createElement("option");
            option.value = tier;
            option.textContent = tierLabels[tier] ?? tier;
            activityFilter.appendChild(option);
          }}
        }};

        const renderTopics = (container, topics) => {{
          const limited = topics.slice(0, 10);
          for (const topic of limited) {{
            const span = document.createElement("span");
            span.className = "topic-tag";
            span.textContent = topic;
            container.appendChild(span);
          }}
        }};

        const renderList = (items) => {{
          list.replaceChildren();
          if (items.length === 0) {{
            const empty = document.createElement("li");
            empty.className = "empty-state";
            empty.textContent = "No matches found for the current filters.";
            list.appendChild(empty);
            return;
          }}

          for (const item of items) {{
            const li = document.createElement("li");
            li.className = "star-item";

            const header = document.createElement("div");
            header.className = "star-header";
            const userSpan = document.createElement("span");
            userSpan.className = "star-user";
            userSpan.textContent = item.login;
            header.appendChild(userSpan);

            if (item.normalizedTier) {{
              const tierSpan = document.createElement("span");
              tierSpan.className = `activity-tag activity-${{item.normalizedTier}}`;
              tierSpan.textContent = tierLabels[item.normalizedTier] ?? item.normalizedTier;
              header.appendChild(tierSpan);
            }}

            li.appendChild(header);

            const link = document.createElement("a");
            link.className = "repo-link";
            link.href = item.repo_html_url;
            link.textContent = item.repo_full_name;
            link.target = "_blank";
            link.rel = "noopener noreferrer";
            li.appendChild(link);

            if (item.repo_description) {{
              const desc = document.createElement("p");
              desc.className = "star-description";
              desc.textContent = item.repo_description;
              li.appendChild(desc);
            }}

            const meta = document.createElement("div");
            meta.className = "star-meta";

            if (item.repo_language) {{
              const lang = document.createElement("span");
              lang.textContent = item.repo_language;
              meta.appendChild(lang);
            }}

            if (item.repo_topics.length > 0) {{
              const topicsWrap = document.createElement("div");
              topicsWrap.className = "topics";
              renderTopics(topicsWrap, item.repo_topics);
              meta.appendChild(topicsWrap);
            }}

            const starredTime = document.createElement("time");
            starredTime.dateTime = item.starred_at;
            starredTime.textContent = new Date(item.starred_at).toLocaleString();
            meta.appendChild(starredTime);

            const fetchedSpan = document.createElement("span");
            fetchedSpan.className = "fetch-time";
            fetchedSpan.textContent = `Fetched: ${{new Date(item.fetched_at).toLocaleString()}}`;
            meta.appendChild(fetchedSpan);

            li.appendChild(meta);
            list.appendChild(li);
          }}
        }};

        const applyFilters = () => {{
          const search = state.search.trim().toLowerCase();
          let items = state.items.filter((item) => {{
            if (state.language !== "all" && item.repo_language !== state.language) {{
              return false;
            }}
            if (state.activity !== "all") {{
              if ((item.normalizedTier ?? "") !== state.activity) {{
                return false;
              }}
            }}
            if (!search) {{
              return true;
            }}
            const haystack = [
              item.repo_full_name,
              item.login,
              item.repo_description ?? "",
              item.repo_language ?? "",
              item.repo_topics.join(" ")
            ]
              .join(" ")
              .toLowerCase();
            return haystack.includes(search);
          }});

          if (state.sort === "alpha") {{
            items.sort((a, b) => a.repo_full_name.localeCompare(b.repo_full_name));
          }} else {{
            items.sort((a, b) => b.fetched_at_ms - a.fetched_at_ms);
          }}

          resultCount.hidden = false;
          resultCount.textContent = `Showing ${{items.length}} of ${{state.items.length}} starred repositories`;
          renderList(items);
        }};

        searchInput.addEventListener("input", (event) => {{
          state.search = event.target.value;
          applyFilters();
        }});
        languageFilter.addEventListener("change", (event) => {{
          state.language = event.target.value;
          applyFilters();
        }});
        activityFilter.addEventListener("change", (event) => {{
          state.activity = event.target.value;
          applyFilters();
        }});
        sortToggle.addEventListener("click", () => {{
          if (state.sort === "newest") {{
            state.sort = "alpha";
            sortToggle.textContent = "Sort: Alphabetical";
            sortToggle.setAttribute("aria-pressed", "true");
          }} else {{
            state.sort = "newest";
            sortToggle.textContent = "Sort: Newest";
            sortToggle.setAttribute("aria-pressed", "false");
          }}
          applyFilters();
        }});

        const bootstrap = async () => {{
          setStatus("Loading…");
          clearError();
          try {{
            const response = await fetch("/api/stars");
            if (!response.ok) {{
              throw new Error(`Request failed with status ${{response.status}}`);
            }}
            const raw = await response.json();
            state.items = raw.map((item) => {{
              const normalizedTier = item.user_activity_tier ? item.user_activity_tier.toLowerCase() : null;
              return {{
                ...item,
                normalizedTier,
                repo_topics: Array.isArray(item.repo_topics) ? item.repo_topics : [],
                starred_at_ms: Date.parse(item.starred_at) || 0,
                fetched_at_ms: Date.parse(item.fetched_at) || 0
              }};
            }});
            populateFilters();
            applyFilters();
          }} catch (err) {{
            console.error(err);
            showError("Failed to load starred repositories. Refresh to try again.");
          }} finally {{
            setStatus("");
          }}
        }};

        bootstrap();
      }})();
    </script>
  </body>
</html>
"#,
        last = encode_text(&last_updated)
    )
}
