// Startpage extension — options page: pair with a server + sync token.
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
function storageRemove(keys) {
	return new Promise(res => {
		try {
			const r = api.storage.local.remove(keys, () => res());
			if (r && r.then) r.then(() => res(), () => res());
		} catch (_) { res(); }
	});
}
function requestHostPermission(originPattern) {
	return new Promise(res => {
		try {
			const r = api.permissions.request({ origins: [originPattern] }, ok => res(!!ok));
			if (r && r.then) r.then(() => res(true), () => res(false));
		} catch (_) { res(false); }
	});
}

const $ = id => document.getElementById(id);
const status = (msg, cls) => { const s = $('status'); s.textContent = msg; s.className = cls || ''; };

(async () => {
	const cfg = await storageGet(['sp_server', 'sp_token']);
	if (cfg.sp_server) $('server').value = cfg.sp_server;
	if (cfg.sp_token) $('token').value = cfg.sp_token;
	if (cfg.sp_server && cfg.sp_token) status('Currently paired with ' + cfg.sp_server, 'ok');
})();

$('save').addEventListener('click', async () => {
	const serverUrl = $('server').value.trim().replace(/\/+$/, '') || 'https://start.dox.box';
	const token = $('token').value.trim();
	if (!token) { status('Paste a sync token first.', 'err'); return; }
	let origin;
	try { origin = new URL(serverUrl).origin; } catch (_) { status('That server URL doesn’t parse.', 'err'); return; }
	status('Requesting permission for ' + origin + ' …');
	const granted = await requestHostPermission(origin + '/*');
	if (!granted) { status('Host permission was declined — the extension can’t reach the server without it.', 'err'); return; }
	status('Testing sync …');
	try {
		const r = await fetch(serverUrl + '/api/sync', { headers: { 'Authorization': 'Bearer ' + token } });
		if (r.status === 401) { status('The server rejected that token (401). Create a fresh one and try again.', 'err'); return; }
		if (!r.ok) { status('Sync failed: HTTP ' + r.status, 'err'); return; }
		const payload = await r.json();
		await storageSet({ sp_server: serverUrl, sp_token: token, sp_cache: payload });
		const n = (payload.desktop || []).length + (payload.dock || []).length +
			(payload.apps || []).reduce((a, g) => a + (g.links || []).length, 0);
		status('Paired ✓ — ' + n + ' links cached from ' + serverUrl + '. New tabs will show them, online or off.', 'ok');
	} catch (e) {
		status('Couldn’t reach ' + serverUrl + ' — check the URL and that the server is up. (' + e.message + ')', 'err');
	}
});

$('clear').addEventListener('click', async () => {
	await storageRemove(['sp_server', 'sp_token', 'sp_cache']);
	$('server').value = ''; $('token').value = '';
	status('Pairing cleared. New tabs will show the pairing notice again.');
});
