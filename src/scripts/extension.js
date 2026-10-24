// ─────────────────────────────────────────────────────────────────────
// Extension boot shim — inert on the website.
// ─────────────────────────────────────────────────────────────────────
// When this page runs as the browser extension's new-tab page, the startpage
// server may be offline, so state cannot come from /api/state. Instead the
// shim swaps the state loader: it renders instantly from the last synced
// payload cached in chrome.storage.local (with the REAL URLs, so links open
// even with the server down), then refreshes from GET /api/sync with the
// stored pairing token and re-renders if anything changed.
//
// Pairing (server URL + sync token) lives in the extension options page.
// Tokens are per-user and issued from the admin panel or /account.
(function () {
	const isExtPage = location.protocol === 'chrome-extension:' || location.protocol === 'moz-extension:';
	if (!isExtPage) return;
	const api = globalThis.browser || globalThis.chrome;
	if (!api || !api.storage || !api.storage.local) return;

	window.__EXT__ = true;

	const storageGet = keys => new Promise(res => {
		try {
			const r = api.storage.local.get(keys, v => res(v || {}));
			if (r && r.then) r.then(v => res(v || {}), () => res({}));
		} catch (_) { res({}); }
	});
	const storageSet = obj => new Promise(res => {
		try {
			const r = api.storage.local.set(obj, () => res());
			if (r && r.then) r.then(() => res(), () => res());
		} catch (_) { res(); }
	});

	const emptyS = () => ({ desktopIcons: [], dock: [], bookmarkSections: [{ blocks: [] }], about: [], branding: null });

	// Build the render model from a /api/sync payload. Anchors use the real
	// cached URLs (offline clicking must work); the disguised admin link is
	// just a normal link here — its URL is the server origin + admin path.
	function buildS(payload, serverUrl) {
		const norm = x => {
			const it = {
				id: x.url + '|' + x.label,
				label: x.label,
				icon_bg: x.icon_bg || null,
				icon: x.icon && x.icon.startsWith('/') ? serverUrl + x.icon : (x.icon || 'src/icons/globe.svg'),
				href: x.admin ? serverUrl + x.url : x.url,
				col: x.col, row: x.row,
			};
			if (x.folder) {
				it.folder = true;
				it.members = (x.members || []).map(norm);
			}
			return it;
		};
		const groups = (arr) => (arr || []).map(g => ({ title: g.title, links: (g.links || []).map(norm) }));
		return {
			desktopIcons: (payload.desktop || []).map(norm),
			dock: (payload.dock || []).map(norm),
			bookmarkSections: [{ blocks: groups(payload.apps) }],
			about: groups(payload.about),
			branding: payload.branding || null,
		};
	}

	async function refresh(serverUrl, token, version) {
		try {
			const headers = { 'Authorization': 'Bearer ' + token };
			if (version) headers['If-None-Match'] = '"' + version + '"';
			const r = await fetch(serverUrl + '/api/sync', { headers });
			if (r.status === 304 || !r.ok) return null;
			const payload = await r.json();
			await storageSet({ sp_cache: payload });
			return payload;
		} catch (_) { return null; }
	}

	// Swap the loader installed by state.js (this script runs before app.js).
	loadState = async function () {
		const cfg = await storageGet(['sp_server', 'sp_token', 'sp_cache']);
		const serverUrl = String(cfg.sp_server || '').replace(/\/+$/, '');
		const token = cfg.sp_token || '';
		if (!serverUrl || !token) {
			window.__EXT_UNPAIRED = true;
			S = emptyS();
			ensureIconPositions();
			return;
		}
		if (cfg.sp_cache) {
			S = buildS(cfg.sp_cache, serverUrl);
			ensureIconPositions();
			// Render from cache now (app.js does), refresh behind it and
			// re-render only if the server state actually changed.
			refresh(serverUrl, token, cfg.sp_cache.version).then(payload => {
				if (payload && payload.version !== cfg.sp_cache.version) {
					S = buildS(payload, serverUrl);
					ensureIconPositions();
					renderDesktopIcons(); renderDock(); renderAbout();
				}
			});
		} else {
			const payload = await refresh(serverUrl, token, null);
			S = payload ? buildS(payload, serverUrl) : emptyS();
			ensureIconPositions();
		}
	};

	document.addEventListener('DOMContentLoaded', () => {
		// The About-card account line points at server pages; hide it here.
		const acct = document.getElementById('aboutAccount');
		if (acct) acct.style.display = 'none';

		// Unpaired: no fake sample links — a short notice + options button.
		const check = () => {
			if (!window.__EXT_UNPAIRED || document.querySelector('.ext-pair-notice')) return;
			const wrap = document.createElement('div');
			wrap.className = 'ext-pair-notice';
			wrap.innerHTML = '<div class="ext-pair-card"><h2>Pair this extension</h2>' +
				'<p>This new-tab page shows your startpage links, cached for offline use. ' +
				'Create a sync token in the admin panel (or your account page), then enter it ' +
				'once in the extension options.</p>' +
				'<button type="button">Open extension options</button></div>';
			wrap.querySelector('button').addEventListener('click', () => {
				if (api.runtime && api.runtime.openOptionsPage) api.runtime.openOptionsPage();
			});
			document.body.appendChild(wrap);
		};
		setTimeout(check, 1200);
		setTimeout(check, 3000);
	});
})();
