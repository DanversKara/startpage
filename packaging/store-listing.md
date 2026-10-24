# Store listing — DOX BOX new-tab extension

Copy for the Chrome Web Store / Edge Add-ons / Firefox Add-ons (AMO)
submission forms. The extension is the client half of a **self-hosted**
startpage: it shows the links served by your own startpage server and
keeps a cached copy so the new-tab page also works offline.

Most people never need the stores — **Load unpacked** (Brave/Chrome) or
**about:debugging** (Firefox) with a package built from the admin panel
is the normal way to run this. The listing below exists for if/when a
public store presence is wanted.

---

## Name

DOX BOX

## Summary (Chrome Web Store, max 132 characters)

Your own startpage as a desktop on every new tab — synced from your own
server, cached for offline use. No account, no tracking.

## Category

Productivity

## Description

DOX BOX turns every new tab into a clean, macOS-style desktop of *your*
links — the same startpage served by your own startpage server, on every
browser you use.

- **Your links, everywhere.** Add links once in the server's admin
  panel; every paired browser shows the same desktop, dock and app
  drawer.
- **Works offline.** The extension syncs your link set from your server
  and caches it locally, so your startpage still opens when the server
  (or the internet) is down. Links open their real destinations from the
  cache.
- **Private by architecture.** There is no cloud account and no
  third-party service. The extension talks only to the server address
  *you* enter in its Options page, using a per-device token you create
  and can revoke at any time. No analytics, no ads, no tracking — the
  developer cannot see your links because they never leave your server
  and your browser.
- **Familiar desktop.** Drag-to-arrange grid (arranged on the web page
  while signed in), dock, app drawer, and an About card — identical to
  the web startpage because it is literally the same page.

**Requirement:** a running startpage server (self-hosted, free and open
source — CC0 upstream, see the project repository). The extension alone,
without a server to pair with, shows an empty desktop.

## Single purpose statement

The extension's single purpose is to display the user's own startpage
links — served by their own startpage server — as the browser's new-tab
page, with an offline cache of that link set.

## Permission justifications

- **storage** — stores the server address, the pairing token, and the
  cached copy of the user's link set (needed for offline use).
- **alarms** — refreshes the cached link set every 30 minutes.
- **host permissions (optional, requested at pairing time)** — the
  extension must be able to reach the user's *own* server to sync. The
  server's address is entered by the user in the Options page; host
  access is requested only for that origin via the optional-permissions
  flow (`optional_host_permissions` / `optional_permissions`).

## Data collection disclosure (AMO / CWS privacy forms)

- The extension **collects nothing**. No personal data, no browsing
  history, no usage analytics is gathered, transmitted to the developer,
  or sold to anyone — there is no developer-side server at all.
- The only network traffic is between the browser and the startpage
  server the user themselves configured, authenticated with a token the
  user created on that server.
- AMO data-collection declaration in the manifest: `none`.

## Reviewer notes

- The extension requires a self-hosted startpage server to demonstrate
  full functionality. Pairing: open the extension's Options page, enter
  any startpage server URL and a sync token created in that server's
  admin panel (Tokens card), and save. Without pairing it renders an
  empty desktop and makes no network requests.
- Source code: this extension is packaged from the project's own web
  files (identical to the served website) plus the small files in
  `packaging/` (manifest, background sync worker, options page).
  `packaging/build.py` reproduces the store packages; the server's
  admin panel can also build them on demand.

## Screenshots to capture before submitting

1. A populated new-tab desktop (the DOX BOX coastal wallpaper with a
   grid of real icons).
2. The About card open (branding + profile rows).
3. The extension Options page showing the server/token pairing fields.
4. A new tab with the browser offline (to show the cached copy working).

Use fictional/demo links only in screenshots — never real internal
addresses.
