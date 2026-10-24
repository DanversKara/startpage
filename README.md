# Startpage — Bookmark OS, Secured Edition

A desktop-for-your-links startpage with a real backend. This is a secured,
multi-user edition of **Bookmark OS** by Marc Cheng (see *Attribution*).

## What changed vs. upstream

Upstream Bookmark OS is a **static web page with no backend at all** — despite
occasionally being described as a PHP app, there is no server code in it: the
links live in the browser's `localStorage`, edited through on-page UI. That
means no security boundary, and your links don't follow you from browser to
browser.

This edition adds a small **Rust** service (`server/`, binary
`startpage-server`: axum + SQLite) and makes the page a pure launcher:

- **Links live on the server**, per user, in SQLite — the same desktop from
  every browser.
- **The public page contains no real URLs.** Every tile, dock icon, All Apps
  entry and About-row icon is an opaque `/go/<id>`; the server resolves it
  with a 302 redirect when clicked. View-source reveals nothing.
- **All management moved into an authenticated admin panel** at an obscure,
  configurable path. The public page has no Add/Edit/Import/Sync UI at all —
  the dock and the read-only All Apps window stay.
- **Disguised links:** any admin link can be labelled like a normal site
  (e.g. "Gmail" with a mail icon) while actually opening the admin login.
  Uniform `/go/<id>` treatment makes it indistinguishable from other tiles.
- **Icons are server-side too:** the server fetches and caches favicons
  itself (`/icon/<id>`); the browser never derives an icon from a URL.
  If a site's favicon can't be fetched (bot walls, LAN panels that serve
  none), the server falls back to a bundled file named after the link's
  host — `src/icons/<host>.png` (also `.svg`/`.ico`/`.jpg`/`.webp`, with a
  parent-domain fallback for subdomains). Drop a file in that folder and
  every link to that host picks it up; the files also appear in the
  panel's Bundled-icon dropdown.
- **Branding is server-side** (product name, tagline, About note, profile
  block) and starts out identical to the upstream About card.

## Security model — and its honest limits

- Passwords are hashed with **argon2id**; sessions are server-side, hashed
  at rest, HttpOnly + SameSite=Lax cookies (Secure behind HTTPS), revocable.
- Logins are **rate-limited per IP** (5 failures → 15-minute lockout).
- Admin API mutations require a per-session **CSRF token**.
- The admin panel lives at a generated obscure path (`/panel-<16 hex>`,
  changeable in the panel). Obscurity hides *where the door is*; the lock is
  the password.
- **The disguise's limit:** it hides *which tile is the door* from the page
  source and casual inspection. It cannot hide the destination from someone
  who clicks the tile and watches where they land — no design can. Protect
  the admin password accordingly, and **run this behind HTTPS** (e.g. a
  reverse proxy) anywhere beyond your LAN: over plain HTTP the session cookie
  and synced link URLs are visible to the network path.
- `/api/sync` (extension) is the only other URL-revealing route; it requires
  a per-user bearer token, hashed at rest, revocable per device.
- Nothing here is multi-tenant hardened for the open internet; it's built
  for a personal/family startpage behind your own proxy.

## Users

- One **admin** (the owner) plus any number of **users**. There is **no
  self-registration** — accounts are created by the admin only.
- Everyone has **their own link set** (desktop / dock / All Apps / About
  rows), their **own sync tokens**, and their own extension cache.
- Signed out, the public page shows the **admin's** set (the public face —
  the disguised admin tile lives there). Signed in (discreet *Sign in* link
  in the About card, or `/login`), it shows **your** set.
- Users manage their own password and sync tokens at **`/account`**. They
  can't edit links in this version — the admin edits any user's set from the
  panel via the owner switcher.
- A new user starts with a **clone of the admin's current set** (new ids;
  the disguised admin link is never cloned), so their first page isn't empty.
- Disabling a user kills their sessions and sync tokens immediately;
  deleting a user cascades to their links, tokens and sessions.

## Import & export

The admin panel's Links card exports the selected user's **entire link set** as
one JSON file, and imports the same format (hand-written files are fine — this
is the shape to produce when converting links from another page):

```json
{
  "app": "startpage",
  "format": 1,
  "owner": "admin",
  "exported_at": 0,
  "links": [
    { "label": "YouTube", "kind": "url", "url": "https://www.youtube.com/",
      "icon": null, "icon_mode": "auto", "section": "desktop",
      "col": 0, "row": 2, "sort": 0, "group_title": null },
    { "label": "Gmail", "kind": "admin", "url": null,
      "icon": "src/icons/mail.svg", "icon_mode": "bundled", "section": "desktop",
      "col": 0, "row": 0, "sort": 0, "group_title": null }
  ]
}
```

Field rules: `section` is `desktop` | `dock` | `apps` | `about`; `col`/`row`
are the desktop grid cell (null elsewhere); `group_title` names the All Apps
group or About row; `icon_mode: "auto"` makes the server fetch the site's
favicon, `"bundled"` takes a path under `src/`; `kind: "admin"` is the
disguised admin-login link (no `url`; admin-owned sets only); `kind: "folder"`
is a desktop folder (no `url`; desktop only) whose `members` list of
`{label, url}` references is resolved against the imported set on import.
Import offers
**Replace** (wipe the user's set, then load) or **Append** (skip entries whose
section + URL already exist); invalid entries are reported back, not silently
dropped.

## Run it (Docker)

```bash
docker compose up -d --build
docker compose logs startpage | grep "ADMIN LOGIN PATH"
```

The log line shows the one-time admin path, e.g. `/panel-0123abcd…`. Open
`http://<host>:8080/panel-…`, create the admin account (username + a
password of 10+ characters), and you're in the panel:

1. **Links** — add/edit/delete tiles per user; try the
   *＋ New disguised admin link* quick action (a "Gmail" tile that opens the
   panel login).
2. **Users** — create accounts for the household.
3. **Extension sync tokens** — pair browsers (below).
4. **Branding** — the About card starts as the upstream original; rebrand
   from there. (The credit line at the card's foot is fixed by design.)
5. Point your start page / new tab at the server, or front it with your
   reverse proxy for HTTPS.

Data lives in the `startpage-data` volume (`/data/startpage.db`).

### Environment

| Variable      | Default   | Purpose                                                        |
|---------------|-----------|----------------------------------------------------------------|
| `PORT`        | `8080`    | Listen port (binds 0.0.0.0).                                   |
| `DATA_DIR`    | `./data`  | SQLite location (`startpage.db`). `/data` in Docker.           |
| `WEB_DIR`     | repo root | Static root (`index.html` + `src/`). `/app/web` in Docker.     |
| `RESET_ADMIN` | unset     | `1` for one boot deletes the admin account (see recovery).     |

### Updating

```bash
git pull
docker compose up -d --build
```

The database volume persists; schema is created with
`CREATE TABLE IF NOT EXISTS` on boot.

### Password recovery

Locked out of the admin account? Restart once with `RESET_ADMIN=1`
(uncomment it in `docker-compose.yml`, `docker compose up -d`, then remove it
again). The admin account is deleted, **its links are parked** and every
session revoked; visit the admin path (see the original log line, or the
panel's Settings if you changed it) and set up a new admin — it re-adopts the
parked links. Other users are untouched.

## Offline extension sync

The upstream project also ships as a browser extension (Chrome/Firefox/Edge;
Brave uses the Chromium package). This edition makes it an **offline cache**
of your server links:

1. In the admin panel (or at `/account`) create a **sync token** for a device
   — the plaintext is shown **once**.
2. Build/install the extension: `python packaging/build.py`, then load
   `build/chrome/` unpacked (or use the zip in `dist/`).
3. Open the extension's **options page**, enter the server URL
   (default `https://start.dox.box`) and the token, hit **Test & save**.

From then on the extension syncs `GET /api/sync` (ETag-aware) into
`chrome.storage.local`, refreshes it in the background every 30 minutes, and
the new-tab page renders from that cache instantly — **server down included**.
Clicks use the cached real URLs, so links open offline.

Honest notes:

- The cache **contains real URLs by design** — it is a trusted copy for the
  owner's own browser. Treat a paired browser like a signed-in session:
  revoke its token (admin panel or `/account`) if the device is lost.
- The disguised admin link is **just a normal link inside the extension**:
  the camouflage lives on the public site; in your own browser it simply
  opens the admin login, which still asks for the password.
- Tokens are per user, so each person's extension shows *their* set.

## Layout

```
index.html, src/     the public page (no URLs, display-only)
server/              the Rust service (Cargo project; seed.json = upstream defaults)
packaging/           browser-extension packaging (manifest, options, background sync)
Dockerfile           multi-stage: cargo build → debian-slim runtime
docker-compose.yml   one service + a data volume
```

## Attribution

Built on **Bookmark OS** by **Marc Cheng** — open source (CC0); the upstream
`LICENSE` file is kept in this repo root. The About card carries the fixed
credit line *"Built on Bookmark OS by Marc Cheng — open source (CC0)"*.
