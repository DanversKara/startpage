// Startpage extension — background sync worker.
// Keeps the cached /api/sync payload warm so a new tab renders the owner's
// links instantly (and works at all while the server is unreachable).
// Refreshes on install, on browser startup, and every 30 minutes.
const api = globalThis.browser || globalThis.chrome;

function storageGet(keys) {
	return new Promise(res => {
		try {
			const r = api.storage.local.get(keys, v => res(v || {}));
			if (r && r.then) r.then(v => res(v || {}), () => res({}));
		} catch (_) { res({}); }
	});
}
function storageSet(obj) {
	return new Promise(res => {
		try {
			const r = api.storage.local.set(obj, () => res());
			if (r && r.then) r.then(() => res(), () => res());
		} catch (_) { res(); }
	});
}

async function refreshSync() {
	const cfg = await storageGet(['sp_server', 'sp_token', 'sp_cache']);
	const serverUrl = String(cfg.sp_server || '').replace(/\/+$/, '');
	const token = cfg.sp_token || '';
	if (!serverUrl || !token) return;
	try {
		const headers = { 'Authorization': 'Bearer ' + token };
		if (cfg.sp_cache && cfg.sp_cache.version) headers['If-None-Match'] = '"' + cfg.sp_cache.version + '"';
		const r = await fetch(serverUrl + '/api/sync', { headers });
		if (r.ok) {
			const payload = await r.json();
			await storageSet({ sp_cache: payload });
		}
	} catch (_) { /* server offline — the cache stays as it was */ }
}

if (api.runtime && api.runtime.onInstalled) api.runtime.onInstalled.addListener(refreshSync);
if (api.runtime && api.runtime.onStartup) api.runtime.onStartup.addListener(refreshSync);
if (api.alarms) {
	api.alarms.create('sp-sync', { periodInMinutes: 30 });
	api.alarms.onAlarm.addListener(a => { if (a.name === 'sp-sync') refreshSync(); });
}
refreshSync();
