// ─────────────────────────────────────────────────────────────────────
// All Apps — Miller columns UI, read-only launcher edition
// ─────────────────────────────────────────────────────────────────────
// The All Apps window is a launcher grid only: browse categories, search,
// click to open. There is no edit mode, no pick mode, no inline rename —
// links are managed from the admin panel.
const FOLDER_SVG = '<svg width="15" height="15" viewBox="0 0 16 16" fill="currentColor"><path d="M1.5 4c0-.55.45-1 1-1h3.1c.32 0 .62.15.8.4L7.3 4.5H13.5c.55 0 1 .45 1 1V12c0 .55-.45 1-1 1h-11c-.55 0-1-.45-1-1V4z"/></svg>';

let bookmarkQuery = '';

// ── Accessors ─────────────────────────────────────────────────────────
function cats() { return S.bookmarkSections[0].blocks; }

// ── Renderers ─────────────────────────────────────────────────────────
function renderMillerCats() {
	const col = $('millerCats');
	if (!col) return;
	col.innerHTML = '';
	cats().forEach((c, bi) => {
		const links   = c.links;
		const matches = bookmarkQuery
			? links.filter(l => l.label.toLowerCase().includes(bookmarkQuery)).length
			: links.length;
		const item = document.createElement('div');
		item.className  = 'miller-cat';
		item.dataset.bi = bi;
		if (c.locked && !c.unlocked) item.classList.add('locked');
		if (bookmarkQuery && matches === 0) item.classList.add('dim');
		if (!bookmarkQuery && bi === millerSel) item.classList.add('sel');
		item.innerHTML = '<span class="miller-cat-ico">' + FOLDER_SVG + '</span>';
		item.appendChild(el('span', { class: 'miller-cat-label', text: c.locked && !c.unlocked ? c.title + ' 🔒' : c.title }));
		item.appendChild(el('span', { class: 'miller-cat-count', text: matches }));
		item.appendChild(el('span', { class: 'miller-chevron', text: '›' }));
		col.appendChild(item);
	});
}

function renderMillerLinks() {
	const col = $('millerLinks');
	if (!col) return;
	col.innerHTML = '';
	const entries = [];
	if (bookmarkQuery) {
		cats().forEach((c, bi) => c.links.forEach((l, li) => {
			if (l.label.toLowerCase().includes(bookmarkQuery)) entries.push({ link: l, cat: c.title, bi, li });
		}));
	} else {
		const blk = cats()[millerSel];
		if (blk) blk.links.forEach((l, li) => entries.push({ link: l, cat: null, bi: millerSel, li }));
	}
	if (!entries.length) {
		const empty = document.createElement('div');
		empty.className   = 'miller-empty';
		empty.textContent = bookmarkQuery ? 'No results' : 'No links';
		col.appendChild(empty);
	} else {
		entries.forEach(({ link, cat }) => {
			const row = el('div', { class: 'miller-link-row' });
			const art = iconArt(link); art.classList.add('miller-link-ico');
			if (link.locked) {
				const a = el('a', { class: 'miller-link locked', href: '#', draggable: false, data: { search: '', key: '' } },
					art,
					el('span', { class: 'miller-link-label', text: '🔒 Locked' }),
				);
				a.addEventListener('click', e => { e.preventDefault(); openUnlockPrompt(link.lock_target); });
				row.appendChild(a);
				col.appendChild(row);
				return;
			}
			const a = el('a', { class: 'miller-link', href: link.href, target: '_blank', rel: 'noopener noreferrer', draggable: false, data: { search: link.label.toLowerCase(), key: link.id } },
				art,
				el('span', { class: 'miller-link-label', text: link.label }),
			);
			if (cat) a.appendChild(el('span', { class: 'miller-link-cat', text: cat }));
			row.appendChild(a);
			col.appendChild(row);
		});
	}
}

function renderAllBookmarks() {
	const scroll = $('allAppsScroll');
	scroll.innerHTML = '';
	const miller = document.createElement('div');
	miller.className = 'miller';
	miller.innerHTML = '<div class="miller-col miller-cats" id="millerCats"></div>'
		+ '<div class="miller-col miller-links" id="millerLinks"></div>';
	scroll.appendChild(miller);

	if (millerSel >= cats().length) millerSel = 0;

	renderMillerCats();
	renderMillerLinks();
}

// ── Open / close ──────────────────────────────────────────────────────
function openAllBookmarks() {
	const overlay = $('allApps');
	$('allAppsWindow').style.translate = '';
	renderAllBookmarks();
	overlay.classList.add('open');
	setTimeout(() => $('bookmarkSearch').focus(), 260);
}

function closeAllApps() {
	const overlay = $('allApps');
	overlay.classList.remove('open');
	$('bookmarkSearch').value = '';
	filterBookmarks('');
}

// ── Search / filter ───────────────────────────────────────────────────
function filterBookmarks(query) {
	bookmarkQuery = query.trim().toLowerCase();
	$('allApps').classList.toggle('searching', !!bookmarkQuery);
	renderMillerCats();
	renderMillerLinks();
}

// ── Locked group unlock ───────────────────────────────────────────────
// A locked All Apps group renders icon-only placeholders; tapping one
// asks for the group password here. Success sets the grant cookie, so
// a state reload brings the real links in.
let unlockPopEl = null;
let unlockTarget = null;

function openUnlockPrompt(target) {
	if (!target) return;
	unlockTarget = target;
	if (!unlockPopEl) {
		const pw = el('input', { type: 'password', class: 'ls-input', placeholder: 'Group password', autocomplete: 'current-password' });
		const err = el('p', { class: 'ls-msg' });
		const cancel = el('button', { class: 'report-btn', type: 'button', text: 'Cancel' });
		const go = el('button', { class: 'report-btn primary', type: 'button', text: 'Unlock' });
		const card = el('div', { class: 'link-sheet-card' },
			el('h3', { class: 'ls-title', text: '🔒 Locked group' }),
			el('p', { class: 'repsub', text: 'This group is password-protected.' }),
			pw, err,
			el('div', { class: 'report-actions ls-actions' }, cancel, go),
		);
		const pop = el('div', { class: 'link-sheet', hidden: true }, card);
		pop._f = { pw, err };
		cancel.addEventListener('click', () => { pop.hidden = true; });
		pop.addEventListener('pointerdown', e => { if (e.target === pop) pop.hidden = true; });
		go.addEventListener('click', async () => {
			const r = await fetch('/api/unlock', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ target: unlockTarget, password: pw.value }) });
			if (r.ok) { pop.hidden = true; pw.value = ''; await loadState(); renderAllBookmarks(); }
			else err.textContent = 'Wrong password.';
		});
		pw.addEventListener('keydown', e => { if (e.key === 'Enter') go.click(); e.stopPropagation(); });
		document.body.appendChild(pop);
		unlockPopEl = pop;
	}
	unlockPopEl._f.err.textContent = '';
	unlockPopEl.hidden = false;
	unlockPopEl._f.pw.focus();
}

// ── Bookmark window drag (title-bar) ──────────────────────────────────
// The window is flex-centered (with an 8vh bottom-padding nudge) and moved via
// `translate`, so its base position isn't a simple formula. Rather than assume
// one, nudge `translate` by the delta between where the window is now and where
// the clamped position wants it — self-correcting regardless of the layout.
function setupBookmarkWindowDrag() {
	const bar = $('allAppsBar');
	const win = $('allAppsWindow');
	makeWindowDraggable(bar, win, bar, (left, top) => {
		const r = win.getBoundingClientRect();
		const t = (win.style.translate || '').split(/\s+/);
		const tx = parseFloat(t[0]) || 0, ty = parseFloat(t[1]) || 0;
		win.style.translate = (tx + left - r.left) + 'px ' + (ty + top - r.top) + 'px';
	});
}
