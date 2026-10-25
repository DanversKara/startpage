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

let S = { desktopIcons: [], dock: [], bookmarkSections: [{ blocks: [] }], about: [], branding: null, wallpaper: null, rotation: null };
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
	return new Set(S.desktopIcons.filter(i => i.col != null).map(i => i.col + ',' + i.row));
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
			csrf,
		};
	} catch (_) {
		S = { desktopIcons: [], dock: [], bookmarkSections: [{ blocks: [] }], about: [], branding: null, wallpaper: null, rotation: null, csrf };
	}
	ensureIconPositions();
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
