(() => {
	const REFRESH_INTERVAL_MS = 5 * 60 * 1000;
	const MAX_BACKOFF_MS = 30 * 60 * 1000;
	const ACK_STORAGE_KEY = "starchaser:lastAckFetchedAt";
	const UI_STORAGE_KEY = "starchaser:uiState";
	const DEFAULT_PAGE_SIZE = 25;
	const PAGE_SIZE_OPTIONS = [10, 25, 50, 100];
	const QUICK_FILTER_LANG_LIMIT = 6;
	const GRID_BREAKPOINT = 1024;
	const VIRTUALIZE_LENGTH_THRESHOLD = 500;
	const VIRTUAL_WINDOW = 40;
	const VIRTUAL_OVERSCAN = 10;
	const PRESET_STORAGE_KEY = "starchaser:viewPresets";
	const PRESET_LAST_KEY = "starchaser:lastPresetId";
	const MAX_PRESETS = 5;
	const PAGE_CACHE_LIMIT = 3;
	const perfApi = window.performance || null;
	const perfParams = new URLSearchParams(window.location.search);
	const PERF_DEBUG_ENABLED = perfParams.get("debug") === "perf";
	const BASE_PATH = (window.__HOSHI_PREFIX__ || "").replace(/\/+$/, "");

	function withBasePath(path) {
		const normalizedPath = path.startsWith("/") ? path : `/${path}`;
		if (!BASE_PATH) {
			return normalizedPath;
		}
		if (normalizedPath === "/") {
			return BASE_PATH;
		}
		return `${BASE_PATH}${normalizedPath}`;
	}

	let fetchImpl = window.fetch.bind(window);
	let refreshTimer = null;
	let syncTicker = null;
	let backoffMs = REFRESH_INTERVAL_MS;
	let isFetching = false;
	let lastSyncedAt = null;
	let newestFetchedMs = 0;
	const pageCache = new Map();
	const etagCache = new Map();
	let cacheSignature = "";
	let currentFilterSignature = "";
	let optionsEtag = null;
	let searchDebounce = null;
	const virtualState = {
		enabled: false,
		items: [],
		startIndex: 0,
		endIndex: 0,
		itemHeight: 320,
		listOffset: 0,
		listenersAttached: false,
		statusTimer: null,
	};

	function logPerf(message, extra = undefined) {
		if (!PERF_DEBUG_ENABLED) {
			return;
		}
		if (extra !== undefined) {
			console.info(`[perf] ${message}`, extra);
		} else {
			console.info(`[perf] ${message}`);
		}
	}

	function markPerf(label) {
		if (!PERF_DEBUG_ENABLED || !perfApi || typeof perfApi.mark !== "function") {
			return;
		}
		try {
			perfApi.mark(label);
		} catch (err) {
			console.warn("perf mark failed", label, err);
		}
	}

	function measurePerf(name, start, end) {
		if (!PERF_DEBUG_ENABLED || !perfApi || typeof perfApi.measure !== "function") {
			return;
		}
		try {
			perfApi.measure(name, start, end);
			const entries = perfApi.getEntriesByName(name);
			const entry = entries[entries.length - 1];
			if (entry) {
				logPerf(`${name}: ${entry.duration.toFixed(1)}ms`);
			}
		} catch (err) {
			console.warn("perf measure failed", name, err);
		}
	}

	function clearPerfMeasures(name) {
		if (!PERF_DEBUG_ENABLED || !perfApi || typeof perfApi.clearMeasures !== "function") {
			return;
		}
		try {
			perfApi.clearMeasures(name);
		} catch (err) {
			console.warn("perf clear failed", name, err);
		}
	}

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
		summaryStrip: document.getElementById("summary-strip"),
		summaryCount: document.getElementById("summary-count"),
		summaryUnseen: document.getElementById("summary-unseen"),
		summaryFilters: document.getElementById("summary-filters"),
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
		virtualStatus: document.getElementById("virtual-status"),
		presetBar: document.getElementById("preset-bar"),
		presetList: document.getElementById("preset-list"),
		presetSaveButton: document.getElementById("preset-save-button"),
		presetModal: document.getElementById("preset-modal"),
		presetModalClose: document.getElementById("preset-modal-close"),
		presetModalTitle: document.getElementById("preset-modal-title"),
		presetForm: document.getElementById("preset-form"),
		presetNameInput: document.getElementById("preset-name-input"),
		presetCancelButton: document.getElementById("preset-cancel-button"),
		presetDeleteButton: document.getElementById("preset-delete-button"),
		presetConfirmButton: document.getElementById("preset-confirm-button"),
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
		pageMeta: null,
		filterOptions: {
			languages: [],
			activity: [],
		},
		presets: [],
		activePresetId: null,
		renderSource: "network",
		firstListPaintRecorded: false,
	};

	markPerf("bootstrap:init");

	function noteFirstListPaint(kind) {
		if (state.firstListPaintRecorded) {
			return;
		}
		state.firstListPaintRecorded = true;
		const label = `render:first:${kind}`;
		markPerf(label);
		measurePerf(`time-to-${kind}`, "bootstrap:init", label);
		logPerf("first list render", { kind, page: state.page, source: state.renderSource });
	}

	const tierLabels = {
		high: "High activity",
		medium: "Medium activity",
		low: "Low activity",
		unknown: "Unclassified",
	};

	let lastAcknowledgedMs = readAckTimestamp();
	let isApplyingPreset = false;
	const presetDialogState = {
		mode: "create",
		targetId: null,
	};

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

	function snapshotFromState() {
		return {
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
	}

	function persistUiState() {
		const snapshot = { ...snapshotFromState(), page: 1 };
		if (!isApplyingPreset && state.activePresetId) {
			state.activePresetId = null;
			persistLastPresetId(null);
			renderPresets();
		}
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

	function computeFilterSignature(overrides = {}) {
		const searchValue =
			typeof overrides.search === "string" ? overrides.search : state.search;
		const languageValue = overrides.language ?? state.language;
		const activityValue = overrides.activity ?? state.activity;
		const sortValue = overrides.sort ?? state.sort;
		const pageSizeValue = overrides.pageSize ?? state.pageSize;
		const userModeValue = overrides.userMode ?? state.userMode;
		const userValue = overrides.userValue ?? state.userValue;
		return JSON.stringify({
			search: searchValue.trim().toLowerCase(),
			language: languageValue,
			activity: activityValue,
			sort: sortValue,
			pageSize: pageSizeValue,
			userMode: userModeValue,
			userValue:
				userModeValue === "pin" || userModeValue === "exclude" ? userValue : "",
		});
	}

	function resetPaginationCachesIfNeeded(providedSignature = null) {
		const nextSignature = providedSignature ?? computeFilterSignature();
		if (nextSignature !== currentFilterSignature) {
			currentFilterSignature = nextSignature;
			cacheSignature = nextSignature;
			pageCache.clear();
			etagCache.clear();
		}
	}

	function getUserModeParam(mode) {
		if (mode === "pin" || mode === "exclude") {
			return mode;
		}
		return "all";
	}

	function buildApiQuery(overrides = {}) {
		const params = new URLSearchParams();
		const searchValue =
			typeof overrides.search === "string" ? overrides.search : state.search;
		const trimmedSearch = searchValue.trim();
		if (trimmedSearch) {
			params.set("q", trimmedSearch);
		}
		const languageValue = overrides.language ?? state.language;
		if (languageValue && languageValue !== "all") {
			params.set("language", languageValue);
		}
		const activityValue = overrides.activity ?? state.activity;
		if (activityValue && activityValue !== "all") {
			params.set("activity", activityValue);
		}
		const userModeValue = overrides.userMode ?? state.userMode;
		const normalizedMode = getUserModeParam(userModeValue);
		params.set("user_mode", normalizedMode);
		const userValue = overrides.userValue ?? state.userValue;
		if (normalizedMode !== "all" && userValue) {
			params.set("user", userValue);
		}
		params.set("sort", overrides.sort ?? state.sort);
		params.set("page", String(overrides.page ?? state.page));
		params.set("page_size", String(overrides.pageSize ?? state.pageSize));
		return params;
	}

	function storePageCacheEntry(pageNumber, rawItems, meta) {
		if (cacheSignature !== currentFilterSignature) {
			cacheSignature = currentFilterSignature;
			pageCache.clear();
		}
		pageCache.set(pageNumber, { rawItems, meta });
		prunePageCache();
	}

	function getCachedPageEntry(pageNumber) {
		if (cacheSignature !== currentFilterSignature) {
			return null;
		}
		return pageCache.get(pageNumber) ?? null;
	}

	function prunePageCache() {
		if (pageCache.size <= PAGE_CACHE_LIMIT) {
			return;
		}
		const pages = Array.from(pageCache.keys());
		pages.sort((a, b) => {
			const distA = Math.abs(a - state.page);
			const distB = Math.abs(b - state.page);
			if (distA === distB) {
				return a - b;
			}
			return distA - distB;
		});
		while (pageCache.size > PAGE_CACHE_LIMIT && pages.length > 0) {
			const removed = pages.pop();
			if (typeof removed === "number") {
				pageCache.delete(removed);
			}
		}
	}

	function loadPresetsFromStorage() {
		try {
			const raw = window.localStorage.getItem(PRESET_STORAGE_KEY);
			if (!raw) {
				return [];
			}
			const parsed = JSON.parse(raw);
			if (Array.isArray(parsed)) {
				return parsed
					.filter(
						(entry) =>
							entry &&
							typeof entry.id === "string" &&
							typeof entry.name === "string" &&
							entry.snapshot &&
							typeof entry.snapshot === "object",
					)
					.slice(0, MAX_PRESETS);
			}
		} catch (err) {
			console.warn("Failed to load presets", err);
		}
		return [];
	}

	function persistPresets(presets) {
		try {
			window.localStorage.setItem(PRESET_STORAGE_KEY, JSON.stringify(presets));
		} catch (err) {
			console.warn("Failed to persist presets", err);
		}
	}

	function loadLastPresetId() {
		try {
			return window.localStorage.getItem(PRESET_LAST_KEY);
		} catch (err) {
			console.warn("Failed to load last preset id", err);
			return null;
		}
	}

	function persistLastPresetId(value) {
		try {
			if (value) {
				window.localStorage.setItem(PRESET_LAST_KEY, value);
			} else {
				window.localStorage.removeItem(PRESET_LAST_KEY);
			}
		} catch (err) {
			console.warn("Failed to persist last preset id", err);
		}
	}

	function renderPresets() {
		const list = dom.presetList;
		if (!list) {
			return;
		}
		list.replaceChildren();
		if (!Array.isArray(state.presets) || state.presets.length === 0) {
			const empty = document.createElement("p");
			empty.className = "preset-bar__empty";
			empty.textContent = "No saved views yet.";
			list.appendChild(empty);
			return;
		}

		state.presets.forEach((preset, index) => {
			const wrapper = document.createElement("div");
			wrapper.className = "preset-chip-wrapper";
			wrapper.setAttribute("role", "listitem");
			const button = document.createElement("button");
			button.type = "button";
			button.className = "preset-chip";
			button.setAttribute(
				"aria-pressed",
				preset.id === state.activePresetId ? "true" : "false",
			);
			if (preset.id === state.activePresetId) {
				button.classList.add("is-active");
			}
			button.textContent = "";
			const label = document.createElement("span");
			label.className = "preset-chip__label";
			label.textContent = preset.name;
			button.appendChild(label);
			button.addEventListener("click", () => {
				applyPresetById(preset.id);
			});

			if (index < 5) {
				const keySpan = document.createElement("span");
				keySpan.className = "preset-chip__keys";
				keySpan.textContent = `Alt+${index + 1}`;
				button.appendChild(keySpan);
			}

			const actions = document.createElement("div");
			actions.className = "preset-chip__actions";

			const editBtn = document.createElement("button");
			editBtn.type = "button";
			editBtn.className = "preset-chip__icon-btn";
			editBtn.textContent = "Edit";
			editBtn.setAttribute("aria-label", `Rename preset ${preset.name}`);
			editBtn.addEventListener("click", (event) => {
				event.stopPropagation();
				openPresetModal({ mode: "edit", preset });
			});

			const deleteBtn = document.createElement("button");
			deleteBtn.type = "button";
			deleteBtn.className = "preset-chip__icon-btn";
			deleteBtn.textContent = "Delete";
			deleteBtn.setAttribute("aria-label", `Delete preset ${preset.name}`);
			deleteBtn.addEventListener("click", (event) => {
				event.stopPropagation();
				deletePreset(preset.id);
			});

			actions.appendChild(editBtn);
			actions.appendChild(deleteBtn);

			wrapper.appendChild(button);
			wrapper.appendChild(actions);
			list.appendChild(wrapper);
		});
	}

	function openPresetModal(options = {}) {
		if (
			!dom.presetModal ||
			!dom.presetNameInput ||
			!dom.presetConfirmButton ||
			!dom.presetDeleteButton
		) {
			return;
		}
		const preset = options.preset;
		presetDialogState.mode = options.mode === "edit" ? "edit" : "create";
		presetDialogState.targetId = preset?.id ?? null;
		const initialName = preset?.name ?? "";
		dom.presetModal.hidden = false;
		dom.presetModal.setAttribute("aria-hidden", "false");
		if (dom.presetModalTitle) {
			dom.presetModalTitle.textContent =
				presetDialogState.mode === "edit"
					? "Update saved view"
					: "Save current view";
		}
		dom.presetNameInput.value = initialName;
		dom.presetDeleteButton.hidden = presetDialogState.mode !== "edit";
		dom.presetConfirmButton.textContent =
			presetDialogState.mode === "edit" ? "Update" : "Save";
		window.setTimeout(() => {
			dom.presetNameInput.focus();
			dom.presetNameInput.select();
		}, 0);
	}

	function closePresetModal() {
		if (!dom.presetModal || dom.presetModal.hidden) {
			return;
		}
		dom.presetModal.hidden = true;
		dom.presetModal.setAttribute("aria-hidden", "true");
		if (dom.presetForm) {
			dom.presetForm.reset();
		}
		presetDialogState.mode = "create";
		presetDialogState.targetId = null;
	}

	function generatePresetId() {
		if (window.crypto && window.crypto.randomUUID) {
			return window.crypto.randomUUID();
		}
		return `preset-${Date.now()}-${Math.random().toString(16).slice(2, 8)}`;
	}

	function upsertPreset(id, name) {
		const snapshot = snapshotFromState();
		const filtered = state.presets.filter((preset) => preset.id !== id);
		const entry = { id, name, snapshot };
		state.presets = [entry, ...filtered];
		if (state.presets.length > MAX_PRESETS) {
			state.presets.length = MAX_PRESETS;
		}
		persistPresets(state.presets);
		state.activePresetId = id;
		persistLastPresetId(id);
		renderPresets();
	}

	function deletePreset(id) {
		state.presets = state.presets.filter((preset) => preset.id !== id);
		persistPresets(state.presets);
		if (state.activePresetId === id) {
			state.activePresetId = null;
			persistLastPresetId(null);
		}
		renderPresets();
	}

	function handlePresetFormSubmit(event) {
		event.preventDefault();
		if (!dom.presetNameInput) {
			return;
		}
		const name = dom.presetNameInput.value.trim();
		if (!name) {
			dom.presetNameInput.focus();
			return;
		}
		if (presetDialogState.mode === "edit" && presetDialogState.targetId) {
			upsertPreset(presetDialogState.targetId, name);
		} else {
			upsertPreset(generatePresetId(), name);
		}
		closePresetModal();
	}

	function handlePresetDeleteFromModal() {
		if (!presetDialogState.targetId) {
			return;
		}
		deletePreset(presetDialogState.targetId);
		closePresetModal();
	}

	async function applyPresetById(id) {
		const preset = state.presets.find((entry) => entry.id === id);
		if (!preset) {
			return;
		}
		isApplyingPreset = true;
		try {
			const presetSnapshot = { ...(preset.snapshot || {}), page: 1 };
			applySnapshot(presetSnapshot);
			syncControlsToState();
			state.activePresetId = preset.id;
			persistLastPresetId(preset.id);
			await requestFilterUpdate();
		} finally {
			isApplyingPreset = false;
		}
		renderPresets();
	}

	function applyPresetByShortcut(index) {
		if (index < 0) {
			return;
		}
		const preset = state.presets[index];
		if (preset) {
			applyPresetById(preset.id);
		}
	}

	function setupPresetUi() {
		state.presets = loadPresetsFromStorage();
		const lastPresetId = loadLastPresetId();
		if (
			lastPresetId &&
			state.presets.some((preset) => preset.id === lastPresetId)
		) {
			state.activePresetId = lastPresetId;
		} else {
			state.activePresetId = null;
			persistLastPresetId(null);
		}
		if (!dom.presetBar) {
			return;
		}
		renderPresets();

		if (dom.presetSaveButton) {
			dom.presetSaveButton.addEventListener("click", () => {
				openPresetModal({ mode: "create" });
			});
		}
		if (dom.presetCancelButton) {
			dom.presetCancelButton.addEventListener("click", () => {
				closePresetModal();
			});
		}
		if (dom.presetModalClose) {
			dom.presetModalClose.addEventListener("click", () => {
				closePresetModal();
			});
		}
		if (dom.presetModal) {
			dom.presetModal.addEventListener("click", (event) => {
				if (event.target === dom.presetModal) {
					closePresetModal();
				}
			});
		}
		if (dom.presetForm) {
			dom.presetForm.addEventListener("submit", handlePresetFormSubmit);
		}
		if (dom.presetDeleteButton) {
			dom.presetDeleteButton.addEventListener(
				"click",
				handlePresetDeleteFromModal,
			);
		}
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
			dom.staleBadge.textContent =
				ageMinutes > 0 ? `Stale (≈${ageMinutes} min)` : "Stale";
			dom.staleBadge.hidden = false;
		} else {
			dom.staleBadge.hidden = true;
		}
	}

	function scheduleNextFetch(delayMs) {
		if (refreshTimer !== null) {
			window.clearTimeout(refreshTimer);
		}
		refreshTimer = window.setTimeout(
			() => loadPage(state.page, { manual: false }),
			delayMs,
		);
	}

	function updateMarkSeenButton(newCount = null) {
		if (newCount === null) {
			newCount = state.items.reduce(
				(acc, item) => acc + (item.isNew ? 1 : 0),
				0,
			);
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
		const sortedLangs = getLanguageOptions();
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

		const sortedTiers = getActivityOptions();
		dom.activityFilter.innerHTML =
			'<option value="all">All activity levels</option>';
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

	function getLanguageOptions() {
		if (state.filterOptions.languages.length > 0) {
			return state.filterOptions.languages
				.map((entry) => entry.name)
				.filter(Boolean);
		}
		const languages = new Set();
		for (const item of state.items) {
			if (item.repo_language) {
				languages.add(item.repo_language);
			}
		}
		return Array.from(languages).sort((a, b) => a.localeCompare(b));
	}

	function getActivityOptions() {
		if (state.filterOptions.activity.length > 0) {
			return state.filterOptions.activity
				.map((entry) => entry.tier)
				.filter(Boolean);
		}
		const tiers = new Set();
		for (const item of state.items) {
			if (item.normalizedTier) {
				tiers.add(item.normalizedTier);
			}
		}
		return Array.from(tiers).sort();
	}

	function computeQuickFilters() {
		let languages;
		if (state.filterOptions.languages.length > 0) {
			languages = state.filterOptions.languages
				.map((entry) => ({ lang: entry.name, count: entry.count }))
				.filter((entry) => entry.lang)
				.sort((a, b) => b.count - a.count)
				.slice(0, QUICK_FILTER_LANG_LIMIT);
		} else {
			const languageCounts = new Map();
			for (const item of state.items) {
				if (item.repo_language) {
					languageCounts.set(
						item.repo_language,
						(languageCounts.get(item.repo_language) || 0) + 1,
					);
				}
			}
			languages = Array.from(languageCounts.entries())
				.sort((a, b) => b[1] - a[1])
				.slice(0, QUICK_FILTER_LANG_LIMIT)
				.map(([lang, count]) => ({ lang, count }));
		}
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
				addChip(
					entry.lang,
					() => {
						state.language = entry.lang;
						dom.languageFilter.value = state.language;
						requestFilterUpdate();
					},
					{
						active: state.language === entry.lang,
						title: `Filter by ${entry.lang}`,
					},
				);
			}
			addChip(
				"All",
				() => {
					state.language = "all";
					dom.languageFilter.value = state.language;
					requestFilterUpdate();
				},
				{ active: state.language === "all", title: "Show all languages" },
			);
		}

		const activityChips = ["high", "medium", "low"];
		const availableActivity = new Set(getActivityOptions());
		if (activityChips.some((tier) => availableActivity.has(tier))) {
			const groupLabel = document.createElement("span");
			groupLabel.className = "quick-filter-label";
			groupLabel.textContent = "Activity:";
			fragment.appendChild(groupLabel);
			for (const tier of activityChips) {
				addChip(
					tierLabels[tier] ?? tier,
					() => {
						state.activity = tier;
						dom.activityFilter.value = state.activity;
						requestFilterUpdate();
					},
					{
						active: state.activity === tier,
						title: `Filter ${tierLabels[tier] ?? tier}`,
					},
				);
			}
			addChip(
				"All",
				() => {
					state.activity = "all";
					dom.activityFilter.value = state.activity;
					requestFilterUpdate();
				},
				{ active: state.activity === "all", title: "Show all activity" },
			);
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
		const modeText =
			state.userMode === "pin"
				? `Showing only stars from ${state.userValue}`
				: `Hiding stars from ${state.userValue}`;
		dom.userFilterLabel.textContent = modeText;
		dom.userFilterBanner.hidden = false;
	}

	function renderPagination() {
		const meta = state.pageMeta;
		if (!meta) {
			dom.pagination.hidden = true;
			return;
		}
		const totalCount =
			typeof meta.total === "number" ? meta.total : state.items.length;
		const totalPages = Math.max(
			1,
			Math.ceil(totalCount / (meta.page_size || state.pageSize || 1)),
		);
		if (totalPages <= 1) {
			dom.pagination.hidden = true;
			return;
		}
		const currentPage = meta.page || state.page;
		const pageSize = meta.page_size || state.pageSize;
		const startIndex = (currentPage - 1) * pageSize;
		const endIndex = Math.min(totalCount, startIndex + state.items.length);
		const startDisplay = Math.max(1, startIndex + 1);
		const endDisplay = Math.max(startDisplay, endIndex);
		dom.paginationInfo.textContent = `Showing ${startDisplay}–${endDisplay} of ${totalCount} (page ${currentPage} of ${totalPages})`;
		dom.pagePrev.disabled = !meta.has_prev || isFetching;
		dom.pageNext.disabled = !meta.has_next || isFetching;
		dom.pagination.hidden = false;
	}

	function renderSummary(pageItemsLength) {
		const summaryStrip = dom.summaryStrip;
		const summaryCount = dom.summaryCount;
		const summaryUnseen = dom.summaryUnseen;
		const summaryFilters = dom.summaryFilters;

		if (!summaryStrip || !summaryCount || !summaryUnseen || !summaryFilters) {
			if (dom.resultCount) {
				dom.resultCount.hidden = true;
			}
			return;
		}

		const meta = state.pageMeta;
		const totalItems =
			meta && typeof meta.total === "number" ? meta.total : state.items.length;
		const hasItems = totalItems > 0;
		let summaryText = "";
		if (!hasItems) {
			summaryText = "No starred repositories match the current view.";
		} else {
			const pageSize = meta?.page_size ?? state.pageSize;
			const currentPage = meta?.page ?? state.page;
			const startIndex = (currentPage - 1) * pageSize + 1;
			const endIndex = Math.min(totalItems, startIndex + pageItemsLength - 1);
			summaryText = `${startIndex}–${endIndex} of ${totalItems} starred repositories`;
		}
		summaryCount.textContent = summaryText;
		summaryStrip.classList.toggle("summary-strip--empty", !hasItems);

		const totalNew = state.items.reduce(
			(acc, item) => acc + (item.isNew ? 1 : 0),
			0,
		);
		if (totalNew > 0) {
			summaryUnseen.hidden = false;
			summaryUnseen.textContent = `${totalNew} new`;
		} else {
			summaryUnseen.hidden = true;
			summaryUnseen.textContent = "";
		}

		const filters = [];
		const trimmedSearch = state.search.trim();
		if (trimmedSearch) {
			filters.push({ key: "search", label: `Search: "${trimmedSearch}"` });
		}
		if (state.language !== "all") {
			filters.push({ key: "language", label: `Language: ${state.language}` });
		}
		if (state.activity !== "all") {
			filters.push({
				key: "activity",
				label: `Activity: ${tierLabels[state.activity] ?? state.activity}`,
			});
		}
		if (state.userMode === "pin" && state.userValue) {
			filters.push({ key: "user", label: `Pinned: ${state.userValue}` });
		} else if (state.userMode === "exclude" && state.userValue) {
			filters.push({ key: "user", label: `Excluded: ${state.userValue}` });
		}

		summaryFilters.replaceChildren();
		if (filters.length > 0) {
			const frag = document.createDocumentFragment();
			for (const filter of filters) {
				const chip = document.createElement("span");
				chip.className = `summary-chip summary-chip--${filter.key}`;
				chip.textContent = filter.label;
				frag.appendChild(chip);
			}
			summaryFilters.appendChild(frag);
			summaryFilters.hidden = false;
		} else {
			summaryFilters.hidden = true;
		}

		if (dom.resultCount) {
			dom.resultCount.textContent = summaryText;
			dom.resultCount.hidden = true;
		}
	}

	function renderList(items) {
		if (!dom.list) {
			return;
		}
		if (shouldVirtualizeList(items)) {
			enableVirtualScroll(items);
		} else {
			disableVirtualScroll();
			renderFullList(items);
		}
	}

	function renderFullList(items) {
		const list = dom.list;
		list.replaceChildren();
		const isCompact = state.density === "compact";
		if (items.length === 0) {
			list.appendChild(createEmptyStateItem());
			return;
		}
		for (const item of items) {
			list.appendChild(createCardElement(item, isCompact));
		}
		noteFirstListPaint("full");
	}

	function createCardElement(item, isCompact, itemIndex = null) {
		const card = document.createElement("li");
		card.className = "star-card";
		if (item.isNew) {
			card.classList.add("star-card--new");
		}
		if (isCompact) {
			card.classList.add("star-card--compact");
		}
		const isPinnedUser =
			state.userMode === "pin" && state.userValue === item.login;
		if (isPinnedUser) {
			card.classList.add("star-card--pinned");
		}

		const header = document.createElement("div");
		header.className = "star-card__header";

		if (itemIndex !== null) {
			card.dataset.itemIndex = String(itemIndex);
		}

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
			card.classList.add("star-card--excluded");
		} else {
			userButton.setAttribute("aria-pressed", "false");
		}
		userButton.title = "Click to cycle user filter";
		userButton.addEventListener("click", () => handleUserToggle(item.login));
		header.appendChild(userButton);

		const headerSignals = document.createElement("div");
		headerSignals.className = "star-card__signals";

		if (item.isNew) {
			const newBadge = document.createElement("span");
			newBadge.className = "new-badge";
			newBadge.textContent = "New";
			headerSignals.appendChild(newBadge);
		}

		if (item.normalizedTier) {
			const tierSpan = document.createElement("span");
			tierSpan.className = `activity-tag activity-${item.normalizedTier}`;
			tierSpan.textContent =
				tierLabels[item.normalizedTier] ?? item.normalizedTier;
			headerSignals.appendChild(tierSpan);
		}

		if (headerSignals.childElementCount > 0) {
			header.appendChild(headerSignals);
		}

		card.appendChild(header);

		const body = document.createElement("div");
		body.className = "star-card__body";

		const link = document.createElement("a");
		link.className = "repo-link";
		link.href = item.repo_html_url;
		link.textContent = item.repo_full_name;
		link.target = "_blank";
		link.rel = "noopener noreferrer";
		body.appendChild(link);

		if (item.repo_description) {
			const desc = document.createElement("p");
			desc.className = "star-description";
			desc.textContent = item.repo_description;
			body.appendChild(desc);
		}

		card.appendChild(body);

		const footer = document.createElement("div");
		footer.className = "star-card__footer";

		const meta = document.createElement("div");
		meta.className = "star-meta";

		if (item.repo_language) {
			const lang = document.createElement("span");
			lang.className = "meta-pill";
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

		footer.appendChild(meta);

		const times = document.createElement("div");
		times.className = "star-card__timestamps";

		const starredLabel = document.createElement("span");
		starredLabel.className = "timestamp-label";
		starredLabel.textContent = "Starred";
		times.appendChild(starredLabel);

		const starredTime = document.createElement("time");
		starredTime.dateTime = item.starred_at;
		starredTime.textContent = new Date(item.starred_at).toLocaleString();
		starredTime.className = "timestamp-value";
		times.appendChild(starredTime);

		const fetchedLabel = document.createElement("span");
		fetchedLabel.className = "timestamp-label";
		fetchedLabel.textContent = "Fetched";
		times.appendChild(fetchedLabel);

		const fetchedTime = document.createElement("time");
		fetchedTime.dateTime = item.fetched_at;
		fetchedTime.textContent = new Date(item.fetched_at).toLocaleString();
		fetchedTime.className = "timestamp-value";
		times.appendChild(fetchedTime);

		footer.appendChild(times);

		card.appendChild(footer);
		return card;
	}

	function createEmptyStateItem() {
		const empty = document.createElement("li");
		empty.className = "empty-state";
		empty.textContent = "No matches found for the current filters.";
		return empty;
	}

	function shouldVirtualizeList(items) {
		const totalAvailable =
			state.pageMeta && typeof state.pageMeta.total === "number"
				? state.pageMeta.total
				: items.length;
		return (
			(totalAvailable > VIRTUALIZE_LENGTH_THRESHOLD || state.pageSize > 50) &&
			items.length > VIRTUAL_WINDOW
		);
	}

	function enableVirtualScroll(items) {
		virtualState.enabled = true;
		virtualState.items = items;
		virtualState.startIndex = 0;
		virtualState.endIndex = Math.min(items.length, VIRTUAL_WINDOW);
		updateVirtualOffsets();
		attachVirtualListeners();
		drawVirtualSlice(true);
	}

	function disableVirtualScroll() {
		if (!virtualState.enabled) {
			return;
		}
		virtualState.enabled = false;
		virtualState.items = [];
		virtualState.startIndex = 0;
		virtualState.endIndex = 0;
		detachVirtualListeners();
		if (dom.virtualStatus) {
			dom.virtualStatus.hidden = true;
		}
	}

	function attachVirtualListeners() {
		if (virtualState.listenersAttached) {
			return;
		}
		window.addEventListener("scroll", handleVirtualScroll, { passive: true });
		window.addEventListener("resize", handleVirtualResize);
		virtualState.listenersAttached = true;
	}

	function detachVirtualListeners() {
		if (!virtualState.listenersAttached) {
			return;
		}
		window.removeEventListener("scroll", handleVirtualScroll);
		window.removeEventListener("resize", handleVirtualResize);
		virtualState.listenersAttached = false;
	}

	function handleVirtualScroll() {
		if (!virtualState.enabled) {
			return;
		}
		updateVirtualWindow(false);
	}

	function handleVirtualResize() {
		if (!virtualState.enabled) {
			return;
		}
		updateVirtualOffsets();
		updateVirtualWindow(true);
	}

	function updateVirtualOffsets() {
		const listRect = dom.list.getBoundingClientRect();
		virtualState.listOffset = listRect.top + window.scrollY;
	}

	function updateVirtualWindow(force) {
		if (!virtualState.enabled || virtualState.items.length === 0) {
			return;
		}
		const scrollOffset = Math.max(0, window.scrollY - virtualState.listOffset);
		const estimatedIndex = Math.floor(
			scrollOffset / Math.max(virtualState.itemHeight, 1),
		);
		let nextStart = Math.max(0, estimatedIndex - VIRTUAL_OVERSCAN);
		let nextEnd = Math.min(
			virtualState.items.length,
			nextStart + VIRTUAL_WINDOW,
		);
		const focusedIndex = getFocusedVirtualIndex();
		if (focusedIndex !== null) {
			if (focusedIndex < nextStart) {
				nextStart = Math.max(0, focusedIndex - VIRTUAL_OVERSCAN);
			} else if (focusedIndex >= nextEnd) {
				nextStart = Math.max(0, focusedIndex - Math.floor(VIRTUAL_WINDOW / 2));
			}
			nextEnd = Math.min(virtualState.items.length, nextStart + VIRTUAL_WINDOW);
		}
		if (
			!force &&
			nextStart === virtualState.startIndex &&
			nextEnd === virtualState.endIndex
		) {
			return;
		}
		virtualState.startIndex = nextStart;
		virtualState.endIndex = nextEnd;
		drawVirtualSlice();
	}

	function drawVirtualSlice(forceEmptyCheck) {
		const list = dom.list;
		list.replaceChildren();
		if (virtualState.items.length === 0) {
			list.appendChild(createEmptyStateItem());
			return;
		}
		showVirtualStatus("Loading more stars…");
		const isCompact = state.density === "compact";
		const beforeSpacer = document.createElement("li");
		beforeSpacer.className = "virtual-spacer";
		beforeSpacer.style.height = `${virtualState.startIndex * virtualState.itemHeight}px`;
		beforeSpacer.setAttribute("aria-hidden", "true");
		list.appendChild(beforeSpacer);

	const slice = virtualState.items.slice(
		virtualState.startIndex,
		virtualState.endIndex,
	);
	if (slice.length === 0 && forceEmptyCheck) {
		list.appendChild(createEmptyStateItem());
	}
	slice.forEach((item, idx) => {
		list.appendChild(
			createCardElement(item, isCompact, virtualState.startIndex + idx),
		);
	});
	if (slice.length > 0) {
		noteFirstListPaint("virtual");
	}

		const afterSpacer = document.createElement("li");
		afterSpacer.className = "virtual-spacer";
		const remaining = Math.max(
			0,
			virtualState.items.length - virtualState.endIndex,
		);
		afterSpacer.style.height = `${remaining * virtualState.itemHeight}px`;
		afterSpacer.setAttribute("aria-hidden", "true");
		list.appendChild(afterSpacer);

		measureVirtualItemHeight();
	}

	function measureVirtualItemHeight() {
		const cards = dom.list.querySelectorAll(".star-card");
		if (!cards || cards.length === 0) {
			return;
		}
		const totalHeight = Array.from(cards).reduce(
			(acc, card) => acc + card.getBoundingClientRect().height,
			0,
		);
		const average = totalHeight / cards.length;
		if (
			Number.isFinite(average) &&
			average > 0 &&
			Math.abs(average - virtualState.itemHeight) > 5
		) {
			virtualState.itemHeight = average;
			updateVirtualWindow(true);
		}
	}

	function showVirtualStatus(message, duration = 500) {
		if (!dom.virtualStatus) {
			return;
		}
		dom.virtualStatus.textContent = message;
		dom.virtualStatus.hidden = false;
		if (virtualState.statusTimer) {
			window.clearTimeout(virtualState.statusTimer);
		}
		virtualState.statusTimer = window.setTimeout(() => {
			dom.virtualStatus.hidden = true;
		}, duration);
	}

	function getFocusedVirtualIndex() {
		const active = document.activeElement;
		if (!active || !dom.list) {
			return null;
		}
		if (!dom.list.contains(active)) {
			return null;
		}
		const card = active.closest?.(".star-card");
		if (!card) {
			return null;
		}
		const value = card.getAttribute("data-item-index");
		if (value === null) {
			return null;
		}
		const parsed = Number.parseInt(value, 10);
		return Number.isFinite(parsed) ? parsed : null;
	}

	function applyFilters() {
		const visible = state.items.slice();
		if (state.sort === "alpha") {
			visible.sort((a, b) => a.repo_full_name.localeCompare(b.repo_full_name));
		} else {
			visible.sort((a, b) => b.fetched_at_ms - a.fetched_at_ms);
		}

		renderList(visible);
		updateListLayout();
		renderPagination();
		renderSummary(visible.length);
		renderUserBanner();
		renderQuickFilters();
		updateMarkSeenButton();
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
		requestFilterUpdate();
	}

	function normalizeItems(rawItems) {
		let newest = 0;
		const normalized = rawItems.map((item) => {
			const normalizedTier = item.user_activity_tier
				? item.user_activity_tier.toLowerCase()
				: null;
			const topics = Array.isArray(item.repo_topics) ? item.repo_topics : [];
			const starredMs = Date.parse(item.starred_at) || 0;
			const fetchedMs = Date.parse(item.fetched_at) || 0;
			if (fetchedMs > newest) {
				newest = fetchedMs;
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
		return { items: normalized, newestFetched: newest };
	}

	function applyPageData(normalizedItems, meta, newestMs, options = {}) {
		newestFetchedMs = newestMs || 0;
		state.items = normalizedItems;
		state.pageMeta = meta || null;
		const nextPage = meta?.page ?? state.page;
		const pageChanged = state.page !== nextPage;
		state.page = nextPage;
		state.renderSource = options.source || "network";
		state.firstListPaintRecorded = false;
		flagNewItems();
		populateFilters();
		computeQuickFilters();
		applyFilters();
		const shouldPersist =
			options.shouldPersist ?? options.manual ?? pageChanged;
		if (shouldPersist) {
			persistUiState();
			syncUrl();
		}
	}

	async function loadPage(page, options = {}) {
		const targetPage = Math.max(1, page || 1);
		const manual = Boolean(options.manual);
		const initial = Boolean(options.initial);
		const bypassCache = Boolean(options.bypassCache);
		resetPaginationCachesIfNeeded();
		const loadMarker = `loadPage:p${targetPage}`;
		markPerf(loadMarker);
		logPerf("loadPage", { page: targetPage, manual, initial, bypassCache });
		if (!bypassCache) {
			const cached = getCachedPageEntry(targetPage);
			if (cached) {
				logPerf("cache hit", { page: targetPage });
				const normalized = normalizeItems(cached.rawItems);
				applyPageData(normalized.items, cached.meta, normalized.newestFetched, {
					manual,
					source: "cache",
				});
				return;
			}
		}
		await fetchStars({ manual, initial, pageOverride: targetPage });
	}

	async function fetchStars(options = {}) {
		if (isFetching) {
			return;
		}
		const manual = Boolean(options.manual);
		const initial = Boolean(options.initial);
		const targetPage = Math.max(1, options.pageOverride ?? state.page ?? 1);
		const ignoreEtag = Boolean(options.ignoreEtag);
		const fetchMarker = `fetch:stars:p${targetPage}`;
		markPerf(`${fetchMarker}:start`);
		logPerf("fetch start", { page: targetPage, manual, initial, ignoreEtag });
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
			const etagKey = `${currentFilterSignature}|${targetPage}`;
			if (!ignoreEtag && etagCache.has(etagKey)) {
				headers["If-None-Match"] = etagCache.get(etagKey);
			}
			const queryString = buildApiQuery({ page: targetPage }).toString();
			const response = await fetchImpl(
				withBasePath(`/api/stars${queryString ? `?${queryString}` : ""}`),
				{ headers },
			);
			const responseEtag = response.headers.get("ETag");
			if (responseEtag) {
				etagCache.set(etagKey, responseEtag);
			}
			if (response.status === 304) {
				const cached = getCachedPageEntry(targetPage);
				if (cached) {
					const normalized = normalizeItems(cached.rawItems);
					applyPageData(
						normalized.items,
						cached.meta,
						normalized.newestFetched,
						{ manual, source: "cache" },
					);
					markPerf(`${fetchMarker}:304-cache`);
					measurePerf(
						`${fetchMarker}:duration`,
						`${fetchMarker}:start`,
						`${fetchMarker}:304-cache`,
					);
					clearPerfMeasures(`${fetchMarker}:duration`);
					logPerf("fetch reused cache", { page: targetPage });
					lastSyncedAt = new Date();
					backoffMs = REFRESH_INTERVAL_MS;
					setStatus("");
					updateSyncStatus();
					scheduleNextFetch(REFRESH_INTERVAL_MS);
					return;
				}
				etagCache.delete(etagKey);
				await fetchStars({
					manual,
					initial,
					pageOverride: targetPage,
					ignoreEtag: true,
				});
				return;
			}
			if (!response.ok) {
				throw new Error(`Request failed with status ${response.status}`);
			}
			const payload = await response.json();
      console.log('Fetched payload count:', Array.isArray(payload) ? payload.length : payload?.items?.length);
      console.log('Fetched payload:', payload);
			const rawItems = Array.isArray(payload)
				? payload
				: Array.isArray(payload?.items)
					? payload.items
					: [];
			const meta = payload?.meta ?? null;
			storePageCacheEntry(meta?.page ?? targetPage, rawItems, meta);
			const normalized = normalizeItems(rawItems);
			markPerf(`${fetchMarker}:done`);
			measurePerf(
				`${fetchMarker}:duration`,
				`${fetchMarker}:start`,
				`${fetchMarker}:done`,
			);
			clearPerfMeasures(`${fetchMarker}:duration`);
			applyPageData(normalized.items, meta, normalized.newestFetched, {
				manual,
				source: options.source || "network",
			});
			logPerf("fetch success", { page: targetPage, count: normalized.items.length });
			lastSyncedAt = new Date();
			backoffMs = REFRESH_INTERVAL_MS;
			setStatus("");
			updateSyncStatus();
			scheduleNextFetch(REFRESH_INTERVAL_MS);
		} catch (err) {
			console.error(err);
			logPerf("fetch failure", { page: targetPage, message: err?.message });
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
			renderPagination();
		}
	}

	async function fetchFilterOptions() {
		try {
			const headers = {};
			if (optionsEtag) {
				headers["If-None-Match"] = optionsEtag;
			}
			const response = await fetchImpl(withBasePath("/api/options"), { headers });
			if (response.status === 304) {
				return;
			}
			if (!response.ok) {
				throw new Error(
					`Options request failed with status ${response.status}`,
				);
			}
			const payload = await response.json();
			state.filterOptions.languages = Array.isArray(payload?.languages)
				? payload.languages
				: [];
			state.filterOptions.activity = Array.isArray(payload?.activity_tiers)
				? payload.activity_tiers
				: [];
			optionsEtag = response.headers.get("ETag") || optionsEtag;
			populateFilters();
			computeQuickFilters();
		} catch (err) {
			console.warn("Failed to fetch filter options", err);
		}
	}

	function initControls() {
		dom.searchInput.value = state.search;
		updateSortToggle();
		ensurePageSizeOption();
		updateDensityToggle();

		dom.searchInput.addEventListener("input", (event) => {
			state.search = event.target.value;
			if (searchDebounce) {
				window.clearTimeout(searchDebounce);
			}
			searchDebounce = window.setTimeout(() => {
				requestFilterUpdate();
			}, 250);
		});

		dom.languageFilter.addEventListener("change", (event) => {
			state.language = event.target.value || "all";
			requestFilterUpdate();
		});

		dom.activityFilter.addEventListener("change", (event) => {
			state.activity = event.target.value || "all";
			requestFilterUpdate();
		});

		dom.sortToggle.addEventListener("click", () => {
			if (state.sort === "newest") {
				state.sort = "alpha";
			} else {
				state.sort = "newest";
			}
			updateSortToggle();
			requestFilterUpdate();
		});

		dom.densityToggle.addEventListener("click", () => {
			state.density =
				state.density === "comfortable" ? "compact" : "comfortable";
			updateDensityToggle();
			persistUiState();
			syncUrl();
			applyFilters();
		});

		dom.pagePrev.addEventListener("click", () => {
			if (state.pageMeta?.has_prev) {
				loadPage(Math.max(1, state.page - 1), { manual: true });
			}
		});

		dom.pageNext.addEventListener("click", () => {
			if (state.pageMeta?.has_next) {
				loadPage(state.page + 1, { manual: true });
			}
		});

		dom.pageSize.addEventListener("change", (event) => {
			const value = Number.parseInt(event.target.value, 10);
			if (Number.isFinite(value) && value > 0) {
				state.pageSize = value;
				requestFilterUpdate();
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
			loadPage(state.page, { manual: true, bypassCache: true });
		});

		dom.retryButton.addEventListener("click", () => {
			backoffMs = REFRESH_INTERVAL_MS;
			clearError();
			loadPage(state.page, { manual: true, bypassCache: true });
		});

		dom.userFilterClear.addEventListener("click", () => {
			state.userMode = "none";
			state.userValue = null;
			requestFilterUpdate();
		});

		document.addEventListener("visibilitychange", () => {
			if (!document.hidden) {
				const age = lastSyncedAt
					? Date.now() - lastSyncedAt.getTime()
					: Infinity;
				if (age > REFRESH_INTERVAL_MS) {
					backoffMs = REFRESH_INTERVAL_MS;
					loadPage(state.page, { manual: true, bypassCache: true });
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

	function syncControlsToState() {
		if (dom.searchInput) {
			dom.searchInput.value = state.search;
		}
		if (dom.languageFilter) {
			dom.languageFilter.value = state.language;
		}
		if (dom.activityFilter) {
			dom.activityFilter.value = state.activity;
		}
		ensurePageSizeOption();
		if (dom.pageSize) {
			dom.pageSize.value = String(state.pageSize);
		}
		updateSortToggle();
		updateDensityToggle();
	}

	function requestFilterUpdate({ resetPage = true, manual = true } = {}) {
		if (searchDebounce) {
			window.clearTimeout(searchDebounce);
			searchDebounce = null;
		}
		if (resetPage) {
			state.page = 1;
		}
		resetPaginationCachesIfNeeded();
		persistUiState();
		syncUrl();
		loadPage(state.page, { manual, bypassCache: true });
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
		dom.densityToggle.textContent = pressed
			? "Layout: Compact"
			: "Layout: Comfortable";
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
		if (
			nodeName === "input" ||
			nodeName === "textarea" ||
			target.isContentEditable
		) {
			return true;
		}
		return false;
	}

	function cycleLanguageFilter() {
		const options = Array.from(dom.languageFilter.options).map(
			(opt) => opt.value,
		);
		let idx = options.indexOf(state.language);
		if (idx === -1) {
			idx = 0;
		}
		idx = (idx + 1) % options.length;
		state.language = options[idx];
		dom.languageFilter.value = state.language;
		requestFilterUpdate();
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
				if (dom.presetModal && !dom.presetModal.hidden) {
					closePresetModal();
					event.preventDefault();
					return;
				}
				if (dom.shortcutModal && !dom.shortcutModal.hidden) {
					closeShortcutModal();
					event.preventDefault();
				}
				return;
			}

			if (isEditableTarget(target) && !event.altKey) {
				return;
			}

			if (event.altKey && !event.ctrlKey && !event.metaKey) {
				if (dom.presetModal && !dom.presetModal.hidden) {
					return;
				}
				if (key >= "1" && key <= "5") {
					event.preventDefault();
					applyPresetByShortcut(Number.parseInt(key, 10) - 1);
					return;
				}
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
				loadPage(state.page, { manual: true, bypassCache: true });
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
				triggerRefresh: () =>
					loadPage(state.page, { manual: true, bypassCache: true }),
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
		currentFilterSignature = computeFilterSignature();
		cacheSignature = currentFilterSignature;
		setupPresetUi();
		initControls();
		updateSyncStatus();
		renderQuickFilters();
		renderUserBanner();
		updateListLayout();

		syncTicker = window.setInterval(updateSyncStatus, 60 * 1000);
		registerTestHook();
		registerKeyboardShortcuts();
		fetchFilterOptions();
		loadPage(state.page, { initial: true, bypassCache: true });
	}

	initialise();
})();
