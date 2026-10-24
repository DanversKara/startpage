// ─────────────────────────────────────────────────────────────────────
// Desktop + dock rendering
// ─────────────────────────────────────────────────────────────────────
// Icons render where the server model places them and every anchor points
// at /go/<id>. The page-side edits, all gated on the owner's session:
// rearranging the desktop grid (Edit Desktop — drag to move, drop on an
// occupied cell to swap), folders (hold a drag over an icon to bundle
// the two into a folder, drop on a folder to join it, drag members out of
// an open folder to ungroup them), and full link CRUD (tap an icon in
// edit mode — or the ＋ Add tile — to add/edit/delete links and upload
// icons, via the /api/me/links endpoints). The server enforces session +
// CSRF + ownership on every one of those calls.

// ─────────────────────────────────────────────────────────────────────
// Desktop icon element factory
// ─────────────────────────────────────────────────────────────────────
// Folders render as a mosaic of their first four members and open the
// folder popover instead of navigating.
function folderPreview(item) {
	const prev = el('div', { class: 'folder-preview' });
	const members = (item.members || []).slice(0, 4);
	if (!members.length) {
		prev.appendChild(iconArtFromSrc('src/icons/globe.svg'));
	} else {
		members.forEach(m => prev.appendChild(iconArt(m)));
	}
	return prev;
}

function makeIconEl(item) {
	if (item.folder) {
		const tile = el('div', { class: 'desktop-icon folder-tile', data: { key: item.id }, role: 'button', tabindex: '0' },
			folderPreview(item),
			el('span', { class: 'desktop-icon-label', text: item.label }),
		);
		tile.addEventListener('click', () => { if (!desktopEditing) openFolder(item.id); });
		return tile;
	}
	return el('a', { class: 'desktop-icon', href: item.href, target: '_blank', rel: 'noopener noreferrer', data: { key: item.id } },
		el('div', { class: 'icon-img-wrap' }, iconArt(item)),
		el('span', { class: 'desktop-icon-label', text: item.label }),
	);
}

// ─────────────────────────────────────────────────────────────────────
// Render: desktop icons
// ─────────────────────────────────────────────────────────────────────
function renderDesktopIcons() {
	const grid = $('desktopGrid');
	grid.innerHTML = '';
	S.desktopIcons.forEach(item => {
		const a = makeIconEl(item);
		Object.assign(a.style, {
			position: 'absolute',
			left: (GRID.ox + item.col * GRID.uw) + 'px',
			top:  (GRID.oy + item.row * GRID.uh) + 'px',
		});
		grid.appendChild(a);
	});
	// In edit mode the owner gets a ＋ Add tile in the first free cell.
	if (desktopEditing && S.csrf) {
		const cell = nextFreeCell(occupiedCells());
		const add = el('div', { class: 'desktop-icon add-tile', role: 'button', tabindex: '0', title: 'Add a link' },
			el('div', { class: 'add-plus', text: '＋' }),
			el('span', { class: 'desktop-icon-label', text: 'Add link' }),
		);
		Object.assign(add.style, {
			position: 'absolute',
			left: (GRID.ox + cell.col * GRID.uw) + 'px',
			top: (GRID.oy + cell.row * GRID.uh) + 'px',
		});
		add.addEventListener('pointerup', e => { e.stopPropagation(); openLinkSheet(null, 'desktop'); });
		grid.appendChild(add);
	}
}

// ─────────────────────────────────────────────────────────────────────
// Folder popover — tap a folder tile to open it. Members are ordinary
// links. While signed in, the title renames the folder; in Edit Desktop
// mode members can be dragged out onto the grid and the folder removed.
// ─────────────────────────────────────────────────────────────────────
let folderPopEl = null;
let openFolderId = null;
let folderManaging = false;

function refreshDesktop() {
	loadState().then(() => renderDesktopIcons());
}

function ensureFolderPop() {
	if (folderPopEl) return folderPopEl;
	const nameInput = el('input', { class: 'folder-name-input', maxlength: 80 });
	const nameSpan = el('span', { class: 'folder-name' });
	const actions = el('div', { class: 'folder-actions' },
		el('button', { class: 'folder-remove', text: 'Remove folder (members go back to the desktop)', onclick: () => removeOpenFolder() }),
	);
	const manageBtn = el('button', { class: 'folder-manage-btn', text: 'Add apps', onclick: () => {
		folderManaging = !folderManaging;
		if (openFolderId) openFolder(openFolderId, true);
	} });
	const manage = el('div', { class: 'folder-manage', hidden: true },
		el('div', { class: 'folder-manage-list' }),
	);
	const card = el('div', { class: 'folder-card' },
		el('div', { class: 'folder-head' },
			nameSpan, nameInput, manageBtn,
			el('button', { class: 'folder-close', text: '✕', title: 'Close', onclick: () => closeFolder() }),
		),
		el('div', { class: 'folder-grid' }),
		manage,
		actions,
	);
	const pop = el('div', { class: 'folder-pop', hidden: true }, card);
	pop.addEventListener('pointerdown', e => { if (e.target === pop) closeFolder(); });
	// In edit mode member tiles must not navigate — but the Add apps
	// checklist inside the same popover must stay clickable.
	pop.addEventListener('click', e => {
		if (desktopEditing && !e.target.closest('.folder-manage')) { e.preventDefault(); e.stopPropagation(); }
	}, true);
	document.body.appendChild(pop);
	folderPopEl = pop;
	nameInput.addEventListener('change', () => renameOpenFolder(nameInput.value));
	nameInput.addEventListener('keydown', e => { if (e.key === 'Enter') nameInput.blur(); e.stopPropagation(); });
	return pop;
}

function openFolder(id, keepManage) {
	const item = S.desktopIcons.find(i => i.id === id && i.folder);
	if (!item) return;
	openFolderId = id;
	if (!keepManage) folderManaging = false;
	const pop = ensureFolderPop();
	const signedIn = !!S.csrf;
	const nameInput = pop.querySelector('.folder-name-input');
	const nameSpan = pop.querySelector('.folder-name');
	nameInput.hidden = !signedIn;
	nameSpan.hidden = signedIn;
	nameInput.value = item.label;
	nameSpan.textContent = item.label;
	pop.querySelector('.folder-actions').style.display = (signedIn && desktopEditing) ? '' : 'none';
	const manageBtn = pop.querySelector('.folder-manage-btn');
	manageBtn.style.display = signedIn ? '' : 'none';
	manageBtn.textContent = folderManaging ? 'Done' : 'Add apps';
	const manage = pop.querySelector('.folder-manage');
	manage.hidden = !folderManaging;
	const grid = pop.querySelector('.folder-grid');
	grid.style.display = folderManaging ? 'none' : '';
	grid.innerHTML = '';
	(item.members || []).forEach(m => {
		const tile = el('a', { class: 'folder-member', href: m.href, target: '_blank', rel: 'noopener noreferrer', data: { key: m.id } },
			iconArt(m),
			el('span', { class: 'folder-member-label', text: m.label }),
		);
		if (desktopEditing && signedIn) {
			tile.addEventListener('pointerdown', e => startMemberDrag(e, item, m, tile));
		}
		grid.appendChild(tile);
	});
	if (folderManaging) renderFolderManage(item);
	pop.hidden = false;
}

// The no-drag way to build a folder: tick the apps that belong in it.
// Members are checked; every other desktop app is a tick away. Changes
// apply immediately.
function renderFolderManage(folder) {
	const list = folderPopEl.querySelector('.folder-manage-list');
	list.innerHTML = '';
	const rows = [];
	(folder.members || []).forEach(m => rows.push({ item: m, inside: true }));
	S.desktopIcons.forEach(i => { if (!i.folder && i.id !== folder.id) rows.push({ item: i, inside: false }); });
	if (!rows.length) {
		list.appendChild(el('div', { class: 'sub', text: 'No desktop apps to add yet.' }));
		return;
	}
	rows.forEach(({ item, inside }) => {
		const cb = el('input', { type: 'checkbox' });
		cb.checked = inside;
		cb.addEventListener('change', () => toggleFolderMember(folder, item, cb.checked));
		list.appendChild(el('label', { class: 'folder-manage-row' }, cb, iconArt(item), el('span', { text: item.label })));
	});
}

async function toggleFolderMember(folder, item, join) {
	try {
		if (join) {
			await postMove({ id: item.id, folder_id: folder.id });
		} else {
			const free = nextFreeCell(occupiedCells());
			await postMove({ id: item.id, col: free.col, row: free.row });
		}
	} catch (_) { /* the refresh below shows the server's truth */ }
	afterFolderChange();
}

// Reload state after a membership change, keeping the folder open in
// manage mode — unless the folder dissolved (it emptied to one member),
// in which case there is nothing left to show.
async function afterFolderChange() {
	await loadState();
	renderDesktopIcons();
	if (!openFolderId) return;
	if (!S.desktopIcons.some(i => i.id === openFolderId && i.folder)) { closeFolder(); return; }
	openFolder(openFolderId, true);
}

function closeFolder() {
	openFolderId = null;
	if (folderPopEl) folderPopEl.hidden = true;
}

async function renameOpenFolder(label) {
	const item = S.desktopIcons.find(i => i.id === openFolderId && i.folder);
	label = (label || '').trim();
	if (!item || !label || label === item.label) return;
	try {
		const r = await fetch('/api/links/folder-rename', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': S.csrf || '' },
			body: JSON.stringify({ id: item.id, label }),
		});
		if (!r.ok) throw new Error('rename rejected');
		item.label = label;
		renderDesktopIcons();
	} catch (_) { /* keep the old name on screen too */
		if (folderPopEl) folderPopEl.querySelector('.folder-name-input').value = item.label;
	}
}

async function removeOpenFolder() {
	const item = S.desktopIcons.find(i => i.id === openFolderId && i.folder);
	if (!item) return;
	try {
		const r = await fetch('/api/links/folder-delete', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': S.csrf || '' },
			body: JSON.stringify({ id: item.id }),
		});
		if (!r.ok) throw new Error('delete rejected');
	} catch (_) { /* fall through to the refresh either way */ }
	closeFolder();
	refreshDesktop();
}

// Drag a member out of the open folder onto the desktop grid. Dropping
// anywhere outside the folder card moves it to that grid cell (the server
// leaves the folder, dissolving it if it drops to one member).
let memberDrag = null;

function startMemberDrag(e, folder, member, tile) {
	if (e.button > 0) return;
	e.preventDefault();
	const rect = tile.getBoundingClientRect();
	memberDrag = {
		folder, member,
		clone: makeDragClone(tile, rect),
		grabDX: e.clientX - rect.left, grabDY: e.clientY - rect.top,
		startX: e.clientX, startY: e.clientY,
	};
}

window.addEventListener('pointermove', e => {
	if (!memberDrag) return;
	memberDrag.clone.style.left = (e.clientX - memberDrag.grabDX) + 'px';
	memberDrag.clone.style.top = (e.clientY - memberDrag.grabDY) + 'px';
});

window.addEventListener('pointerup', async e => {
	if (!memberDrag) return;
	const d = memberDrag; memberDrag = null;
	d.clone.remove();
	// A tap on a member in edit mode opens its link editor.
	const card = folderPopEl && folderPopEl.querySelector('.folder-card');
	if (desktopEditing && Math.hypot(e.clientX - d.startX, e.clientY - d.startY) < 6 && card && card.contains(e.target)) {
		openLinkSheet(d.member.id);
		return;
	}
	// Dropped back on the folder card = put it back (do nothing).
	if (card && card.contains(e.target)) return;
	const gr = $('desktopGrid').getBoundingClientRect();
	const cell = dragCellFor(e.clientX - d.grabDX - gr.left, e.clientY - d.grabDY - gr.top);
	try {
		const r = await fetch('/api/links/move', {
			method: 'POST',
			headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': S.csrf || '' },
			body: JSON.stringify({ id: d.member.id, col: cell.col, row: cell.row }),
		});
		if (!r.ok) throw new Error('move rejected');
	} catch (_) { /* refresh shows the truth either way */ }
	closeFolder();
	refreshDesktop();
});

// ─────────────────────────────────────────────────────────────────────
// Render: dock
// ─────────────────────────────────────────────────────────────────────
// All Apps launcher uses the site's own bundled icon (a 3×3 grid),
// rendered through the same tile pipeline as every other icon.
const ALL_APPS_ICON = 'src/icons/all-apps.svg';

function renderDock() {
	const dock = $('dock');
	dock.innerHTML = '';

	dock.appendChild(el('button', { class: 'dock-all-apps', title: 'All Apps', onclick: openAllBookmarks }, iconArtFromSrc(ALL_APPS_ICON)));
	dock.appendChild(el('div', { class: 'dock-sep' }));

	S.dock.forEach(item => {
		const a = el('a', { class: 'dock-icon', href: item.href, target: '_blank', rel: 'noopener noreferrer', title: item.label, data: { id: item.id } });
		a.appendChild(iconArt(item));
		dock.appendChild(a);
	});
}

// ─────────────────────────────────────────────────────────────────────
// Dock magnification (macOS-style gaussian proximity scaling)
// ─────────────────────────────────────────────────────────────────────
function setupDockMagnification() {
	const dock   = $('dock');
	const MAX    = 1.48;
	const SIGMA  = 58;
	const LIFT   = 22;
	const HALF   = 34;

	function icons() {
		return [...dock.querySelectorAll('.dock-icon, .dock-all-apps')];
	}

	function captureNaturalPositions() {
		resetMag();
		icons().forEach(icon => {
			const r = icon.getBoundingClientRect();
			icon.dataset.naturalCx = (r.left + r.width / 2).toFixed(1);
		});
	}

	function applyMag(mouseX) {
		icons().forEach(icon => {
			const cx = parseFloat(icon.dataset.naturalCx || '0');
			const d  = mouseX - cx;
			const s  = 1 + (MAX - 1) * Math.exp(-(d * d) / (2 * SIGMA * SIGMA));
			const fs = icon.dataset.pressed ? s * 0.88 : s;
			const em = (HALF * (s - 1)).toFixed(1) + 'px';
			icon.style.transform   = `scale(${fs.toFixed(3)}) translateY(${(-(s - 1) * LIFT).toFixed(1)}px)`;
			icon.style.marginLeft  = em;
			icon.style.marginRight = em;
			icon.style.zIndex      = s > 1.15 ? '10' : '';
		});
	}

	function resetMag() {
		icons().forEach(icon => {
			icon.style.willChange  = '';
			icon.style.transform   = '';
			icon.style.marginLeft  = '';
			icon.style.marginRight = '';
			icon.style.zIndex      = '';
		});
	}

	let magRafId  = null;
	let lastMouseX = 0;

	dock.addEventListener('pointerenter', () => {
		icons().forEach(icon => { icon.style.willChange = 'transform, margin'; });
		captureNaturalPositions();
	});
	dock.addEventListener('pointermove', e => {
		lastMouseX = e.clientX;
		if (!magRafId) magRafId = requestAnimationFrame(() => { applyMag(lastMouseX); magRafId = null; });
	});
	dock.addEventListener('pointerleave', () => { if (magRafId) { cancelAnimationFrame(magRafId); magRafId = null; } resetMag(); });

	dock.addEventListener('pointerdown', e => {
		const icon = e.target.closest('.dock-icon, .dock-all-apps');
		if (icon) { icon.dataset.pressed = '1'; applyMag(e.clientX); }
	});
	['pointerup', 'pointercancel'].forEach(ev => {
		dock.addEventListener(ev, e => {
			const icon = e.target.closest('.dock-icon, .dock-all-apps');
			if (icon) { delete icon.dataset.pressed; applyMag(e.clientX); }
		});
	});
}

// ─────────────────────────────────────────────────────────────────────
// Edit mode — drag desktop icons to rearrange (signed-in owner only)
// ─────────────────────────────────────────────────────────────────────
let desktopEditing = false;
let dragState = null;

function toggleDesktopEdit(force) {
	desktopEditing = typeof force === 'boolean' ? force : !desktopEditing;
	$('desktop').classList.toggle('editing', desktopEditing);
	$('mbEdit').textContent = desktopEditing ? 'Done' : 'Edit Desktop';
	if (!desktopEditing && dragState) endDrag(false);
	if (!desktopEditing) closeLinkSheet();
	renderDesktopIcons();
}

function dragCellFor(left, top) {
	return {
		col: Math.max(0, Math.round((left - GRID.ox) / GRID.uw)),
		row: Math.max(0, Math.round((top - GRID.oy) / GRID.uh)),
	};
}

function setupDesktopEditing() {
	const grid = $('desktopGrid');
	// While editing, icons rearrange — they must not navigate.
	grid.addEventListener('click', e => {
		if (desktopEditing) { e.preventDefault(); e.stopPropagation(); }
	}, true);
	grid.addEventListener('pointerdown', e => {
		if (!desktopEditing || e.button > 0) return;
		const iconEl = e.target.closest('.desktop-icon');
		if (!iconEl) return;
		const item = S.desktopIcons.find(i => i.id === iconEl.dataset.key);
		if (!item) return;
		e.preventDefault();
		const gr = grid.getBoundingClientRect();
		dragState = {
			el: iconEl, item,
			startCol: item.col, startRow: item.row,
			grabDX: e.clientX - gr.left - iconEl.offsetLeft,
			grabDY: e.clientY - gr.top - iconEl.offsetTop,
			moved: false, ghost: null, cell: null,
			folderIntent: false, hoverCell: null, hoverSince: 0, intentEl: null, intentTimer: null,
		};
	});
	window.addEventListener('pointermove', e => {
		if (!dragState) return;
		const gr = grid.getBoundingClientRect();
		const left = e.clientX - gr.left - dragState.grabDX;
		const top = e.clientY - gr.top - dragState.grabDY;
		if (!dragState.moved) {
			const homeX = GRID.ox + dragState.startCol * GRID.uw;
			const homeY = GRID.oy + dragState.startRow * GRID.uh;
			if (Math.hypot(left - homeX, top - homeY) < 5) return;
			dragState.moved = true;
			dragState.el.classList.add('dragging');
			dragState.ghost = el('div', { class: 'desktop-icon-ghost' });
			grid.appendChild(dragState.ghost);
		}
		dragState.el.style.left = left + 'px';
		dragState.el.style.top = top + 'px';
		const cell = dragCellFor(left, top);
		dragState.cell = cell;
		dragState.ghost.style.left = (GRID.ox + cell.col * GRID.uw) + 'px';
		dragState.ghost.style.top = (GRID.oy + cell.row * GRID.uh) + 'px';
		updateDragIntent(cell);
	});
	window.addEventListener('pointerup', e => { if (dragState) endDrag(true, e.clientX, e.clientY); });
	document.addEventListener('keydown', e => {
		if (e.key !== 'Escape') return;
		if (openFolderId) { closeFolder(); return; }
		if (desktopEditing) toggleDesktopEdit(false);
	});
}

// Folder gestures while dragging: dropping on a folder tile joins it
// (instant). Hovering over an ICON for ~600 ms arms "make a folder" —
// the ghost turns amber and the target icon is outlined; a quick drop
// just swaps, like before.
function clearDragIntent() {
	const d = dragState;
	if (!d) return;
	if (d.intentEl) { d.intentEl.classList.remove('folder-target'); d.intentEl = null; }
	if (d.intentTimer) { clearTimeout(d.intentTimer); d.intentTimer = null; }
	d.folderIntent = false;
	d.hoverSince = 0;
	if (d.ghost) d.ghost.classList.remove('folder-intent', 'join-intent');
}

function updateDragIntent(cell) {
	const d = dragState;
	const key = cell.col + ',' + cell.row;
	const occupant = S.desktopIcons.find(i => i !== d.item && i.col === cell.col && i.row === cell.row);
	if (d.item.folder || !occupant) {
		clearDragIntent();
		d.hoverCell = key;
		return;
	}
	if (occupant.folder) {
		clearDragIntent();
		d.hoverCell = key;
		if (d.ghost) d.ghost.classList.add('join-intent');
		return;
	}
	if (d.hoverCell !== key) {
		clearDragIntent();
		d.hoverCell = key;
		d.hoverSince = Date.now();
		const occEl = $('desktopGrid').querySelector('.desktop-icon[data-key="' + occupant.id + '"]');
		if (occEl) { occEl.classList.add('folder-target'); d.intentEl = occEl; }
		// Arm on a real timer: a perfectly still hold fires no pointermove
		// events, so movement-based checks never see the 600 ms pass.
		d.intentTimer = setTimeout(() => {
			if (dragState === d && d.hoverCell === key && !d.folderIntent) {
				d.folderIntent = true;
				if (d.ghost) d.ghost.classList.add('folder-intent');
			}
		}, 600);
		return;
	}
}

async function postMove(body) {
	const r = await fetch('/api/links/move', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': S.csrf || '' },
		body: JSON.stringify(body),
	});
	if (!r.ok) throw new Error('move rejected');
}

// What's actually under the pointer at drop time (with the dragged icon
// and ghost momentarily hidden)? The user's aim is the ground truth —
// the dragged icon's corner can compute to a neighboring cell.
function probeDropTarget(d, px, py) {
	d.el.style.visibility = 'hidden';
	if (d.ghost) d.ghost.style.visibility = 'hidden';
	const hit = document.elementFromPoint(px, py);
	if (d.ghost) d.ghost.style.visibility = '';
	d.el.style.visibility = '';
	const tile = hit && hit.closest ? hit.closest('.desktop-icon') : null;
	if (!tile || tile === d.el) return null;
	return S.desktopIcons.find(i => i.id === tile.dataset.key && i !== d.item) || null;
}

async function endDrag(commit, px, py) {
	const d = dragState; dragState = null;
	if (!d) return;
	if (d.intentTimer) clearTimeout(d.intentTimer);
	if (d.ghost) d.ghost.remove();
	if (d.intentEl) d.intentEl.classList.remove('folder-target');
	d.el.classList.remove('dragging');
	if (!commit || !d.moved || !d.cell) {
		renderDesktopIcons();
		// A tap (press without a drag) in edit mode opens the link editor.
		if (commit && !d.moved && desktopEditing) openLinkSheet(d.item.id);
		return;
	}
	const { col, row } = d.cell;
	if (col === d.startCol && row === d.startRow) { renderDesktopIcons(); return; }
	let occupant = S.desktopIcons.find(i => i !== d.item && i.col === col && i.row === row);
	if (px != null && !d.item.folder) {
		const probed = probeDropTarget(d, px, py);
		if (probed && probed.folder) {
			try { await postMove({ id: d.item.id, folder_id: probed.id }); } catch (_) {}
			refreshDesktop();
			return;
		}
		if (probed && !occupant) occupant = probed;
	}
	// Drop on a folder tile: join the folder.
	if (occupant && occupant.folder && !d.item.folder) {
		try { await postMove({ id: d.item.id, folder_id: occupant.id }); } catch (_) {}
		refreshDesktop();
		return;
	}
	// Held over an icon: bundle the two into a new folder at that cell.
	if (occupant && d.folderIntent && !d.item.folder) {
		try {
			const r = await fetch('/api/links/folder', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': S.csrf || '' },
				body: JSON.stringify({ a: d.item.id, b: occupant.id, col, row }),
			});
			if (!r.ok) throw new Error('folder rejected');
		} catch (_) {}
		refreshDesktop();
		return;
	}
	try {
		await postMove({
			id: d.item.id, col, row,
			swap_with: occupant ? occupant.id : null,
			swap_col: d.startCol, swap_row: d.startRow,
		});
		d.item.col = col; d.item.row = row;
		if (occupant) { occupant.col = d.startCol; occupant.row = d.startRow; }
	} catch (_) { /* leave the model as-is; the re-render snaps back */ }
	renderDesktopIcons();
}

// ─────────────────────────────────────────────────────────────────────
// Desktop icon press feedback
// ─────────────────────────────────────────────────────────────────────
function setupDesktopIconPress() {
	const grid = $('desktopGrid');
	grid.addEventListener('pointerdown', e => {
		const icon = e.target.closest('.desktop-icon');
		if (icon) icon.classList.add('pressed');
	});
	['pointerup', 'pointercancel', 'pointerleave'].forEach(ev => {
		grid.addEventListener(ev, e => {
			const icon = e.target.closest('.desktop-icon');
			if (icon) icon.classList.remove('pressed');
		});
	});
}

// ─────────────────────────────────────────────────────────────────────
// Report a link / app / folder — right-click (or press-and-hold) any
// tile, dock icon, folder member or All Apps entry and tell the admin
// what's wrong. Reports land in the admin panel's Reports card.
// ─────────────────────────────────────────────────────────────────────
let reportPopEl = null;
let reportItem = null;
let ctxMenuEl = null;

function findLinkItem(id) {
	for (const it of S.desktopIcons) {
		if (it.id === id) return it;
		for (const m of (it.members || [])) if (m.id === id) return m;
	}
	for (const it of S.dock) if (it.id === id) return it;
	for (const sec of (S.bookmarkSections || [])) {
		for (const b of (sec.blocks || [])) {
			for (const l of (b.links || [])) if (l.id === id) return l;
		}
	}
	return null;
}

function ensureReportPop() {
	if (reportPopEl) return reportPopEl;
	const title = el('h3', {});
	const sub = el('p', {});
	const reason = el('textarea', { maxlength: 500, placeholder: 'What is wrong with it? (optional) — broken link, wrong site, outdated…' });
	const msg = el('p', { class: 'report-msg' });
	const send = el('button', { class: 'report-btn primary', text: 'Send report', onclick: () => submitReport(reason, msg, send) });
	const card = el('div', { class: 'report-card' },
		title, sub, reason,
		el('div', { class: 'report-actions' },
			el('button', { class: 'report-btn', text: 'Cancel', onclick: () => closeReport() }),
			send,
		),
		msg,
	);
	const pop = el('div', { class: 'report-pop', hidden: true }, card);
	pop.addEventListener('pointerdown', e => { if (e.target === pop) closeReport(); });
	document.body.appendChild(pop);
	pop._title = title; pop._sub = sub; pop._reason = reason; pop._msg = msg;
	reportPopEl = pop;
	return pop;
}

function openReport(id) {
	const item = findLinkItem(id);
	if (!item) return;
	reportItem = item;
	hideCtxMenu();
	const pop = ensureReportPop();
	pop._title.textContent = item.folder ? 'Report this folder' : 'Report this link';
	pop._sub.textContent = '“' + item.label + '” — the admin sees your report in the panel.';
	pop._reason.value = '';
	pop._msg.textContent = '';
	pop.hidden = false;
	setTimeout(() => pop._reason.focus(), 30);
}

function closeReport() {
	if (reportPopEl) reportPopEl.hidden = true;
	reportItem = null;
}

async function submitReport(reasonEl, msgEl, btn) {
	if (!reportItem) return;
	btn.disabled = true;
	try {
		const headers = { 'Content-Type': 'application/json' };
		if (S.csrf) headers['X-CSRF-Token'] = S.csrf;
		const r = await fetch('/api/report', {
			method: 'POST', headers,
			body: JSON.stringify({ link_id: reportItem.id, reason: reasonEl.value }),
		});
		if (!r.ok) { const d = await r.json().catch(() => ({})); throw new Error(d.error || ('failed (' + r.status + ')')); }
		msgEl.textContent = '✓ Sent — thanks. The admin will take a look.';
		setTimeout(closeReport, 1100);
	} catch (e) {
		msgEl.textContent = 'Could not send: ' + e.message;
	} finally {
		btn.disabled = false;
	}
}

function hideCtxMenu() { if (ctxMenuEl) { ctxMenuEl.remove(); ctxMenuEl = null; } }

document.addEventListener('contextmenu', e => {
	if (desktopEditing) return;
	const t = e.target.closest('.desktop-icon, .dock-icon, .folder-member, .miller-link');
	if (!t) return;
	const id = t.dataset.key || t.dataset.id;
	const item = id && findLinkItem(id);
	if (!item) return;
	e.preventDefault();
	hideCtxMenu();
	const menu = el('div', { class: 'ctx-menu' },
		el('button', { text: item.folder ? '⚑ Report this folder' : '⚑ Report this link', onclick: () => openReport(item.id) }),
	);
	menu.style.left = Math.min(e.clientX, innerWidth - 190) + 'px';
	menu.style.top = Math.min(e.clientY, innerHeight - 60) + 'px';
	document.body.appendChild(menu);
	ctxMenuEl = menu;
});
document.addEventListener('pointerdown', e => {
	if (ctxMenuEl && !e.target.closest('.ctx-menu')) hideCtxMenu();
});

// ─────────────────────────────────────────────────────────────────────
// Link editor (v1.17.0) — add / edit / delete your own links without
// leaving the desktop. Opens from a tap in edit mode (desktop icon,
// folder member, dock icon) or the ＋ Add tile. All writes go to the
// owner-scoped /api/me/links endpoints; the server re-validates
// ownership on every call.
// ─────────────────────────────────────────────────────────────────────
let linkSheetEl = null;
let lsRow = null;          // the full link row being edited (null = new)
let lsIconsCache = null;   // { icons: [...], uploads: [...] }

function meFetch(method, path, body) {
	return fetch(path, {
		method,
		headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': S.csrf || '' },
		body: body ? JSON.stringify(body) : undefined,
	});
}

function ensureLinkSheet() {
	if (linkSheetEl) return linkSheetEl;
	const field = (labelText, node) => el('div', { class: 'ls-field' }, el('label', { text: labelText }), node);
	const title = el('h3', { class: 'ls-title', text: 'Edit link' });
	const labelIn = el('input', { class: 'ls-input', maxlength: 120, placeholder: 'Name' });
	const urlIn = el('input', { class: 'ls-input', placeholder: 'https://example.com' });
	const urlRow = field('Address', urlIn);
	const sectionSel = el('select', { class: 'ls-input' },
		el('option', { value: 'desktop', text: 'Desktop' }),
		el('option', { value: 'dock', text: 'Dock' }),
		el('option', { value: 'apps', text: 'All Apps' }),
	);
	// The About card carries the software branding — admin-only.
	if (S.user && S.user.role === 'admin') {
		sectionSel.appendChild(el('option', { value: 'about', text: 'About card' }));
	}
	const sectionRow = field('Place', sectionSel);
	const groupIn = el('input', { class: 'ls-input', placeholder: 'Group heading (e.g. Ai)', list: 'lsGroupList' });
	const groupList = el('datalist', { id: 'lsGroupList' });
	const groupRow = field('Group', groupIn);
	groupRow.appendChild(groupList);
	const unfolderBtn = el('button', { class: 'report-btn', type: 'button', text: 'Remove from folder', hidden: true });
	const unfolderRow = el('div', { class: 'ls-field' }, unfolderBtn);
	const iconPrev = el('img', { class: 'ls-icon-prev', alt: '' });
	const iconIn = el('input', { class: 'ls-input', placeholder: 'Auto (the site’s own icon)' });
	const fileIn = el('input', { type: 'file', accept: 'image/*', hidden: true });
	const autoBtn = el('button', { class: 'report-btn', type: 'button', text: 'Site icon' });
	const uploadBtn = el('button', { class: 'report-btn', type: 'button', text: 'Upload image…' });
	const iconRow = el('div', { class: 'ls-field' },
		el('label', { text: 'Icon' }),
		el('div', { class: 'ls-icon-row' }, iconPrev, iconIn),
		el('div', { class: 'ls-icon-actions' }, autoBtn, uploadBtn, fileIn),
		el('div', { class: 'ls-picker' }),
	);
	const bgColor = el('input', { type: 'color', class: 'ls-bg-color', value: '#2f6bff' });
	const bgIn = el('input', { class: 'ls-input ls-bg-text', placeholder: 'No background' });
	const bgClear = el('button', { class: 'report-btn', type: 'button', text: 'Clear' });
	const bgRow = el('div', { class: 'ls-field' },
		el('label', { text: 'Icon background' }),
		el('div', { class: 'ls-icon-row' }, bgColor, bgIn, bgClear),
	);
	const msg = el('p', { class: 'report-msg ls-msg' });
	const delBtn = el('button', { class: 'report-btn ls-delete', type: 'button', text: 'Delete' });
	const cancelBtn = el('button', { class: 'report-btn', type: 'button', text: 'Cancel' });
	const saveBtn = el('button', { class: 'report-btn primary', type: 'button', text: 'Save' });
	const card = el('div', { class: 'link-sheet-card' },
		title, field('Name', labelIn), urlRow, sectionRow, groupRow, unfolderRow, iconRow, bgRow, msg,
		el('div', { class: 'report-actions ls-actions' }, delBtn, el('span', { class: 'ls-spacer' }), cancelBtn, saveBtn),
	);
	const pop = el('div', { class: 'link-sheet', hidden: true }, card);
	pop.addEventListener('pointerdown', e => { if (e.target === pop) closeLinkSheet(); });
	document.body.appendChild(pop);
	linkSheetEl = pop;
	pop._f = { title, labelIn, urlIn, urlRow, sectionSel, sectionRow, groupIn, groupRow, groupList, unfolderBtn, unfolderRow, iconPrev, iconIn, fileIn, picker: iconRow.querySelector('.ls-picker'), bgColor, bgIn, msg, delBtn, saveBtn };
	unfolderBtn.addEventListener('click', () => saveLinkSheet(true));
	sectionSel.addEventListener('change', () => syncLsGroupRow());
	const syncPrev = () => {
		const v = iconIn.value.trim();
		iconPrev.src = v || (lsRow ? '/icon/' + lsRow.id : 'src/icons/globe.svg');
	};
	iconIn.addEventListener('input', syncPrev);
	iconPrev.addEventListener('error', () => { if (!iconPrev.src.endsWith('globe.svg')) iconPrev.src = 'src/icons/globe.svg'; });
	autoBtn.addEventListener('click', () => { iconIn.value = ''; syncPrev(); buildLsPicker(); });
	uploadBtn.addEventListener('click', () => fileIn.click());
	fileIn.addEventListener('change', async () => {
		const f = fileIn.files[0]; fileIn.value = '';
		if (!f) return;
		try {
			const dataUrl = await new Promise((res, rej) => { const r = new FileReader(); r.onload = () => res(r.result); r.onerror = rej; r.readAsDataURL(f); });
			const r = await meFetch('POST', '/api/me/icons', { filename: f.name, data_base64: dataUrl });
			const d = await r.json().catch(() => ({}));
			if (!r.ok) throw new Error(d.error || ('upload failed (' + r.status + ')'));
			iconIn.value = d.path;
			lsIconsCache = null;
			syncPrev(); buildLsPicker();
			msg.textContent = 'Image uploaded — Save to use it.';
		} catch (err) { msg.textContent = err.message; }
	});
	bgColor.addEventListener('input', () => { bgIn.value = bgColor.value; });
	bgClear.addEventListener('click', () => { bgIn.value = ''; });
	bgIn.addEventListener('input', () => { if (/^#[0-9a-fA-F]{6}$/.test(bgIn.value.trim())) bgColor.value = bgIn.value.trim(); });
	cancelBtn.addEventListener('click', () => closeLinkSheet());
	saveBtn.addEventListener('click', () => saveLinkSheet());
	delBtn.addEventListener('click', () => deleteLinkSheet());
	return pop;
}

async function lsIconData() {
	if (lsIconsCache) return lsIconsCache;
	try {
		const r = await meFetch('GET', '/api/me/icons');
		lsIconsCache = await r.json();
	} catch (_) { lsIconsCache = { icons: [], library: [], uploads: [] }; }
	return lsIconsCache;
}

async function buildLsPicker() {
	const F = linkSheetEl._f;
	const data = await lsIconData();
	const cur = F.iconIn.value.trim();
	F.picker.innerHTML = '';
	[...(data.uploads || []), ...(data.library || []), ...(data.icons || [])].forEach(path => {
		const b = el('button', { class: 'ls-pick' + (path === cur ? ' sel' : ''), type: 'button', title: path },
			el('img', { src: path, alt: '', loading: 'lazy' }));
		b.addEventListener('click', () => { F.iconIn.value = path; F.iconIn.dispatchEvent(new Event('input')); buildLsPicker(); });
		F.picker.appendChild(b);
	});
}

let lsAllLinks = [];

function syncLsGroupRow() {
	const F = linkSheetEl._f;
	const sec = F.sectionSel.value;
	const needs = sec === 'apps' || sec === 'about';
	F.groupRow.style.display = needs ? '' : 'none';
	if (needs) {
		const titles = [...new Set(lsAllLinks.filter(l => l.section === sec && l.group_title).map(l => l.group_title))];
		F.groupList.innerHTML = '';
		titles.forEach(t => F.groupList.appendChild(el('option', { value: t })));
	}
}

async function openLinkSheet(id, presetSection) {
	if (!S.csrf) return;
	const pop = ensureLinkSheet();
	const F = pop._f;
	F.msg.textContent = '';
	try {
		const r = await meFetch('GET', '/api/me/links');
		const d = await r.json();
		lsAllLinks = d.links || [];
	} catch (_) { lsAllLinks = []; }
	let row = null;
	if (id) {
		row = lsAllLinks.find(l => l.id === id) || null;
		if (!row) return;
	}
	lsRow = row;
	const isFolder = !!(row && row.kind === 'folder');
	F.title.textContent = row ? (isFolder ? 'Edit folder' : 'Edit link') : 'Add link';
	F.labelIn.value = row ? row.label : '';
	F.urlIn.value = row && row.url ? row.url : '';
	F.urlRow.style.display = isFolder ? 'none' : '';
	F.sectionSel.value = row ? row.section : (presetSection || 'desktop');
	F.sectionRow.style.display = isFolder ? 'none' : '';
	F.groupIn.value = row && row.group_title ? row.group_title : '';
	syncLsGroupRow();
	const inFolder = !!(row && row.folder_id);
	F.unfolderRow.style.display = inFolder ? '' : 'none';
	F.unfolderBtn.hidden = !inFolder;
	F.iconIn.value = row && row.icon_mode !== 'auto' && !String(row.icon || '').startsWith('/icon/') ? row.icon : '';
	F.bgIn.value = row && row.icon_bg ? row.icon_bg : '';
	if (F.bgIn.value) F.bgColor.value = F.bgIn.value;
	F.delBtn.style.display = row ? '' : 'none';
	F.delBtn.textContent = isFolder ? 'Delete folder' : 'Delete';
	pop.hidden = false;
	F.iconIn.dispatchEvent(new Event('input'));
	buildLsPicker();
	F.labelIn.focus();
}

function closeLinkSheet() {
	if (linkSheetEl) linkSheetEl.hidden = true;
	lsRow = null;
}

async function refreshAfterLinkChange() {
	await loadState();
	renderDesktopIcons();
	renderDock();
	if (openFolderId) {
		const still = S.desktopIcons.find(i => i.id === openFolderId && i.folder);
		if (still) openFolder(openFolderId, true); else closeFolder();
	}
}

async function saveLinkSheet(removeFromFolder) {
	const F = linkSheetEl._f;
	const iconVal = F.iconIn.value.trim();
	const isFolder = !!(lsRow && lsRow.kind === 'folder');
	const body = {
		label: F.labelIn.value.trim(),
		kind: lsRow ? lsRow.kind : 'url',
		section: isFolder ? 'desktop' : F.sectionSel.value,
		url: isFolder ? null : F.urlIn.value.trim(),
		icon_mode: iconVal ? 'bundled' : 'auto',
		icon: iconVal || null,
		icon_bg: F.bgIn.value.trim() || null,
	};
	if (removeFromFolder) { body.folder_id = ''; body.section = 'desktop'; }
	if (body.section === 'apps' || body.section === 'about') body.group_title = F.groupIn.value;
	if (!body.label) { F.msg.textContent = 'Give it a name first.'; return; }
	if (!isFolder && !body.url) { F.msg.textContent = 'Add the address (https://…).'; return; }
	F.saveBtn.disabled = true;
	try {
		const r = lsRow
			? await meFetch('POST', '/api/me/links/' + lsRow.id + '/update', body)
			: await meFetch('POST', '/api/me/links/create', body);
		const d = await r.json().catch(() => ({}));
		if (!r.ok) throw new Error(d.error || ('save failed (' + r.status + ')'));
		closeLinkSheet();
		await refreshAfterLinkChange();
	} catch (err) { F.msg.textContent = err.message; }
	F.saveBtn.disabled = false;
}

async function deleteLinkSheet() {
	if (!lsRow) return;
	const F = linkSheetEl._f;
	const isFolder = lsRow.kind === 'folder';
	if (!confirm(isFolder
		? 'Delete the folder “' + lsRow.label + '”? Its apps go back to the desktop.'
		: 'Delete “' + lsRow.label + '”?')) return;
	try {
		const r = await meFetch('POST', '/api/me/links/' + lsRow.id + '/delete', {});
		if (!r.ok) { const d = await r.json().catch(() => ({})); throw new Error(d.error || ('delete failed (' + r.status + ')')); }
		closeLinkSheet();
		await refreshAfterLinkChange();
	} catch (err) { F.msg.textContent = err.message; }
}

// Dock icons join in: in edit mode a tap opens the editor, and clicks
// never navigate while editing.
document.addEventListener('click', e => {
	if (desktopEditing && e.target.closest && e.target.closest('.dock-icon')) { e.preventDefault(); e.stopPropagation(); }
}, true);
document.addEventListener('pointerup', e => {
	if (!desktopEditing || !S.csrf) return;
	const a = e.target.closest && e.target.closest('.dock-icon');
	if (a && a.dataset.id) openLinkSheet(a.dataset.id);
});
