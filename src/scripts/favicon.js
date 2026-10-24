// ─────────────────────────────────────────────────────────────────────
// Icon art — server-model edition
// ─────────────────────────────────────────────────────────────────────
// Icons come from the server model verbatim: each link carries an `icon`
// value from /api/state, either a bundled asset path (e.g.
// "src/icons/globe.svg") or "/icon/<id>" for a favicon the server fetched
// and cached itself. The client NEVER derives or fetches an icon from a
// link's URL — it doesn't know any URLs — and nothing is cached locally.

// Icon as a floating tile: a sharp image over a blurred, enlarged copy of
// itself (self-bloom). Used everywhere icons appear.

// Shared scaffold: span.icon-art (shadow) > span.icon-tile (squircle mask)
// wrapping a blurred bloom + a crisp sharp img.
function iconArtShell() {
	const wrap  = document.createElement('span'); wrap.className = 'icon-art';
	const tile  = document.createElement('span'); tile.className = 'icon-tile';
	const bloom = document.createElement('img');  bloom.className = 'icon-bloom'; bloom.alt = ''; bloom.decoding = 'async';
	const sharp = document.createElement('img');  sharp.className = 'icon-sharp'; sharp.alt = ''; sharp.decoding = 'async';
	tile.appendChild(bloom); tile.appendChild(sharp); wrap.appendChild(tile);
	return { wrap, bloom, sharp };
}

// Tile from a fixed image src (a bundled asset path or /icon/<id>).
function iconArtFromSrc(src) {
	const { wrap, bloom, sharp } = iconArtShell();
	bloom.src = src; sharp.src = src;
	return wrap;
}

// Tile for a link item from the server model: uses its `icon` verbatim.
// An item may carry an icon_bg plate color: transparent artwork normally
// floats over a white bloom plate, so a light glyph on transparency is
// invisible — the owner's chosen color replaces the plate and the bloom
// (the blurred white copy) is hidden.
function iconArt(item) {
	const art = iconArtFromSrc(item.icon || 'src/icons/globe.svg');
	if (item.icon_bg) {
		const tile = art.querySelector('.icon-tile');
		if (tile) {
			tile.style.background = item.icon_bg;
			const bloom = tile.querySelector('.icon-bloom');
			if (bloom) bloom.style.display = 'none';
		}
	}
	return art;
}
