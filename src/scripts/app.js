// ─────────────────────────────────────────────────────────────────────
// Event wiring — read-only launcher edition
// ─────────────────────────────────────────────────────────────────────
// No file menu, no welcome/sync flow, no edit or pick modes. The public page
// can only launch links; all management happens in the admin panel.
function setupEvents() {
	$('mbBookmarks').addEventListener('click', () => {
		$('allApps').classList.contains('open') ? closeAllApps() : openAllBookmarks();
	});
	$('mbBrand').addEventListener('click', e => { e.stopPropagation(); toggleAboutPanel(); });
	$('mbWallpaper').addEventListener('click', () => { toggleWallpaperPanel(); });

	$('dock').addEventListener('click', e => {
		const launch = e.target.closest('.dock-icon, .dock-all-apps');
		if (launch) {
			launch.classList.remove('bouncing');
			void launch.offsetWidth;
			launch.classList.add('bouncing');
			launch.addEventListener('animationend', () => launch.classList.remove('bouncing'), { once: true });
		}
	});

	$('tlClose').addEventListener('click', closeAllApps);
	$('tlMin').addEventListener('click', closeAllApps);

	$('allAppsScroll').addEventListener('click', e => {
		const cat = e.target.closest('.miller-cat');
		if (cat) {
			millerSel = +cat.dataset.bi;
			if (bookmarkQuery) {
				$('bookmarkSearch').value = '';
				bookmarkQuery = '';
				$('allApps').classList.remove('searching');
			}
			renderMillerCats(); renderMillerLinks();
			return;
		}
	});

	$('bookmarkSearch').addEventListener('input', function () { filterBookmarks(this.value); });

	document.addEventListener('keydown', e => {
		if (e.key !== 'Escape') return;
		if ($('aboutPanel').classList.contains('open')) { closeAboutPanel(); return; }
		if ($('allApps').classList.contains('open')) { closeAllApps(); return; }
	});
}

// ─────────────────────────────────────────────────────────────────────
// About card — rendered from the server branding + the owner's About-row
// links (section 'about'). The credit line at the card's foot is fixed in
// the template and is not part of the editable branding.
// ─────────────────────────────────────────────────────────────────────
function renderAbout() {
	const b = S.branding || {};
	if (b.product_name) {
		document.title = b.product_name;
		$('aboutProduct').textContent = b.product_name;
	}
	$('aboutTagline').textContent = b.tagline || '';
	if (b.app_icon) {
		$('aboutIcon').src = b.app_icon;
		const fav = document.querySelector('link[rel="icon"]');
		if (fav) fav.href = b.app_icon;
	}
	$('aboutProfileName').textContent = b.profile_name || '';
	$('aboutProfileSub').textContent = b.profile_sub || '';
	$('aboutNote').textContent = b.about_note || '';

	// Browser chips become real links. An admin-set branding URL wins;
	// otherwise the chip is the extension download for whoever is
	// signed in (they pair it with their own sync token), or the
	// sign-in page when nobody is.
	document.querySelectorAll('.about-chip[data-browser]').forEach(chip => {
		const browser = chip.dataset.browser;
		const custom = (b[browser + '_url'] || '').trim();
		const a = el('a', { class: 'about-chip', data: { browser } });
		a.textContent = chip.textContent;
		if (custom) {
			a.href = custom;
			// Site-relative links (like the extension downloads) stay in
			// the same tab; off-site links open a new one.
			if (!custom.startsWith('/')) {
				a.target = '_blank';
				a.rel = 'noopener noreferrer';
			}
			a.title = custom;
		} else if (S.csrf) {
			a.href = '/api/admin/extension?flavor=' + (browser === 'firefox' ? 'firefox' : 'chrome');
			a.title = 'Download the ' + (browser === 'firefox' ? 'Firefox' : 'Chrome / Edge / Brave') + ' extension — pair it with your sync token';
		} else {
			a.href = '/login';
			a.title = 'Sign in to download the extension';
		}
		chip.replaceWith(a);
	});

	const rows = $('aboutRows');
	rows.innerHTML = '';
	(b.profile_rows || []).forEach(r => {
		rows.appendChild(el('div', { class: 'contact-row' },
			el('span', { class: 'contact-label', text: r.label }),
			el('span', { class: 'contact-email', text: r.value })));
	});

	const strip = $('aboutStrip');
	strip.innerHTML = '';
	(S.about || []).forEach((g, gi) => {
		if (gi > 0) strip.appendChild(el('span', { class: 'strip-sep', text: '|' }));
		const icons = el('div', { class: 'icons' });
		g.links.forEach(l => {
			const img = el('img', { src: l.icon, alt: l.label });
			if (l.icon.startsWith('src/social/')) img.classList.add('icon-silhouette');
			if (l.icon.endsWith('paragraph.png')) img.classList.add('icon-img-filtered');
			icons.appendChild(el('a', { class: 'icon-link', href: l.href, target: '_blank', rel: 'noopener' }, img));
		});
		strip.appendChild(el('div', { class: 'icon-group' },
			el('span', { class: 'icon-group-label', text: g.title }), icons));
	});
}

// ─────────────────────────────────────────────────────────────────────
// About-card account line — a discreet "Sign in" (or who's signed in),
// never a tile or a banner.
// ─────────────────────────────────────────────────────────────────────
async function setupAccountLine() {
	$('aboutSignout').addEventListener('click', async e => {
		e.preventDefault();
		try { await fetch('/logout', { method: 'POST' }); } catch (_) {}
		location.reload();
	});
	try {
		const r = await fetch('/api/whoami');
		if (!r.ok) return;
		const d = await r.json();
		if (!d.user && d.registration_open) {
			// Self-registration is on: offer Sign up next to Sign in.
			$('aboutSignupWrap').hidden = false;
		}
		if (d.user) {
			$('aboutSignin').hidden = true;
			$('aboutUser').hidden = false;
			$('aboutUsername').textContent = d.user.username;
			// Signed in: the owner may rearrange their own desktop grid.
			S.csrf = d.csrf || '';
			const editBtn = $('mbEdit');
			editBtn.hidden = false;
			editBtn.addEventListener('click', () => toggleDesktopEdit());
		}
	} catch (_) {}
}

// ─────────────────────────────────────────────────────────────────────
// Init — state comes from the server; first paint is the desktop itself
// (there is no welcome card in this edition).
// ─────────────────────────────────────────────────────────────────────
// ── Startpage PIN gate (v1.21.0) ─────────────────────────────
// When the server says this owner's page is PIN-protected, it sent no
// links at all — so the desktop stays empty behind a full-screen gate.
// A correct PIN earns the grant cookie and a reload brings the page.
// The gate is a lock screen, not a modal: Escape never dismisses it.
function setupPinGate() {
	const gate = $('pinGate');
	const input = $('pinGateInput');
	const err = $('pinGateErr');
	if (S.pinLen) input.maxLength = S.pinLen;
	$('pinGateForm').addEventListener('submit', async e => {
		e.preventDefault();
		err.textContent = '';
		try {
			const r = await fetch('/api/page-pin/unlock', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ pin: input.value }),
			});
			if (!r.ok) {
				let detail = 'unlock ' + r.status;
				try { const d = await r.json(); if (d.error) detail = d.error; } catch (_) {}
				throw new Error(detail);
			}
			location.reload();
		} catch (e2) {
			err.textContent = e2.message;
			input.value = '';
			input.focus();
		}
	});
	gate.hidden = false;
	input.focus();
}

(async function init() {
	await loadState();
	applyWallpaper(S.wallpaper);
	if (S.pinRequired) { setupPinGate(); return; }
	applyRotationAtBoot();
	renderDesktopIcons();
	renderDock();
	renderAbout();
	setupDockMagnification();
	setupDesktopIconPress();
	setupDesktopEditing();
	setupEvents();
	startClock();
	startStatus();
	setupCalendar();
	setupBookmarkWindowDrag();
	setupAboutPanel();
	setupAccountLine();
})();

