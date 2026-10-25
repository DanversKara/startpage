// ─────────────────────────────────────────────────────────────────────
// Wallpaper picker — menu-bar button, per-user choice (v1.19.0)
// ─────────────────────────────────────────────────────────────────────
// The panel lists every wallpaper as a thumbnail: the bundled
// default, the shared library (admin uploads), and — signed in —
// your own uploads. Other users' personal uploads are never listed,
// except the wallpaper currently on screen (which is reportable).
// Click a tile to apply it; upload to add one; delete your own;
// report anyone else's — visitors included, same as link reports.
let wpPanelEl = null;
let wpReportEl = null;
let wpItems = [];
let wpCurrent = null;
let wpRotation = { mode: 'off', seconds: 60, paths: [] };

function ensureWpPanel() {
	if (wpPanelEl) return wpPanelEl;
	const msg = el('p', { class: 'wp-msg' });
	const grid = el('div', { class: 'wp-grid' });
	const hint = el('p', { class: 'wp-hint' });
	const file = el('input', { type: 'file', accept: 'image/*', hidden: true });
	const uploadBtn = el('button', { class: 'report-btn primary', text: 'Upload wallpaper', onclick: () => file.click() });
	const uploadRow = el('div', { class: 'wp-upload' }, uploadBtn, file);
	const rotMode = el('select', {},
		el('option', { value: 'off', text: 'Rotation off' }),
		el('option', { value: 'timed', text: 'Rotate every…' }),
		el('option', { value: 'refresh', text: 'New one on every refresh' }));
	const rotSecs = el('input', { type: 'number', min: '10', value: '60' });
	const rotSecsLabel = el('span', { text: 'seconds' });
	const rotSummary = el('div', { class: 'wp-rot-note' });
	const rotBlock = el('div', { class: 'wp-rot' },
		el('div', { class: 'wp-rot-row' },
			el('span', { text: 'Rotation' }),
			rotMode,
			rotSecs,
			rotSecsLabel),
		rotSummary);
	const card = el('div', { class: 'wp-card' },
		el('div', { class: 'wp-head' },
			el('h3', { text: 'Wallpaper' }),
			el('button', { class: 'wp-close', text: '×', onclick: () => closeWallpaperPanel() })),
		hint,
		rotBlock,
		grid,
		uploadRow,
		msg);
	const pop = el('div', { class: 'wp-pop', hidden: true }, card);
	pop.addEventListener('pointerdown', e => { if (e.target === pop) closeWallpaperPanel(); });
	file.addEventListener('change', () => uploadWallpaper(file));
	rotMode.addEventListener('change', () => saveRotation(rotMode.value, parseInt(rotSecs.value, 10) || 60, wpRotation.paths));
	rotSecs.addEventListener('change', () => saveRotation(wpRotation.mode, parseInt(rotSecs.value, 10) || 60, wpRotation.paths));
	document.body.appendChild(pop);
	wpPanelEl = pop;
	wpPanelEl._grid = grid;
	wpPanelEl._hint = hint;
	wpPanelEl._msg = msg;
	wpPanelEl._uploadRow = uploadRow;
	wpPanelEl._rotBlock = rotBlock;
	wpPanelEl._rotMode = rotMode;
	wpPanelEl._rotSecs = rotSecs;
	wpPanelEl._rotSecsLabel = rotSecsLabel;
	wpPanelEl._rotSummary = rotSummary;
	return pop;
}

function rotFromState() {
	const r = S.rotation || {};
	return {
		mode: ['off', 'timed', 'refresh'].includes(r.mode) ? r.mode : 'off',
		seconds: Math.max(10, parseInt(r.seconds, 10) || 60),
		paths: Array.isArray(r.paths) ? r.paths.slice() : [],
	};
}

function syncRotBlock() {
	const pop = wpPanelEl;
	if (!pop) return;
	pop._rotMode.value = wpRotation.mode;
	pop._rotSecs.value = wpRotation.seconds;
	const timed = wpRotation.mode === 'timed';
	pop._rotSecs.style.display = timed ? '' : 'none';
	pop._rotSecsLabel.style.display = timed ? '' : 'none';
	pop._rotSummary.textContent = wpRotation.paths.length
		? wpRotation.paths.length + ' wallpaper' + (wpRotation.paths.length === 1 ? '' : 's') + ' in rotation' +
			(wpRotation.mode === 'timed' ? ' — changing every ' + wpRotation.seconds + ' seconds.' :
			 wpRotation.mode === 'refresh' ? ' — a new one on every refresh.' : ' — rotation is off.')
		: 'No wallpapers selected — tap ⟳ on a tile to add it to the rotation set.';
}

function closeWallpaperPanel() {
	if (wpPanelEl) wpPanelEl.hidden = true;
}

async function openWallpaperPanel(notice) {
	const pop = ensureWpPanel();
	pop._msg.textContent = notice || '';
	try {
		const r = await fetch('/api/me/wallpapers', { headers: { 'Accept': 'application/json' } });
		if (!r.ok) throw new Error('wallpapers ' + r.status);
		const d = await r.json();
		wpItems = d.items || [];
		wpCurrent = d.current || null;
	} catch (_) {
		wpItems = [];
		wpCurrent = null;
		pop._msg.textContent = 'Could not load wallpapers.';
	}
	const signedIn = !!S.csrf;
	pop._hint.textContent = signedIn
		? 'Click a wallpaper to apply it. Your choice is yours alone — visitors see the public desktop wallpaper.'
		: 'Sign in to choose or upload a wallpaper. You can still report one that should not be here.';
	pop._uploadRow.style.display = signedIn ? '' : 'none';
	// Rotation editing is signed-in only; visitors just see the tiles.
	wpRotation = rotFromState();
	pop._rotBlock.style.display = signedIn ? '' : 'none';
	syncRotBlock();
	renderWpGrid();
	pop.hidden = false;
}

function toggleWallpaperPanel() {
	if (wpPanelEl && !wpPanelEl.hidden) { closeWallpaperPanel(); return; }
	openWallpaperPanel();
}

function renderWpGrid() {
	const grid = wpPanelEl._grid;
	grid.innerHTML = '';
	wpItems.forEach(item => {
		const isCurrent = item.default ? !wpCurrent : item.path === wpCurrent;
		const tile = el('div', { class: 'wp-tile' + (isCurrent ? ' current' : '') });
		const thumb = el('button', { class: 'wp-thumb', onclick: () => chooseWallpaper(item) },
			el('img', { src: item.thumb, alt: item.name }));
		tile.appendChild(thumb);
		tile.appendChild(el('div', { class: 'wp-name' },
			el('span', { text: item.name }),
			isCurrent ? el('span', { class: 'wp-current', text: ' · Current' }) : null));
		const actions = el('div', { class: 'wp-tile-actions' });
		if (item.mine && !item.default) {
			actions.appendChild(el('button', { class: 'wp-mini danger', text: 'Delete', onclick: () => deleteWallpaper(item) }));
		}
		if (!item.mine && !item.default) {
			actions.appendChild(el('button', { class: 'wp-mini', text: '⚑ Report', onclick: () => openWpReport(item) }));
		}
		if (S.csrf && !item.default) {
			const inRot = wpRotation.paths.includes(item.path);
			actions.appendChild(el('button', { class: 'wp-mini' + (inRot ? ' on' : ''), text: inRot ? '✓ In rotation' : '⟳ Rotate', onclick: () => toggleRot(item) }));
		}
		if (actions.childNodes.length) tile.appendChild(actions);
		grid.appendChild(tile);
	});
	if (!wpItems.length) {
		grid.appendChild(el('p', { text: 'No wallpapers yet.' }));
	}
}

async function chooseWallpaper(item) {
	if (!S.csrf) {
		wpPanelEl._msg.textContent = 'Sign in to choose a wallpaper.';
		return;
	}
	try {
		const r = await fetch('/api/me/wallpapers/choose', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': S.csrf || '' },
			body: JSON.stringify({ path: item.path }),
		});
		if (!r.ok) throw new Error('choose ' + r.status);
		wpCurrent = item.path;
		applyWallpaper(item.path);
		S.wallpaper = item.path;
		let msg = item.default ? 'Back to the default wallpaper.' : 'Wallpaper applied.';
		if (wpRotation.mode !== 'off') {
			// A manual pick wins over rotation: stop it here and
			// persist off, so a reload doesn't restart it. The set
			// itself is kept, ready to switch back on.
			stopRotation();
			try {
				await fetch('/api/me/wallpapers/rotation', {
					method: 'POST',
					headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': S.csrf || '' },
					body: JSON.stringify({ mode: 'off', seconds: wpRotation.seconds, paths: wpRotation.paths }),
				});
			} catch (_) {}
			wpRotation.mode = 'off';
			S.rotation = wpRotation;
			syncRotBlock();
			msg = 'Rotation off — ' + (item.default ? 'back to the default wallpaper.' : 'wallpaper applied.');
		}
		wpPanelEl._msg.textContent = msg;
		renderWpGrid();
	} catch (e) {
		wpPanelEl._msg.textContent = 'Could not apply that wallpaper.';
	}
}

async function uploadWallpaper(fileInput) {
	const f = (fileInput.files || [])[0];
	fileInput.value = '';
	if (!f) return;
	if (!S.csrf) { wpPanelEl._msg.textContent = 'Sign in to upload a wallpaper.'; return; }
	if (f.size > 8 * 1024 * 1024) { wpPanelEl._msg.textContent = 'That file is over 8 MB — pick a smaller one.'; return; }
	try {
		const dataUrl = await new Promise((res, rej) => {
			const rd = new FileReader();
			rd.onload = () => res(rd.result);
			rd.onerror = () => rej(new Error('read failed'));
			rd.readAsDataURL(f);
		});
		const payload = String(dataUrl).split(',').pop();
		const r = await fetch('/api/me/wallpapers/upload', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': S.csrf || '' },
			body: JSON.stringify({ filename: f.name, data_base64: payload }),
		});
		if (!r.ok) {
			let detail = 'upload ' + r.status;
			try { const d = await r.json(); if (d.error) detail = d.error; } catch (_) {}
			throw new Error(detail);
		}
		await openWallpaperPanel('Uploaded — click it to apply.');
	} catch (e) {
		wpPanelEl._msg.textContent = 'Upload failed: ' + e.message;
	}
}

async function deleteWallpaper(item) {
	if (!confirm('Delete "' + item.name + '"?')) return;
	try {
		const r = await fetch('/api/me/wallpapers/delete', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': S.csrf || '' },
			body: JSON.stringify({ path: item.path }),
		});
		if (!r.ok) throw new Error('delete ' + r.status);
		if (wpCurrent === item.path) {
			wpCurrent = null;
			S.wallpaper = null;
			applyWallpaper(null);
		}
		await openWallpaperPanel('Wallpaper deleted.');
	} catch (_) {
		wpPanelEl._msg.textContent = 'Could not delete that wallpaper.';
	}
}

// ── Report a wallpaper ───────────────────────────────────────────
// Same endpoint and rate limit as link reports; the reporter is the
// signed-in username, or "visitor". The admin sees it in the panel's
// Reports card with a one-click removal.
function ensureWpReport() {
	if (wpReportEl) return wpReportEl;
	const msg = el('p', { class: 'report-msg' });
	const reason = el('textarea', { placeholder: 'What is wrong with this wallpaper?' });
	const send = el('button', { class: 'report-btn primary', text: 'Send report', onclick: () => submitWpReport(reason, msg, send) });
	const card = el('div', { class: 'report-card' },
		el('h3', { text: 'Report this wallpaper' }),
		el('p', {}, el('img', { src: '', alt: '', style: 'width:100%;height:120px;object-fit:cover;border-radius:10px' })),
		el('p', { class: 'wp-report-name' }),
		reason,
		el('div', { class: 'report-actions' },
			el('button', { class: 'report-btn', text: 'Cancel', onclick: () => closeWpReport() }),
			send),
		msg);
	const pop = el('div', { class: 'wp-pop top', hidden: true }, card);
	pop.addEventListener('pointerdown', e => { if (e.target === pop) closeWpReport(); });
	document.body.appendChild(pop);
	wpReportEl = pop;
	wpReportEl._reason = reason;
	wpReportEl._msg = msg;
	wpReportEl._send = send;
	return pop;
}

function openWpReport(item) {
	const pop = ensureWpReport();
	pop._item = item;
	pop.querySelector('img').src = item.thumb;
	pop.querySelector('.wp-report-name').textContent = '“' + item.name + '” — the admin sees your report in the panel.';
	pop._reason.value = '';
	pop._msg.textContent = '';
	pop._send.disabled = false;
	pop.hidden = false;
}

function closeWpReport() {
	if (wpReportEl) wpReportEl.hidden = true;
}

async function submitWpReport(reasonEl, msgEl, btn) {
	const item = wpReportEl._item;
	if (!item) return;
	btn.disabled = true;
	try {
		const headers = { 'Content-Type': 'application/json' };
		if (S.csrf) headers['X-CSRF-Token'] = S.csrf;
		const r = await fetch('/api/report', {
			method: 'POST',
			headers,
			body: JSON.stringify({ wallpaper: item.path, reason: reasonEl.value }),
		});
		if (!r.ok) throw new Error('report ' + r.status);
		msgEl.textContent = 'Report sent. Thank you.';
		btn.disabled = false;
		setTimeout(closeWpReport, 1200);
	} catch (_) {
		msgEl.textContent = 'Could not send that report — try again later.';
		btn.disabled = false;
	}
}

// ── Rotation ─────────────────────────────────────────────────────
async function toggleRot(item) {
	const adding = !wpRotation.paths.includes(item.path);
	let paths;
	if (adding) {
		if (wpRotation.paths.length >= 20) {
			wpPanelEl._msg.textContent = 'Rotation holds at most 20 wallpapers.';
			return;
		}
		paths = wpRotation.paths.concat([item.path]);
	} else {
		paths = wpRotation.paths.filter(p => p !== item.path);
	}
	// Adding the first tile switches rotation on (timed) — a set with
	// rotation off would otherwise do nothing at all.
	const mode = (adding && wpRotation.mode === 'off') ? 'timed' : wpRotation.mode;
	await saveRotation(mode, wpRotation.seconds, paths,
		adding ? 'Rotation on — every ' + wpRotation.seconds + ' seconds.' : 'Removed from rotation.');
}

async function saveRotation(mode, seconds, paths, notice) {
	try {
		const r = await fetch('/api/me/wallpapers/rotation', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': S.csrf || '' },
			body: JSON.stringify({ mode, seconds, paths }),
		});
		if (!r.ok) {
			let detail = 'rotation ' + r.status;
			try { const d = await r.json(); if (d.error) detail = d.error; } catch (_) {}
			throw new Error(detail);
		}
		const d = await r.json();
		wpRotation = {
			mode: d.rotation.mode,
			seconds: d.rotation.seconds,
			paths: d.rotation.paths.slice(),
		};
		S.rotation = wpRotation;
		if (wpRotation.mode === 'timed') startRotation();
		else stopRotation();
		syncRotBlock();
		renderWpGrid();
		wpPanelEl._msg.textContent = notice || 'Rotation saved.';
	} catch (e) {
		wpPanelEl._msg.textContent = 'Could not save rotation: ' + e.message;
	}
}

// ── Rotation engine ────────────────────────────────────────────
// Timed rotation ticks on the client; per-refresh picks a random set
// member on each load. Either way the stored wallpaper_path is left
// alone — rotation is a display override, and turning it off (or
// picking a wallpaper by hand) returns to the stored choice.
let wpTimer = null;
let wpRotIndex = -1;

function stopRotation() {
	if (wpTimer) { clearInterval(wpTimer); wpTimer = null; }
}

function startRotation() {
	stopRotation();
	const rot = S.rotation;
	if (!rot || rot.mode !== 'timed' || !rot.paths || !rot.paths.length) return;
	rot.paths.forEach(p => { const im = new Image(); im.src = p; });
	const secs = Math.max(10, rot.seconds || 60);
	wpTimer = setInterval(() => {
		const paths = (S.rotation && S.rotation.paths) || [];
		if (!paths.length) { stopRotation(); return; }
		wpRotIndex = (wpRotIndex + 1) % paths.length;
		applyWallpaper(paths[wpRotIndex]);
	}, secs * 1000);
}

function applyRotationAtBoot() {
	const rot = S.rotation;
	if (!rot || rot.mode === 'off' || !rot.paths || !rot.paths.length) return;
	if (rot.mode === 'refresh') {
		const paths = rot.paths;
		let pick = paths[Math.floor(Math.random() * paths.length)];
		if (paths.length > 1) {
			const cur = S.wallpaper;
			let guard = 0;
			while (pick === cur && guard++ < 10) pick = paths[Math.floor(Math.random() * paths.length)];
		}
		applyWallpaper(pick);
		wpRotIndex = paths.indexOf(pick);
	} else if (rot.mode === 'timed') {
		wpRotIndex = rot.paths.indexOf(S.wallpaper);
		startRotation();
	}
}

document.addEventListener('keydown', e => {
	if (e.key !== 'Escape') return;
	if (wpReportEl && !wpReportEl.hidden) { closeWpReport(); return; }
	if (wpPanelEl && !wpPanelEl.hidden) closeWallpaperPanel();
});
