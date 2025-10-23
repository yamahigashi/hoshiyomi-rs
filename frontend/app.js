(() => {
  const REFRESH_INTERVAL_MS = 5 * 60 * 1000;
  const MAX_BACKOFF_MS = 30 * 60 * 1000;
  const ACK_STORAGE_KEY = "starchaser:lastAckFetchedAt";
  const UI_STORAGE_KEY = "starchaser:uiState";
  const DEFAULT_PAGE_SIZE = 25;
  const PAGE_SIZE_OPTIONS = [10, 25, 50, 100];
  const QUICK_FILTER_LANG_LIMIT = 6;
  const GRID_BREAKPOINT = 1024;

  let fetchImpl = window.fetch.bind(window);
  let refreshTimer = null;
  let syncTicker = null;
  let backoffMs = REFRESH_INTERVAL_MS;
  let etag = null;
  let isFetching = false;
  let lastSyncedAt = null;
  let newestFetchedMs = 0;

  const dom = {
    searchInput: document.getElementById("search-input"),
    languageFilter: document.getElementById("language-filter"),
    activityFilter: document.getElementById("activity-filter"),
    sortToggle: document.getElementById("sort-toggle"),
    statusLine: document.getElementById("status-line"),
    errorBanner: document.getElementById("error-banner"),
    errorMessage: document.getElementById("error-message"),
    retryButton: document.getElementById("retry-button"),
    resultCount: document.getElementById("result-count"),
    list: document.getElementById("star-list"),
    syncStatus: document.getElementById("sync-status"),
    staleBadge: document.getElementById("sync-stale-badge"),
    markSeenButton: document.getElementById("mark-seen-button"),
    refreshButton: document.getElementById("refresh-button"),
    quickFilters: document.getElementById("quick-filters"),
    userFilterBanner: document.getElementById("user-filter-banner"),
    userFilterLabel: document.getElementById("user-filter-label"),
    userFilterClear: document.getElementById("user-filter-clear"),
    pagination: document.getElementById("pagination"),
    paginationInfo: document.getElementById("pagination-info"),
    pagePrev: document.getElementById("page-prev"),
    pageNext: document.getElementById("page-next"),
    pageSize: document.getElementById("page-size"),
    densityToggle: document.getElementById("density-toggle"),
    shortcutModal: document.getElementById("shortcut-modal"),
    shortcutClose: document.getElementById("shortcut-close"),
  };

  const state = {
    items: [],
    search: "",
    language: "all",
    activity: "all",
    sort: "newest",
    page: 1,
    pageSize: DEFAULT_PAGE_SIZE,
    userMode: "none", // none | pin | exclude
    userValue: null,
    hasNew: false,
    quickFilters: {
      languages: [],
    },
    density: "comfortable",
  };

  const tierLabels = {
    high: "High activity",
    medium: "Medium activity",
    low: "Low activity",
    unknown: "Unclassified",
  };

  let lastAcknowledgedMs = readAckTimestamp();

  function readAckTimestamp() {
    try {
      const raw = window.localStorage.getItem(ACK_STORAGE_KEY);
      if (raw === null) {
        return 0;
      }
      const parsed = Number.parseInt(raw, 10);
      return Number.isFinite(parsed) ? parsed : 0;
    } catch (err) {
      console.warn("Failed to read acknowledged timestamp", err);
      return 0;
    }
  }

  function persistAckTimestamp(value) {
    try {
      window.localStorage.setItem(ACK_STORAGE_KEY, String(value));
    } catch (err) {
      console.warn("Failed to persist acknowledged timestamp", err);
    }
  }

  function loadUiSnapshot() {
    let snapshot = {};
    try {
      const raw = window.localStorage.getItem(UI_STORAGE_KEY);
      if (raw) {
        snapshot = JSON.parse(raw);
      }
    } catch (err) {
      console.warn("Failed to parse stored UI state", err);
    }

    const params = new URLSearchParams(window.location.search);
    if (params.has("q")) {
      snapshot.search = params.get("q");
    }
    if (params.has("lang")) {
      snapshot.language = params.get("lang");
    }
    if (params.has("activity")) {
      snapshot.activity = params.get("activity");
    }
    if (params.has("sort")) {
      snapshot.sort = params.get("sort");
    }
    if (params.has("page")) {
      const parsed = Number.parseInt(params.get("page"), 10);
      if (Number.isFinite(parsed) && parsed > 0) {
        snapshot.page = parsed;
      }
    }
    if (params.has("pageSize")) {
      const parsed = Number.parseInt(params.get("pageSize"), 10);
      if (Number.isFinite(parsed) && parsed > 0) {
        snapshot.pageSize = parsed;
      }
    }
    if (params.has("user")) {
      snapshot.userValue = params.get("user");
    }
    if (params.has("userMode")) {
      snapshot.userMode = params.get("userMode");
    }
    if (params.has("density")) {
      snapshot.density = params.get("density");
    }
    return snapshot;
  }

  function applySnapshot(snapshot) {
    state.search = typeof snapshot.search === "string" ? snapshot.search : "";
    state.language = snapshot.language || "all";
    state.activity = snapshot.activity || "all";
    state.sort = snapshot.sort === "alpha" ? "alpha" : "newest";

    const sizeCandidate = snapshot.pageSize;
    if (Number.isInteger(sizeCandidate) && sizeCandidate > 0) {
      state.pageSize = sizeCandidate;
    } else {
      state.pageSize = DEFAULT_PAGE_SIZE;
    }

    const pageCandidate = snapshot.page;
    if (Number.isInteger(pageCandidate) && pageCandidate > 0) {
      state.page = pageCandidate;
    } else {
      state.page = 1;
    }

    if (snapshot.userMode === "pin" || snapshot.userMode === "exclude") {
      state.userMode = snapshot.userMode;
      state.userValue = snapshot.userValue || null;
    } else {
      state.userMode = "none";
      state.userValue = null;
    }

    if (snapshot.density === "compact") {
      state.density = "compact";
    } else {
      state.density = "comfortable";
    }
  }

  function persistUiState() {
    const snapshot = {
      search: state.search,
      language: state.language,
      activity: state.activity,
      sort: state.sort,
      page: state.page,
      pageSize: state.pageSize,
      userMode: state.userMode,
      userValue: state.userValue,
      density: state.density,
    };
    try {
      window.localStorage.setItem(UI_STORAGE_KEY, JSON.stringify(snapshot));
    } catch (err) {
      console.warn("Failed to persist UI state", err);
    }
  }

  function syncUrl() {
    const params = new URLSearchParams();
    const trimmedSearch = state.search.trim();
    if (trimmedSearch) {
      params.set("q", trimmedSearch);
    }
    if (state.language !== "all") {
      params.set("lang", state.language);
    }
    if (state.activity !== "all") {
      params.set("activity", state.activity);
    }
    if (state.sort !== "newest") {
      params.set("sort", state.sort);
    }
    if (state.page > 1) {
      params.set("page", String(state.page));
    }
    if (state.pageSize !== DEFAULT_PAGE_SIZE) {
      params.set("pageSize", String(state.pageSize));
    }
    if (state.userMode !== "none" && state.userValue) {
      params.set("user", state.userValue);
      params.set("userMode", state.userMode);
    }
    if (state.density !== "comfortable") {
      params.set("density", state.density);
    }

    const query = params.toString();
    const newUrl = `${window.location.pathname}${query ? `?${query}` : ""}`;
    window.history.replaceState(null, "", newUrl);
  }

  function setStatus(message) {
    if (!dom.statusLine) {
      return;
    }
    if (message) {
      dom.statusLine.textContent = message;
      dom.statusLine.hidden = false;
    } else {
      dom.statusLine.textContent = "";
      dom.statusLine.hidden = true;
    }
  }

  function showError(message) {
    dom.errorMessage.textContent = message;
    dom.errorBanner.setAttribute("aria-hidden", "false");
    dom.retryButton.disabled = false;
  }

  function clearError() {
    dom.errorMessage.textContent = "";
    dom.errorBanner.setAttribute("aria-hidden", "true");
    dom.retryButton.disabled = false;
  }

  function updateSyncStatus() {
    if (!dom.syncStatus) {
      return;
    }
    if (!lastSyncedAt) {
      dom.syncStatus.textContent = "Last synced: never";
      dom.staleBadge.hidden = true;
      return;
    }
    const ageMs = Date.now() - lastSyncedAt.getTime();
    const staleThreshold = REFRESH_INTERVAL_MS * 1.5;
    const isStale = ageMs > staleThreshold;
    dom.syncStatus.textContent = `Last synced: ${lastSyncedAt.toLocaleString()}`;
    if (isStale) {
      const ageMinutes = Math.round(ageMs / 60000);
      dom.staleBadge.textContent = ageMinutes > 0 ? `Stale (≈${ageMinutes} min)` : "Stale";
      dom.staleBadge.hidden = false;
    } else {
      dom.staleBadge.hidden = true;
    }
  }

  function scheduleNextFetch(delayMs) {
    if (refreshTimer !== null) {
      window.clearTimeout(refreshTimer);
    }
    refreshTimer = window.setTimeout(() => fetchStars({ manual: false }), delayMs);
  }

  function updateMarkSeenButton(newCount = null) {
    if (newCount === null) {
      newCount = state.items.reduce((acc, item) => acc + (item.isNew ? 1 : 0), 0);
    }
    state.hasNew = newCount > 0;
    if (newCount > 0) {
      dom.markSeenButton.disabled = false;
      dom.markSeenButton.textContent = `Mark ${newCount} new as seen`;
    } else {
      dom.markSeenButton.disabled = true;
      dom.markSeenButton.textContent = "Mark all seen";
    }
  }

  function flagNewItems() {
    let newCount = 0;
    for (const item of state.items) {
      const isNew = item.fetched_at_ms > lastAcknowledgedMs;
      item.isNew = isNew;
      if (isNew) {
        newCount += 1;
      }
    }
    updateMarkSeenButton(newCount);
  }

  function ensurePageSizeOption() {
    const select = dom.pageSize;
    const values = new Set([...PAGE_SIZE_OPTIONS, state.pageSize]);
    select.replaceChildren();
    Array.from(values)
      .sort((a, b) => a - b)
      .forEach((value) => {
        const option = document.createElement("option");
        option.value = String(value);
        option.textContent = String(value);
        select.appendChild(option);
      });
    select.value = String(state.pageSize);
  }

  function populateFilters() {
    const languages = new Set();
    const tiers = new Set();
    for (const item of state.items) {
      if (item.repo_language) {
        languages.add(item.repo_language);
      }
      if (item.normalizedTier) {
        tiers.add(item.normalizedTier);
      }
    }

    const sortedLangs = Array.from(languages).sort((a, b) => a.localeCompare(b));
    dom.languageFilter.innerHTML = '<option value="all">All languages</option>';
    for (const lang of sortedLangs) {
      const option = document.createElement("option");
      option.value = lang;
      option.textContent = lang;
      dom.languageFilter.appendChild(option);
    }
    if (!sortedLangs.includes(state.language)) {
      state.language = "all";
    }
    dom.languageFilter.value = state.language;

    const sortedTiers = Array.from(tiers).sort();
    dom.activityFilter.innerHTML = '<option value="all">All activity levels</option>';
    for (const tier of sortedTiers) {
      const option = document.createElement("option");
      option.value = tier;
      option.textContent = tierLabels[tier] ?? tier;
      dom.activityFilter.appendChild(option);
    }
    if (!sortedTiers.includes(state.activity)) {
      state.activity = "all";
    }
    dom.activityFilter.value = state.activity;

    ensurePageSizeOption();
  }

  function computeQuickFilters() {
    const languageCounts = new Map();
    for (const item of state.items) {
      if (item.repo_language) {
        languageCounts.set(item.repo_language, (languageCounts.get(item.repo_language) || 0) + 1);
      }
    }
    const languages = Array.from(languageCounts.entries())
      .sort((a, b) => b[1] - a[1])
      .slice(0, QUICK_FILTER_LANG_LIMIT)
      .map(([lang, count]) => ({ lang, count }));
    state.quickFilters.languages = languages;
  }

  function renderQuickFilters() {
    const container = dom.quickFilters;
    container.replaceChildren();

    const fragment = document.createDocumentFragment();

    const addChip = (label, handler, options = {}) => {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "quick-filter-chip";
      button.textContent = label;
      if (options.active) {
        button.classList.add("is-active");
      }
      if (options.title) {
        button.title = options.title;
      }
      button.addEventListener("click", handler);
      fragment.appendChild(button);
    };

    if (state.quickFilters.languages.length > 0) {
      const groupLabel = document.createElement("span");
      groupLabel.className = "quick-filter-label";
      groupLabel.textContent = "Top languages:";
      fragment.appendChild(groupLabel);
      for (const entry of state.quickFilters.languages) {
        addChip(entry.lang, () => {
          state.language = entry.lang;
          state.page = 1;
          persistUiState();
          syncUrl();
          applyFilters();
        }, {
          active: state.language === entry.lang,
          title: `Filter by ${entry.lang}`,
        });
      }
      addChip("All", () => {
        state.language = "all";
        state.page = 1;
        persistUiState();
        syncUrl();
        applyFilters();
      }, { active: state.language === "all", title: "Show all languages" });
    }

    const activityChips = ["high", "medium", "low"];
    if (activityChips.some((tier) => state.items.some((item) => item.normalizedTier === tier))) {
      const groupLabel = document.createElement("span");
      groupLabel.className = "quick-filter-label";
      groupLabel.textContent = "Activity:";
      fragment.appendChild(groupLabel);
      for (const tier of activityChips) {
        addChip(tierLabels[tier] ?? tier, () => {
          state.activity = tier;
          state.page = 1;
          persistUiState();
          syncUrl();
          applyFilters();
        }, {
          active: state.activity === tier,
          title: `Filter ${tierLabels[tier] ?? tier}`,
        });
      }
      addChip("All", () => {
        state.activity = "all";
        state.page = 1;
        persistUiState();
        syncUrl();
        applyFilters();
      }, { active: state.activity === "all", title: "Show all activity" });
    }

    if (fragment.childElementCount > 0) {
      container.appendChild(fragment);
      container.hidden = false;
    } else {
      container.hidden = true;
    }
  }

  function renderUserBanner() {
    if (state.userMode === "none" || !state.userValue) {
      dom.userFilterBanner.hidden = true;
      return;
    }
    const modeText = state.userMode === "pin"
      ? `Showing only stars from ${state.userValue}`
      : `Hiding stars from ${state.userValue}`;
    dom.userFilterLabel.textContent = modeText;
    dom.userFilterBanner.hidden = false;
  }

  function renderPagination(totalItems, totalPages) {
    if (totalPages <= 1) {
      dom.pagination.hidden = true;
      return;
    }
    const startIndex = (state.page - 1) * state.pageSize;
    const endIndex = Math.min(totalItems, startIndex + state.pageSize);
    dom.paginationInfo.textContent = `Showing ${startIndex + 1}–${endIndex} of ${totalItems} (page ${state.page} of ${totalPages})`;
    dom.pagePrev.disabled = state.page <= 1;
    dom.pageNext.disabled = state.page >= totalPages;
    dom.pagination.hidden = false;
  }

  function renderResultCount(total, pageItemsLength) {
    dom.resultCount.hidden = false;
    if (total === 0) {
      dom.resultCount.textContent = "No starred repositories match the current filters.";
      return;
    }
    const startIndex = (state.page - 1) * state.pageSize;
    const endIndex = startIndex + pageItemsLength;
    dom.resultCount.textContent = `Showing ${startIndex + 1}–${endIndex} of ${total} starred repositories`;
  }

  function renderList(items) {
    const list = dom.list;
    list.replaceChildren();
    const isCompact = state.density === "compact";
    if (items.length === 0) {
      const empty = document.createElement("li");
      empty.className = "empty-state";
      empty.textContent = "No matches found for the current filters.";
      list.appendChild(empty);
      return;
    }

    for (const item of items) {
      const li = document.createElement("li");
      li.className = "star-item";
      if (item.isNew) {
        li.classList.add("star-item--new");
      }
      if (isCompact) {
        li.classList.add("star-item--compact");
      }

      const header = document.createElement("div");
      header.className = "star-header";

      const userButton = document.createElement("button");
      userButton.type = "button";
      userButton.className = "star-user-button";
      userButton.textContent = item.login;
      userButton.dataset.login = item.login;
      const match = state.userValue === item.login;
      if (match && state.userMode === "pin") {
        userButton.classList.add("is-pinned");
        userButton.setAttribute("aria-pressed", "true");
      } else if (match && state.userMode === "exclude") {
        userButton.classList.add("is-excluded");
        userButton.setAttribute("aria-pressed", "true");
      } else {
        userButton.setAttribute("aria-pressed", "false");
      }
      userButton.title = "Click to cycle user filter";
      userButton.addEventListener("click", () => handleUserToggle(item.login));
      header.appendChild(userButton);

      if (item.isNew) {
        const newBadge = document.createElement("span");
        newBadge.className = "new-badge";
        newBadge.textContent = "New";
        header.appendChild(newBadge);
      }

      if (item.normalizedTier) {
        const tierSpan = document.createElement("span");
        tierSpan.className = `activity-tag activity-${item.normalizedTier}`;
        tierSpan.textContent = tierLabels[item.normalizedTier] ?? item.normalizedTier;
        header.appendChild(tierSpan);
      }

      li.appendChild(header);

      const link = document.createElement("a");
      link.className = "repo-link";
      link.href = item.repo_html_url;
      link.textContent = item.repo_full_name;
      link.target = "_blank";
      link.rel = "noopener noreferrer";
      li.appendChild(link);

      if (item.repo_description) {
        const desc = document.createElement("p");
        desc.className = "star-description";
        desc.textContent = item.repo_description;
        li.appendChild(desc);
      }

      const meta = document.createElement("div");
      meta.className = "star-meta";

      if (item.repo_language) {
        const lang = document.createElement("span");
        lang.textContent = item.repo_language;
        meta.appendChild(lang);
      }

      if (item.repo_topics.length > 0) {
        const topicsWrap = document.createElement("div");
        topicsWrap.className = "topics";
        const limited = item.repo_topics.slice(0, 10);
        for (const topic of limited) {
          const span = document.createElement("span");
          span.className = "topic-tag";
          span.textContent = topic;
          topicsWrap.appendChild(span);
        }
        meta.appendChild(topicsWrap);
      }

      const starredTime = document.createElement("time");
      starredTime.dateTime = item.starred_at;
      starredTime.textContent = new Date(item.starred_at).toLocaleString();
      meta.appendChild(starredTime);

      const fetchedSpan = document.createElement("span");
      fetchedSpan.className = "fetch-time";
      fetchedSpan.textContent = `Fetched: ${new Date(item.fetched_at).toLocaleString()}`;
      meta.appendChild(fetchedSpan);

      li.appendChild(meta);
      list.appendChild(li);
    }
  }

  function applyFilters() {
    let filtered = state.items;

    if (state.language !== "all") {
      filtered = filtered.filter((item) => item.repo_language === state.language);
    }
    if (state.activity !== "all") {
      filtered = filtered.filter((item) => (item.normalizedTier ?? "") === state.activity);
    }
    if (state.userMode === "pin" && state.userValue) {
      filtered = filtered.filter((item) => item.login === state.userValue);
    } else if (state.userMode === "exclude" && state.userValue) {
      filtered = filtered.filter((item) => item.login !== state.userValue);
    }

    const search = state.search.trim().toLowerCase();
    if (search) {
      filtered = filtered.filter((item) => {
        const haystack = [
          item.repo_full_name,
          item.login,
          item.repo_description ?? "",
          item.repo_language ?? "",
          item.repo_topics.join(" "),
        ]
          .join(" ")
          .toLowerCase();
        return haystack.includes(search);
      });
    }

    if (state.sort === "alpha") {
      filtered = filtered.slice().sort((a, b) => a.repo_full_name.localeCompare(b.repo_full_name));
    } else {
      filtered = filtered.slice().sort((a, b) => b.fetched_at_ms - a.fetched_at_ms);
    }

    const totalItems = filtered.length;
    const totalPages = Math.max(1, Math.ceil(totalItems / state.pageSize));
    let pageAdjusted = false;
    if (state.page > totalPages) {
      state.page = totalPages;
      pageAdjusted = true;
    }

    const startIndex = (state.page - 1) * state.pageSize;
    const pageItems = filtered.slice(startIndex, startIndex + state.pageSize);

    renderList(pageItems);
    updateListLayout();
    renderPagination(totalItems, totalPages);
    renderResultCount(totalItems, pageItems.length);
    renderUserBanner();
    renderQuickFilters();
    updateMarkSeenButton();

    if (pageAdjusted) {
      persistUiState();
      syncUrl();
    }
  }

  function handleUserToggle(login) {
    if (state.userValue !== login) {
      state.userMode = "pin";
      state.userValue = login;
    } else if (state.userMode === "pin") {
      state.userMode = "exclude";
    } else {
      state.userMode = "none";
      state.userValue = null;
    }
    state.page = 1;
    persistUiState();
    syncUrl();
    applyFilters();
  }

  function ingestItems(rawItems) {
    newestFetchedMs = 0;
    state.items = rawItems.map((item) => {
      const normalizedTier = item.user_activity_tier ? item.user_activity_tier.toLowerCase() : null;
      const topics = Array.isArray(item.repo_topics) ? item.repo_topics : [];
      const starredMs = Date.parse(item.starred_at) || 0;
      const fetchedMs = Date.parse(item.fetched_at) || 0;
      if (fetchedMs > newestFetchedMs) {
        newestFetchedMs = fetchedMs;
      }
      return {
        ...item,
        normalizedTier,
        repo_topics: topics,
        starred_at_ms: starredMs,
        fetched_at_ms: fetchedMs,
        isNew: false,
      };
    });
    flagNewItems();
    populateFilters();
    computeQuickFilters();
    applyFilters();
  }

  async function fetchStars(options = {}) {
    if (isFetching) {
      return;
    }
    const manual = Boolean(options.manual);
    const initial = Boolean(options.initial);
    isFetching = true;
    dom.refreshButton.disabled = true;
    dom.retryButton.disabled = true;

    if (initial) {
      setStatus("Loading…");
    } else if (manual) {
      setStatus("Refreshing…");
    } else {
      setStatus("Refreshing…");
    }

    clearError();

    try {
      const headers = {};
      if (etag) {
        headers["If-None-Match"] = etag;
      }
      const response = await fetchImpl("/api/stars", { headers });
      const responseEtag = response.headers.get("ETag");
      if (responseEtag) {
        etag = responseEtag;
      }
      if (response.status === 304) {
        lastSyncedAt = new Date();
        backoffMs = REFRESH_INTERVAL_MS;
        setStatus("");
        updateSyncStatus();
        scheduleNextFetch(REFRESH_INTERVAL_MS);
        return;
      }
      if (!response.ok) {
        throw new Error(`Request failed with status ${response.status}`);
      }
      const raw = await response.json();
      ingestItems(raw);
      lastSyncedAt = new Date();
      backoffMs = REFRESH_INTERVAL_MS;
      setStatus("");
      updateSyncStatus();
      scheduleNextFetch(REFRESH_INTERVAL_MS);
    } catch (err) {
      console.error(err);
      showError("Failed to refresh starred repositories. Refresh when ready.");
      backoffMs = Math.min(backoffMs * 2, MAX_BACKOFF_MS);
      const seconds = Math.max(Math.round(backoffMs / 1000), 1);
      setStatus(`Retrying in ${seconds}s…`);
      scheduleNextFetch(backoffMs);
    } finally {
      isFetching = false;
      dom.refreshButton.disabled = false;
      dom.retryButton.disabled = false;
      updateSyncStatus();
    }
  }

  function initControls() {
    dom.searchInput.value = state.search;
    updateSortToggle();
    ensurePageSizeOption();
    updateDensityToggle();

    dom.searchInput.addEventListener("input", (event) => {
      state.search = event.target.value;
      state.page = 1;
      persistUiState();
      syncUrl();
      applyFilters();
    });

    dom.languageFilter.addEventListener("change", (event) => {
      state.language = event.target.value || "all";
      state.page = 1;
      persistUiState();
      syncUrl();
      applyFilters();
    });

    dom.activityFilter.addEventListener("change", (event) => {
      state.activity = event.target.value || "all";
      state.page = 1;
      persistUiState();
      syncUrl();
      applyFilters();
    });

    dom.sortToggle.addEventListener("click", () => {
      if (state.sort === "newest") {
        state.sort = "alpha";
      } else {
        state.sort = "newest";
      }
      updateSortToggle();
      persistUiState();
      syncUrl();
      applyFilters();
    });

    dom.densityToggle.addEventListener("click", () => {
      state.density = state.density === "comfortable" ? "compact" : "comfortable";
      updateDensityToggle();
      persistUiState();
      syncUrl();
      applyFilters();
    });

    dom.pagePrev.addEventListener("click", () => {
      if (state.page > 1) {
        state.page -= 1;
        persistUiState();
        syncUrl();
        applyFilters();
      }
    });

    dom.pageNext.addEventListener("click", () => {
      state.page += 1;
      persistUiState();
      syncUrl();
      applyFilters();
    });

    dom.pageSize.addEventListener("change", (event) => {
      const value = Number.parseInt(event.target.value, 10);
      if (Number.isFinite(value) && value > 0) {
        state.pageSize = value;
        state.page = 1;
        persistUiState();
        syncUrl();
        applyFilters();
      }
    });

    dom.markSeenButton.addEventListener("click", () => {
      if (newestFetchedMs > 0) {
        lastAcknowledgedMs = newestFetchedMs;
        persistAckTimestamp(lastAcknowledgedMs);
        flagNewItems();
        applyFilters();
      }
    });

    dom.refreshButton.addEventListener("click", () => {
      backoffMs = REFRESH_INTERVAL_MS;
      fetchStars({ manual: true });
    });

    dom.retryButton.addEventListener("click", () => {
      backoffMs = REFRESH_INTERVAL_MS;
      clearError();
      fetchStars({ manual: true });
    });

    dom.userFilterClear.addEventListener("click", () => {
      state.userMode = "none";
      state.userValue = null;
      state.page = 1;
      persistUiState();
      syncUrl();
      applyFilters();
    });

    document.addEventListener("visibilitychange", () => {
      if (!document.hidden) {
        const age = lastSyncedAt ? Date.now() - lastSyncedAt.getTime() : Infinity;
        if (age > REFRESH_INTERVAL_MS) {
          backoffMs = REFRESH_INTERVAL_MS;
          fetchStars({ manual: true });
        }
      }
    });

    window.addEventListener("beforeunload", () => {
      if (refreshTimer !== null) {
        window.clearTimeout(refreshTimer);
      }
      if (syncTicker !== null) {
        window.clearInterval(syncTicker);
      }
    });

    window.addEventListener("resize", () => {
      updateListLayout();
    });
  }

  function updateSortToggle() {
    if (state.sort === "alpha") {
      dom.sortToggle.textContent = "Sort: Alphabetical";
      dom.sortToggle.setAttribute("aria-pressed", "true");
    } else {
      dom.sortToggle.textContent = "Sort: Newest";
      dom.sortToggle.setAttribute("aria-pressed", "false");
    }
  }

  function updateDensityToggle() {
    const pressed = state.density === "compact";
    dom.densityToggle.textContent = pressed ? "Layout: Compact" : "Layout: Comfortable";
    dom.densityToggle.setAttribute("aria-pressed", pressed ? "true" : "false");
  }

  function updateListLayout() {
    const list = dom.list;
    if (!list) {
      return;
    }
    const wide = window.innerWidth >= GRID_BREAKPOINT;
    list.classList.toggle("is-grid", wide);
    list.classList.toggle("is-compact", state.density === "compact");
  }

  function isEditableTarget(target) {
    if (!target) {
      return false;
    }
    const nodeName = target.nodeName.toLowerCase();
    if (nodeName === "input" || nodeName === "textarea" || target.isContentEditable) {
      return true;
    }
    return false;
  }

  function cycleLanguageFilter() {
    const options = Array.from(dom.languageFilter.options).map((opt) => opt.value);
    let idx = options.indexOf(state.language);
    if (idx === -1) {
      idx = 0;
    }
    idx = (idx + 1) % options.length;
    state.language = options[idx];
    dom.languageFilter.value = state.language;
    state.page = 1;
    persistUiState();
    syncUrl();
    applyFilters();
  }

  function openShortcutModal() {
    if (!dom.shortcutModal) {
      return;
    }
    dom.shortcutModal.hidden = false;
    dom.shortcutModal.removeAttribute("aria-hidden");
    if (dom.shortcutClose) {
      dom.shortcutClose.focus();
    }
  }

  function closeShortcutModal() {
    if (!dom.shortcutModal || dom.shortcutModal.hidden) {
      return;
    }
    dom.shortcutModal.hidden = true;
    dom.shortcutModal.setAttribute("aria-hidden", "true");
  }

  function toggleShortcutModal() {
    if (!dom.shortcutModal) {
      return;
    }
    if (dom.shortcutModal.hidden) {
      openShortcutModal();
    } else {
      closeShortcutModal();
    }
  }

  function registerKeyboardShortcuts() {
    document.addEventListener("keydown", (event) => {
      if (event.defaultPrevented) {
        return;
      }
      const target = event.target;
      const key = event.key;

      if (key === "Escape") {
        if (!dom.shortcutModal.hidden) {
          closeShortcutModal();
          event.preventDefault();
        }
        return;
      }

      if (isEditableTarget(target)) {
        return;
      }

      if (key === "/") {
        event.preventDefault();
        dom.searchInput.focus();
        dom.searchInput.select();
        return;
      }

      if (key === "?") {
        event.preventDefault();
        toggleShortcutModal();
        return;
      }

      if (key === "[") {
        event.preventDefault();
        dom.pagePrev.click();
        return;
      }

      if (key === "]") {
        event.preventDefault();
        dom.pageNext.click();
        return;
      }

      if (key === "l" || key === "L") {
        event.preventDefault();
        cycleLanguageFilter();
        return;
      }

      if (key === "m" || key === "M") {
        event.preventDefault();
        dom.markSeenButton.click();
        return;
      }

      if (key === "r" || key === "R") {
        event.preventDefault();
        backoffMs = REFRESH_INTERVAL_MS;
        fetchStars({ manual: true });
      }
    });

    if (dom.shortcutClose) {
      dom.shortcutClose.addEventListener("click", () => {
        closeShortcutModal();
      });
    }
    if (dom.shortcutModal) {
      dom.shortcutModal.addEventListener("click", (event) => {
        if (event.target === dom.shortcutModal) {
          closeShortcutModal();
        }
      });
    }
  }

  function registerTestHook() {
    if (window.__STARCHASER_TEST_HOOK__) {
      window.__STARCHASER_TEST_HOOK__({
        triggerRefresh: () => fetchStars({ manual: true }),
        overrideFetch: (fn) => {
          fetchImpl = fn;
        },
        state,
        getLastSyncedAt: () => lastSyncedAt,
        acknowledge: () => dom.markSeenButton.click(),
      });
    }
  }

  function initialise() {
    applySnapshot(loadUiSnapshot());
    initControls();
    updateSyncStatus();
    renderQuickFilters();
    renderUserBanner();
    updateListLayout();

    syncTicker = window.setInterval(updateSyncStatus, 60 * 1000);
    registerTestHook();
    registerKeyboardShortcuts();
    fetchStars({ initial: true });
  }

  initialise();
})();
