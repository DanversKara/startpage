// ─────────────────────────────────────────────────────────────────────
// State — server-backed edition
// ─────────────────────────────────────────────────────────────────────
// Links live on the server, not in this browser. The page fetches its whole
// model from GET /api/state at boot; the payload contains no real URLs, only
// opaque ids. Every rendered anchor points at /go/<id> and the server
// resolves it to the real destination with a redirect when clicked.
//
// There is deliberately no localStorage persistence and no client-side
// editing: links are managed exclusively from the admin panel.

const GRID = { ox: 24, oy: 55, uw: 120, uh: 124 };

let S = { desktopIcons: [], dock: [], bookmarkSections: [{ blocks: [] }], about: [], widgets: [], branding: null, wallpaper: null, rotation: null, pinRequired: false, pinLen: null };
let millerSel = 0;

// ─────────────────────────────────────────────────────────────────────
// Grid utilities
// ─────────────────────────────────────────────────────────────────────
function nextFreeCell(occupied) {
	const maxCols = Math.max(1, Math.floor((window.innerWidth - GRID.ox) / GRID.uw));
	for (let row = 0; row < 100; row++)
		for (let col = 0; col < maxCols; col++)
			if (!occupied.has(col + ',' + row)) return { col, row };
	return { col: 0, row: 0 };
}

function occupiedCells() {
	const taken = new Set(S.desktopIcons.filter(i => i.col != null).map(i => i.col + ',' + i.row));
	(S.widgets || []).forEach(w => {
		if (w.col == null || w.row == null) return;
		const s = widgetSpan(w);
		for (let r = s.row; r < s.row + s.h; r++)
			for (let c = s.col; c < s.col + s.w; c++) taken.add(c + ',' + r);
	});
	return taken;
}

// A widget's span on the grid, clamped to the viewport's columns so a
// wide widget never claims cells past the right edge.
function widgetSpan(w) {
	const maxCols = Math.max(1, Math.floor((window.innerWidth - GRID.ox) / GRID.uw));
	return {
		col: (w.col == null ? 0 : w.col),
		row: (w.row == null ? 0 : w.row),
		w: Math.max(1, Math.min(w.w || 3, maxCols)),
		h: Math.max(1, w.h || 2),
	};
}

// First top-left cell where a w-by-h block fits, row-major.
function nextFreeSpan(occupied, w, h) {
	const maxCols = Math.max(1, Math.floor((window.innerWidth - GRID.ox) / GRID.uw));
	const ww = Math.max(1, Math.min(w, maxCols));
	const hh = Math.max(1, h);
	for (let row = 0; row < 100; row++)
		for (let col = 0; col + ww <= maxCols; col++) {
			let fits = true;
			for (let r = row; r < row + hh && fits; r++)
				for (let c = col; c < col + ww; c++)
					if (occupied.has(c + ',' + r)) { fits = false; break; }
			if (fits) return { col, row };
		}
	return { col: 0, row: 0 };
}

// Is a w-by-h span at (col, row) entirely free? Optionally ignores one
// widget (the one being moved/resized — its old cells free up).
function spanFree(col, row, w, h, excludeWidgetId) {
	const maxCols = Math.max(1, Math.floor((window.innerWidth - GRID.ox) / GRID.uw));
	if (col < 0 || row < 0 || col + w > maxCols) return false;
	const taken = occupiedCells();
	if (excludeWidgetId) {
		const self = (S.widgets || []).find(x => x.id === excludeWidgetId);
		if (self && self.col != null && self.row != null) {
			const s = widgetSpan(self);
			for (let r = s.row; r < s.row + s.h; r++)
				for (let c = s.col; c < s.col + s.w; c++) taken.delete(c + ',' + r);
		}
	}
	for (let r = row; r < row + h; r++)
		for (let c = col; c < col + w; c++)
			if (taken.has(c + ',' + r)) return false;
	return true;
}

// The widget (if any) whose span covers one grid cell.
function widgetCoveringCell(col, row) {
	return (S.widgets || []).find(w => {
		if (w.col == null || w.row == null) return false;
		const s = widgetSpan(w);
		return col >= s.col && col < s.col + s.w && row >= s.row && row < s.row + s.h;
	}) || null;
}

function ensureIconPositions() {
	const occupied = occupiedCells();
	S.desktopIcons.forEach(icon => {
		if (icon.col == null || icon.row == null) {
			const { col, row } = nextFreeCell(occupied);
			icon.col = col; icon.row = row;
			occupied.add(col + ',' + row);
		}
	});
}

// Widgets created before grid placement get a cell: first free span of
// their size, after the icons. (The server backfills this too on boot;
// this is the safety net for any row that still arrives unplaced.)
function ensureWidgetPositions() {
	const occupied = occupiedCells();
	(S.widgets || []).forEach(w => {
		if (w.col != null && w.row != null) return;
		const s = widgetSpan(w);
		const { col, row } = nextFreeSpan(occupied, s.w, s.h);
		w.col = col; w.row = row;
		for (let r = row; r < row + s.h; r++)
			for (let c = col; c < col + s.w; c++) occupied.add(c + ',' + r);
	});
}

// ─────────────────────────────────────────────────────────────────────
// Server state
// ─────────────────────────────────────────────────────────────────────
// Public payload shape:
//   { desktop: [{id,label,icon,col,row}],
//     dock:    [{id,label,icon}],
//     apps:    [{title, links:[{id,label,icon}]}] }
// Internally each item also gets `href: '/go/<id>'`, the only URL-ish value
// the client ever sees.
function linkItem(x) {
	const it = { id: x.id, label: x.label, icon: x.icon, icon_bg: x.icon_bg || null, href: '/go/' + encodeURIComponent(x.id) };
	if (x.locked) { it.locked = true; if (x.lock_target) it.lock_target = x.lock_target; }
	if (x.folder) {
		it.folder = true;
		it.unlocked = !!x.unlocked;
		it.members = (x.members || []).map(linkItem);
	}
	return it;
}

async function loadState() {
	// The session CSRF token lives on S (set by the account check in
	// app.js) and authorizes every edit call — a state rebuild must carry
	// it over, or the first folder/move refresh silently disarms editing
	// for the rest of the session.
	const csrf = S.csrf || '';
	try {
		const r = await fetch('/api/state', { headers: { 'Accept': 'application/json' } });
		if (!r.ok) throw new Error('state ' + r.status);
		const data = await r.json();
		S = {
			desktopIcons: (data.desktop || []).map(x => Object.assign(linkItem(x), { col: x.col, row: x.row })),
			dock: (data.dock || []).map(linkItem),
			bookmarkSections: [{ blocks: (data.apps || []).map(g => ({
				title: g.title,
				locked: !!g.locked,
				unlocked: !!g.unlocked,
				lock_target: g.lock_target || null,
				links: (g.links || []).map(linkItem),
			})) }],
			about: (data.about || []).map(g => ({
				title: g.title,
				links: (g.links || []).map(linkItem),
			})),
			branding: data.branding || null,
			wallpaper: data.wallpaper || null,
			rotation: data.rotation || null,
			pinRequired: !!data.pin_required,
			pinLen: data.pin_len || null,
			// Widgets are the owner's own HTML embeds (and public ones,
			// when the viewer isn't the owner). col/row/w/h place each
			// one on the desktop grid like an icon tile.
			widgets: (data.widgets || []).map(w => ({ id: w.id, title: w.title, html: w.html, sort: w.sort, is_public: !!w.is_public, col: w.col, row: w.row, w: w.w || 3, h: w.h || 2 })),
			csrf,
		};
	} catch (_) {
		S = { desktopIcons: [], dock: [], bookmarkSections: [{ blocks: [] }], about: [], widgets: [], branding: null, wallpaper: null, rotation: null, pinRequired: false, pinLen: null, csrf };
	}
	ensureIconPositions();
	ensureWidgetPositions();
}

// ── Wallpaper ──────────────────────────────────────────────────
// The desktop background comes from the server (state.wallpaper):
// the signed-in user's choice, else the public owner's, else the
// bundled default. It rides a CSS variable so choosing a new one
// repaints instantly without a reload.
function applyWallpaper(path) {
	if (path) document.documentElement.style.setProperty('--wallpaper-url', 'url("' + path + '")');
	else document.documentElement.style.removeProperty('--wallpaper-url');
}
