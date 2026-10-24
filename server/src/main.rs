// startpage-server — secured, server-backed Bookmark OS, multi-user edition.
//
// The upstream project was a static page whose links lived in localStorage:
// no backend, no security, and no way for links to follow the user between
// browsers. This service keeps every user's links in SQLite on the server.
//
// Model: one admin (the owner) + admin-created user accounts. Every person
// has their OWN link set (desktop/dock/apps), their own extension sync
// tokens, and their own extension cache. The public page shows the ADMIN's
// set when signed out, and the signed-in user's set when signed in. The page
// itself contains no real URLs: every anchor is an opaque /go/<id> resolved
// by this server with a 302. One of the admin's links may be a disguised
// door to the admin panel (kind='admin') — indistinguishable in the page
// source from any other tile. The only URL-revealing route besides /go is
// /api/sync, gated by per-user bearer tokens for the browser extension.

use axum::{
    body::Body,
    extract::{ConnectInfo, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use rand::Rng;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::{Path as FsPath, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod admin_page;

// ─────────────────────────────────────────────────────────────────────
// State
// ─────────────────────────────────────────────────────────────────────
struct FailInfo {
    count: u32,
    locked_until: Option<Instant>,
}

struct AppState {
    db: Mutex<Connection>,
    web_dir: PathBuf,
    packaging_dir: PathBuf,
    data_dir: PathBuf,
    attempts: Mutex<HashMap<String, FailInfo>>,
    http: reqwest::Client,
}

type Shared = Arc<AppState>;

const SEED_JSON: &str = include_str!("../seed.json");
const SESSION_COOKIE: &str = "sp_session";
const SESSION_TTL_SECS: i64 = 7 * 24 * 3600;
const MAX_LOGIN_FAILURES: u32 = 5;
const LOCKOUT: Duration = Duration::from_secs(15 * 60);
const RESERVED_PATHS: &[&str] = &[
    "api", "go", "icon", "healthz", "src", "index.html", "favicon.ico", "login", "account", "logout",
];
/// owner_id used to park the admin's links across a RESET_ADMIN password reset.
const PENDING_OWNER: &str = "__pending__";

// ─────────────────────────────────────────────────────────────────────
// Small helpers
// ─────────────────────────────────────────────────────────────────────
fn now_secs() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn random_id() -> String {
    const CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let mut rng = rand::thread_rng();
    (0..12).map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char).collect()
}

fn random_hex(bytes: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..bytes).map(|_| format!("{:02x}", rng.gen::<u8>())).collect()
}

fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    h.finalize().iter().map(|b| format!("{:02x}", b)).collect()
}

fn hash_password(pw: &str) -> Result<String, String> {
    use argon2::password_hash::{rand_core::OsRng, SaltString};
    use argon2::PasswordHasher;
    let salt = SaltString::generate(&mut OsRng);
    argon2::Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

fn verify_password(pw: &str, stored: &str) -> bool {
    use argon2::PasswordVerifier;
    if stored.is_empty() { return false; }
    match argon2::password_hash::PasswordHash::new(stored) {
        Ok(parsed) => argon2::Argon2::default().verify_password(pw.as_bytes(), &parsed).is_ok(),
        Err(_) => false,
    }
}

fn json_response(status: StatusCode, v: serde_json::Value) -> Response {
    (status, [(header::CONTENT_TYPE, "application/json")], v.to_string()).into_response()
}

fn err_json(status: StatusCode, msg: &str) -> Response {
    json_response(status, serde_json::json!({ "error": msg }))
}

fn admin_json(status: StatusCode, v: serde_json::Value) -> Response {
    let mut r = json_response(status, v);
    r.headers_mut().insert("x-robots-tag", HeaderValue::from_static("noindex, nofollow"));
    r
}

fn html_page(status: StatusCode, body: String, noindex: bool) -> Response {
    let mut r = (status, [(header::CONTENT_TYPE, "text/html; charset=utf-8")], body).into_response();
    if noindex {
        r.headers_mut().insert("x-robots-tag", HeaderValue::from_static("noindex, nofollow"));
        r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    r
}

fn redirect_to(loc: &str) -> Response {
    Response::builder().status(StatusCode::FOUND).header(header::LOCATION, loc).body(Body::empty()).unwrap()
}

// ─────────────────────────────────────────────────────────────────────
// Users + settings
// ─────────────────────────────────────────────────────────────────────
#[derive(Clone)]
struct UserRow {
    id: String,
    username: String,
    role: String,
    disabled: bool,
}

fn user_by_id(db: &Connection, id: &str) -> Option<UserRow> {
    db.query_row(
        "SELECT id, username, role, disabled FROM users WHERE id = ?1",
        params![id],
        |r| Ok(UserRow { id: r.get(0)?, username: r.get(1)?, role: r.get(2)?, disabled: r.get::<_, i64>(3)? != 0 }),
    )
    .ok()
}

fn user_by_name(db: &Connection, name: &str) -> Option<(UserRow, String)> {
    db.query_row(
        "SELECT id, username, role, disabled, password_hash FROM users WHERE username = ?1",
        params![name],
        |r| {
            Ok((
                UserRow { id: r.get(0)?, username: r.get(1)?, role: r.get(2)?, disabled: r.get::<_, i64>(3)? != 0 },
                r.get(4)?,
            ))
        },
    )
    .ok()
}

fn admin_user(db: &Connection) -> Option<UserRow> {
    db.query_row(
        "SELECT id, username, role, disabled FROM users WHERE role = 'admin' ORDER BY created_at LIMIT 1",
        [],
        |r| Ok(UserRow { id: r.get(0)?, username: r.get(1)?, role: r.get(2)?, disabled: r.get::<_, i64>(3)? != 0 }),
    )
    .ok()
}

fn get_setting(db: &Connection, key: &str) -> Option<String> {
    db.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0)).ok()
}

fn set_setting(db: &Connection, key: &str, value: &str) {
    let _ = db.execute(
        "INSERT INTO settings(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    );
}

fn admin_path(db: &Connection) -> String {
    get_setting(db, "admin_path").unwrap_or_default()
}

fn valid_username(s: &str) -> bool {
    !s.is_empty() && s.len() <= 32 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

// ─────────────────────────────────────────────────────────────────────
// Sessions
// ─────────────────────────────────────────────────────────────────────
#[derive(Clone)]
struct Session {
    token_hash: String,
    csrf: String,
    user: UserRow,
}

fn session_from_headers(st: &Shared, headers: &HeaderMap) -> Option<Session> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    let mut token = None;
    for part in cookie.split(';') {
        let part = part.trim();
        if let Some(v) = part.strip_prefix(&format!("{}=", SESSION_COOKIE)) {
            token = Some(v.to_string());
        }
    }
    let token = token?;
    let hash = sha256_hex(&token);
    let db = st.db.lock().unwrap();
    let row: Option<(String, String, i64)> = db
        .query_row("SELECT csrf, user_id, expires FROM sessions WHERE token_hash = ?1", params![hash], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .ok();
    let (csrf, user_id, expires) = row?;
    if expires <= now_secs() {
        let _ = db.execute("DELETE FROM sessions WHERE token_hash = ?1", params![hash]);
        return None;
    }
    let user = user_by_id(&db, &user_id)?;
    if user.disabled {
        // Disabled accounts lose their sessions immediately.
        let _ = db.execute("DELETE FROM sessions WHERE user_id = ?1", params![user_id]);
        return None;
    }
    Some(Session { token_hash: hash, csrf, user })
}

fn require_session(st: &Shared, headers: &HeaderMap) -> Result<Session, Response> {
    session_from_headers(st, headers)
        .ok_or_else(|| admin_json(StatusCode::UNAUTHORIZED, serde_json::json!({ "error": "unauthorized" })))
}

fn require_admin(st: &Shared, headers: &HeaderMap) -> Result<Session, Response> {
    let s = require_session(st, headers)?;
    if s.user.role != "admin" {
        return Err(admin_json(StatusCode::FORBIDDEN, serde_json::json!({ "error": "admin only" })));
    }
    Ok(s)
}

fn require_csrf(sess: &Session, headers: &HeaderMap) -> Result<(), Response> {
    let ok = headers.get("x-csrf-token").and_then(|v| v.to_str().ok()).map(|v| v == sess.csrf).unwrap_or(false);
    if ok { Ok(()) } else { Err(admin_json(StatusCode::FORBIDDEN, serde_json::json!({ "error": "bad csrf token" }))) }
}

fn is_https(headers: &HeaderMap) -> bool {
    headers.get("x-forwarded-proto").and_then(|v| v.to_str().ok()).map(|v| v.eq_ignore_ascii_case("https")).unwrap_or(false)
}

fn session_cookie(token: &str, headers: &HeaderMap) -> String {
    let mut c = format!("{}={}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}", SESSION_COOKIE, token, SESSION_TTL_SECS);
    if is_https(headers) { c.push_str("; Secure"); }
    c
}

fn clear_cookie(headers: &HeaderMap) -> String {
    let mut c = format!("{}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0", SESSION_COOKIE);
    if is_https(headers) { c.push_str("; Secure"); }
    c
}

fn create_session(st: &Shared, user_id: &str, headers: &HeaderMap) -> String {
    let token = random_hex(32);
    let csrf = random_hex(16);
    let hash = sha256_hex(&token);
    {
        let db = st.db.lock().unwrap();
        let _ = db.execute("DELETE FROM sessions WHERE expires <= ?1", params![now_secs()]);
        let _ = db.execute(
            "INSERT INTO sessions(token_hash, csrf, user_id, expires) VALUES(?1, ?2, ?3, ?4)",
            params![hash, csrf, user_id, now_secs() + SESSION_TTL_SECS],
        );
    }
    session_cookie(&token, headers)
}

// ─────────────────────────────────────────────────────────────────────
// Login rate limiting (per IP, in memory)
// ─────────────────────────────────────────────────────────────────────
fn client_ip(headers: &HeaderMap, conn: &SocketAddr) -> String {
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = xff.split(',').next() {
            let t = first.trim();
            if !t.is_empty() { return t.to_string(); }
        }
    }
    conn.ip().to_string()
}

/// Ok(()) if the attempt may proceed; Err(429) while locked out.
fn login_gate(st: &Shared, ip: &str) -> Result<(), Response> {
    let attempts = st.attempts.lock().unwrap();
    if let Some(f) = attempts.get(ip) {
        if let Some(until) = f.locked_until {
            if until > Instant::now() {
                return Err(err_json(StatusCode::TOO_MANY_REQUESTS, "too many failed logins — try again in 15 minutes"));
            }
        }
    }
    Ok(())
}

fn login_failed(st: &Shared, ip: &str) -> Response {
    let mut attempts = st.attempts.lock().unwrap();
    let f = attempts.entry(ip.to_string()).or_insert(FailInfo { count: 0, locked_until: None });
    f.count += 1;
    if f.count >= MAX_LOGIN_FAILURES {
        f.locked_until = Some(Instant::now() + LOCKOUT);
        f.count = 0;
        return err_json(StatusCode::TOO_MANY_REQUESTS, "too many failed logins — locked for 15 minutes");
    }
    err_json(StatusCode::UNAUTHORIZED, "invalid username or password")
}

fn login_succeeded(st: &Shared, ip: &str) {
    st.attempts.lock().unwrap().remove(ip);
}

// ─────────────────────────────────────────────────────────────────────
// Seed data (converted from the upstream config.js defaults)
// ─────────────────────────────────────────────────────────────────────
#[derive(Deserialize)]
struct SeedLink {
    label: String,
    url: String,
}
#[derive(Deserialize)]
struct SeedDesktop {
    label: String,
    url: String,
    col: Option<i64>,
    row: Option<i64>,
}
#[derive(Deserialize)]
struct SeedGroup {
    title: String,
    links: Vec<SeedLink>,
}
#[derive(Deserialize)]
struct Seed {
    desktop: Vec<SeedDesktop>,
    dock: Vec<SeedLink>,
    apps: Vec<SeedGroup>,
}

fn insert_link(
    db: &Connection,
    owner: &str,
    kind: &str,
    label: &str,
    url: Option<&str>,
    icon: &str,
    section: &str,
    col: Option<i64>,
    row: Option<i64>,
    sort: i64,
    group: Option<&str>,
) -> String {
    let id = random_id();
    let icon = if icon == "auto" { format!("/icon/{}", id) } else { icon.to_string() };
    let _ = db.execute(
        "INSERT INTO links(id, owner_id, kind, label, url, icon, section, col, row, sort, group_title) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![id, owner, kind, label, url, icon, section, col, row, sort, group],
    );
    id
}

/// Seed the default link set for `owner` (used for the first admin account).
fn seed_links_for(db: &Connection, owner: &str) {
    let seed: Seed = match serde_json::from_str(SEED_JSON) {
        Ok(s) => s,
        Err(e) => { eprintln!("seed.json parse failed: {e}"); return; }
    };
    for (i, d) in seed.desktop.iter().enumerate() {
        insert_link(db, owner, "url", &d.label, Some(&d.url), "auto", "desktop", d.col, d.row, i as i64, None);
    }
    for (i, d) in seed.dock.iter().enumerate() {
        insert_link(db, owner, "url", &d.label, Some(&d.url), "auto", "dock", None, None, i as i64, None);
    }
    let mut sort = 0i64;
    for g in &seed.apps {
        for l in &g.links {
            insert_link(db, owner, "url", &l.label, Some(&l.url), "auto", "apps", None, None, sort, Some(g.title.as_str()));
            sort += 1;
        }
    }
}

/// Delete a user and everything they own (links, favicons, tokens, sessions).
fn delete_user_cascade(db: &Connection, user_id: &str) {
    let _ = db.execute(
        "DELETE FROM favicons WHERE link_id IN (SELECT id FROM links WHERE owner_id = ?1)",
        params![user_id],
    );
    let _ = db.execute("DELETE FROM links WHERE owner_id = ?1", params![user_id]);
    let _ = db.execute("DELETE FROM sync_tokens WHERE user_id = ?1", params![user_id]);
    let _ = db.execute("DELETE FROM sessions WHERE user_id = ?1", params![user_id]);
    let _ = db.execute("DELETE FROM users WHERE id = ?1", params![user_id]);
}

// ─────────────────────────────────────────────────────────────────────
// Link queries shared by /api/state and /api/sync
// ─────────────────────────────────────────────────────────────────────
struct LinkRow {
    id: String,
    kind: String,
    label: String,
    url: Option<String>,
    icon: String,
    section: String,
    col: Option<i64>,
    row: Option<i64>,
    group: Option<String>,
    folder_id: Option<String>,
    icon_bg: Option<String>,
}

fn links_of(db: &Connection, owner: &str) -> Vec<LinkRow> {
    let mut stmt = match db.prepare(
        "SELECT id, kind, label, url, icon, section, col, row, group_title, folder_id, icon_bg FROM links WHERE owner_id = ?1 ORDER BY sort, row, col",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mapped = stmt.query_map(params![owner], |r| {
        Ok(LinkRow {
            id: r.get(0)?,
            kind: r.get(1)?,
            label: r.get(2)?,
            url: r.get(3)?,
            icon: r.get(4)?,
            section: r.get(5)?,
            col: r.get(6)?,
            row: r.get(7)?,
            group: r.get(8)?,
            folder_id: r.get(9)?,
            icon_bg: r.get(10)?,
        })
    });
    match mapped {
        Ok(q) => q.filter_map(|x| x.ok()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Whose link set a page view shows: the signed-in user's own set, otherwise
/// the admin's (the public face).
/// Whose link set the PUBLIC (signed-out) startpage shows. The admin
/// picks it in Settings → Public startpage; the stored choice wins
/// while that account exists and is enabled, otherwise the earliest
/// admin — so a stray second admin account can never hijack the
/// homepage (v1.17.0).
fn default_owner(db: &Connection) -> Option<String> {
    if let Some(id) = get_setting(db, "default_owner") {
        if let Some(u) = user_by_id(db, &id) {
            if !u.disabled {
                return Some(u.id);
            }
        }
    }
    admin_user(db).map(|u| u.id)
}

fn viewer_owner(st: &Shared, headers: &HeaderMap) -> Option<String> {
    if let Some(sess) = session_from_headers(st, headers) {
        return Some(sess.user.id);
    }
    let db = st.db.lock().unwrap();
    default_owner(&db)
}

// ─────────────────────────────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────────────────────────────
async fn api_state(State(st): State<Shared>, headers: HeaderMap) -> Response {
    let owner = viewer_owner(&st, &headers);
    let mut desktop = Vec::new();
    let mut dock = Vec::new();
    let mut apps: Vec<(String, Vec<serde_json::Value>)> = Vec::new();
    let mut about: Vec<(String, Vec<serde_json::Value>)> = Vec::new();
    let mut branding = default_branding();
    if let Some(owner) = owner {
        let db = st.db.lock().unwrap();
        branding = get_branding(&db);
        let rows = links_of(&db, &owner);
        // Folder members ride inside their folder's entry, not the top level.
        let mut members: std::collections::HashMap<String, Vec<serde_json::Value>> = std::collections::HashMap::new();
        for l in &rows {
            if l.section == "desktop" {
                if let Some(fid) = &l.folder_id {
                    members.entry(fid.clone()).or_default().push(serde_json::json!({
                        "id": l.id, "label": l.label, "icon": l.icon, "icon_bg": l.icon_bg
                    }));
                }
            }
        }
        for l in &rows {
            let entry = serde_json::json!({ "id": l.id, "label": l.label, "icon": l.icon, "icon_bg": l.icon_bg });
            match l.section.as_str() {
                "desktop" => {
                    if l.folder_id.is_some() { continue; }
                    if l.kind == "folder" {
                        desktop.push(serde_json::json!({
                            "id": l.id, "label": l.label, "icon": l.icon, "icon_bg": l.icon_bg, "col": l.col, "row": l.row,
                            "folder": true,
                            "members": members.get(&l.id).cloned().unwrap_or_default()
                        }));
                    } else {
                        desktop.push(serde_json::json!({
                            "id": l.id, "label": l.label, "icon": l.icon, "icon_bg": l.icon_bg, "col": l.col, "row": l.row
                        }));
                    }
                }
                "dock" => dock.push(entry),
                "about" => {
                    let title = l.group.clone().unwrap_or_else(|| "Links".to_string());
                    match about.iter_mut().find(|(t, _)| *t == title) {
                        Some((_, links)) => links.push(entry),
                        None => about.push((title, vec![entry])),
                    }
                }
                _ => {
                    let title = l.group.clone().unwrap_or_else(|| "Apps".to_string());
                    match apps.iter_mut().find(|(t, _)| *t == title) {
                        Some((_, links)) => links.push(entry),
                        None => apps.push((title, vec![entry])),
                    }
                }
            }
        }
    }
    let group = |v: Vec<(String, Vec<serde_json::Value>)>| -> Vec<serde_json::Value> {
        v.into_iter().map(|(title, links)| serde_json::json!({ "title": title, "links": links })).collect()
    };
    json_response(StatusCode::OK, serde_json::json!({
        "desktop": desktop, "dock": dock, "apps": group(apps), "about": group(about), "branding": branding
    }))
}

async fn api_whoami(State(st): State<Shared>, headers: HeaderMap) -> Response {
    let open = {
        let db = st.db.lock().unwrap();
        registration_open(&db)
    };
    match session_from_headers(&st, &headers) {
        Some(s) => json_response(
            StatusCode::OK,
            serde_json::json!({ "user": { "username": s.user.username, "role": s.user.role }, "csrf": s.csrf, "registration_open": open }),
        ),
        None => json_response(StatusCode::OK, serde_json::json!({ "user": null, "registration_open": open })),
    }
}

#[derive(Deserialize)]
struct MoveInput {
    id: String,
    // Optional: join requests ({id, folder_id}) legitimately carry no cell.
    col: Option<i64>,
    row: Option<i64>,
    swap_with: Option<String>,
    swap_col: Option<i64>,
    swap_row: Option<i64>,
    folder_id: Option<String>,
}

/// After a member leaves a folder: a folder with one member left dissolves
/// (the member takes the folder's cell) and an empty folder deletes itself.
fn dissolve_check(db: &Connection, folder_id: &str) {
    let folder = match link_full(db, folder_id) {
        Some(f) if f.kind == "folder" => f,
        _ => return,
    };
    let mut stmt = match db.prepare("SELECT id FROM links WHERE folder_id = ?1") {
        Ok(s) => s,
        Err(_) => return,
    };
    let ids: Vec<String> = stmt
        .query_map(params![folder_id], |r| r.get::<_, String>(0))
        .map(|q| q.filter_map(|x| x.ok()).collect())
        .unwrap_or_default();
    drop(stmt);
    match ids.len() {
        0 => {
            let _ = db.execute("DELETE FROM links WHERE id = ?1", params![folder_id]);
            let _ = db.execute("DELETE FROM favicons WHERE link_id = ?1", params![folder_id]);
        }
        1 => {
            let _ = db.execute(
                "UPDATE links SET folder_id = NULL, col = ?1, row = ?2 WHERE id = ?3",
                params![folder.col, folder.row, ids[0]],
            );
            let _ = db.execute("DELETE FROM links WHERE id = ?1", params![folder_id]);
            let _ = db.execute("DELETE FROM favicons WHERE link_id = ?1", params![folder_id]);
        }
        _ => {}
    }
}

/// POST /api/links/move — the signed-in owner rearranges their own desktop.
/// This is the one page-side mutation: it needs the session AND its CSRF
/// token, only ever touches the caller's own desktop links, and changes
/// nothing but grid positions (swapping with the cell's occupant when the
/// client reports one).
async fn api_links_move(State(st): State<Shared>, headers: HeaderMap, Json(input): Json<MoveInput>) -> Response {
    let sess = match session_from_headers(&st, &headers) {
        Some(s) => s,
        None => return json_response(StatusCode::UNAUTHORIZED, serde_json::json!({ "error": "sign in first" })),
    };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    let col = input.col.unwrap_or(0).clamp(0, 64);
    let row = input.row.unwrap_or(0).clamp(0, 200);
    let db = st.db.lock().unwrap();
    let link = match link_full(&db, &input.id) {
        Some(l) => l,
        None => return json_response(StatusCode::NOT_FOUND, serde_json::json!({ "error": "no such link" })),
    };
    if link.owner_id != sess.user.id || link.section != "desktop" {
        return json_response(StatusCode::FORBIDDEN, serde_json::json!({ "error": "not your desktop link" }));
    }
    // Join a folder: the link leaves the grid and becomes a member.
    if let Some(fid) = &input.folder_id {
        if *fid == input.id || link.kind == "folder" {
            return json_response(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "folders cannot go in folders" }));
        }
        match link_full(&db, fid) {
            Some(f) if f.owner_id == sess.user.id && f.kind == "folder" && f.section == "desktop" => {
                let _ = db.execute("UPDATE links SET folder_id = ?1, col = NULL, row = NULL WHERE id = ?2", params![fid, input.id]);
                return json_response(StatusCode::OK, serde_json::json!({ "ok": true }));
            }
            _ => return json_response(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "no such folder" })),
        }
    }
    let old_folder = link.folder_id.clone();
    // Swap: prefer the occupant the client saw at the target cell (its model
    // includes render-assigned positions the DB may not have yet); fall back
    // to whoever the DB has parked there.
    let mut swapped = false;
    if let Some(other_id) = &input.swap_with {
        if *other_id != input.id {
            if let Some(other) = link_full(&db, other_id) {
                if other.owner_id == sess.user.id && other.section == "desktop" {
                    let (oc, orow) = (input.swap_col.unwrap_or(0).clamp(0, 64), input.swap_row.unwrap_or(0).clamp(0, 200));
                    let _ = db.execute("UPDATE links SET col = ?1, row = ?2 WHERE id = ?3", params![oc, orow, other_id]);
                    swapped = true;
                }
            }
        }
    }
    if !swapped {
        let occupant: Option<String> = db.query_row(
            "SELECT id FROM links WHERE owner_id = ?1 AND section = 'desktop' AND col = ?2 AND row = ?3 AND id != ?4",
            params![sess.user.id, col, row, input.id], |r| r.get(0)).ok();
        if let Some(occ) = occupant {
            match (link.col, link.row) {
                (Some(c), Some(r)) => { let _ = db.execute("UPDATE links SET col = ?1, row = ?2 WHERE id = ?3", params![c, r, occ]); }
                _ => { let _ = db.execute("UPDATE links SET col = NULL, row = NULL WHERE id = ?1", params![occ]); }
            }
        }
    }
    let _ = db.execute("UPDATE links SET folder_id = NULL, col = ?1, row = ?2 WHERE id = ?3", params![col, row, input.id]);
    if let Some(fid) = old_folder {
        dissolve_check(&db, &fid);
    }
    json_response(StatusCode::OK, serde_json::json!({ "ok": true }))
}

#[derive(Deserialize)]
struct FolderCreateReq {
    a: String,
    b: String,
    col: i64,
    row: i64,
}

/// POST /api/links/folder — bundle two desktop links into a new folder at
/// the target's cell (the Android gesture: drag one icon onto another).
async fn api_links_folder(State(st): State<Shared>, headers: HeaderMap, Json(input): Json<FolderCreateReq>) -> Response {
    let sess = match session_from_headers(&st, &headers) {
        Some(s) => s,
        None => return json_response(StatusCode::UNAUTHORIZED, serde_json::json!({ "error": "sign in first" })),
    };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    if input.a == input.b {
        return json_response(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "need two different links" }));
    }
    let db = st.db.lock().unwrap();
    for id in [&input.a, &input.b] {
        match link_full(&db, id) {
            Some(l) if l.owner_id == sess.user.id && l.section == "desktop" && l.kind == "url" && l.folder_id.is_none() => {}
            _ => return json_response(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "both links must be ungrouped desktop links of yours" })),
        }
    }
    let col = input.col.clamp(0, 64);
    let row = input.row.clamp(0, 200);
    let fid = insert_link(&db, &sess.user.id, "folder", "Folder", None, "src/icons/globe.svg", "desktop", Some(col), Some(row), 0, None);
    let _ = db.execute("UPDATE links SET folder_id = ?1 WHERE id IN (?2, ?3)", params![fid, input.a, input.b]);
    json_response(StatusCode::OK, serde_json::json!({ "ok": true, "folder_id": fid }))
}

#[derive(Deserialize)]
struct FolderIdReq {
    id: String,
    label: Option<String>,
}

/// POST /api/links/folder-rename — rename one of your folders.
async fn api_links_folder_rename(State(st): State<Shared>, headers: HeaderMap, Json(input): Json<FolderIdReq>) -> Response {
    let sess = match session_from_headers(&st, &headers) {
        Some(s) => s,
        None => return json_response(StatusCode::UNAUTHORIZED, serde_json::json!({ "error": "sign in first" })),
    };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    let label = input.label.as_deref().unwrap_or("").trim().to_string();
    if label.is_empty() || label.chars().count() > 80 {
        return json_response(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "label must be 1-80 characters" }));
    }
    let db = st.db.lock().unwrap();
    match link_full(&db, &input.id) {
        Some(l) if l.owner_id == sess.user.id && l.kind == "folder" => {
            let _ = db.execute("UPDATE links SET label = ?1 WHERE id = ?2", params![label, input.id]);
            json_response(StatusCode::OK, serde_json::json!({ "ok": true }))
        }
        _ => json_response(StatusCode::NOT_FOUND, serde_json::json!({ "error": "no such folder" })),
    }
}

/// POST /api/links/folder-delete — remove a folder; its members return to
/// the desktop unpositioned (the client deals them into free cells).
async fn api_links_folder_delete(State(st): State<Shared>, headers: HeaderMap, Json(input): Json<FolderIdReq>) -> Response {
    let sess = match session_from_headers(&st, &headers) {
        Some(s) => s,
        None => return json_response(StatusCode::UNAUTHORIZED, serde_json::json!({ "error": "sign in first" })),
    };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    let db = st.db.lock().unwrap();
    match link_full(&db, &input.id) {
        Some(l) if l.owner_id == sess.user.id && l.kind == "folder" => {
            let _ = db.execute("UPDATE links SET folder_id = NULL WHERE folder_id = ?1", params![input.id]);
            let _ = db.execute("DELETE FROM links WHERE id = ?1", params![input.id]);
            let _ = db.execute("DELETE FROM favicons WHERE link_id = ?1", params![input.id]);
            json_response(StatusCode::OK, serde_json::json!({ "ok": true }))
        }
        _ => json_response(StatusCode::NOT_FOUND, serde_json::json!({ "error": "no such folder" })),
    }
}

/// /go/<id> — resolve only links owned by the admin (public set) or by the
/// currently signed-in user; everything else is the same 404 as a bad id.
/// The container-link handoff page (shared by /go and /s/<token>/go).
fn container_handoff_response(label: &str, t: &str) -> Response {
        let inner = container_inner_url(&t).unwrap_or_default();
        let target_js = serde_json::to_string(&t).unwrap_or_else(|_| "\"\"".to_string());
        // Auto-return: a page can't truly confirm a protocol handoff,
        // but when the handler takes over, this tab loses focus or
        // hides (prompt shown / new container tab focused) — treat
        // that as "worked", then close (script-opened tabs), go back,
        // or land on the start page. If nothing stirs in 10 s, return
        // anyway. Placeholders are swapped with .replace (not
        // format!) so the script's braces need no escaping.
        let html = r#"<!doctype html><html><head><meta charset="utf-8"><title>Opening __LABEL__…</title>
<meta name="referrer" content="no-referrer"><meta name="viewport" content="width=device-width, initial-scale=1"></head>
<body style="font-family:system-ui,sans-serif;background:#10121a;color:#e8eaf2;display:flex;align-items:center;justify-content:center;min-height:100vh;margin:0">
<div style="text-align:center;max-width:34em;padding:2em">
<p id="msg" style="font-size:1.1em">Opening <b>__LABEL__</b> in its container…</p>
<p><a id="openBtn" href="__TARGET__" style="display:inline-block;background:#2f6bff;color:#fff;padding:.7em 1.4em;border-radius:12px;text-decoration:none;font-weight:600">Open in container</a></p>
<p style="opacity:.75;font-size:.92em">Nothing happening? The container-link handler isn't installed or hasn't been allowed on this device yet.<br>
<a href="__INNER__" style="color:#9db9ff">Open the site without a container</a> instead, or <a href="/" style="color:#9db9ff">return to your start page</a>.<br>
<span style="opacity:.7">This page returns on its own once the handoff lands, or after 10 seconds.</span></p>
</div>
<script>
var handedOff = false, doneFlag = false, t10 = null;
function done() {
  if (doneFlag) return; doneFlag = true;
  var m = document.getElementById('msg'); if (m) m.textContent = 'Returning to your start page…';
  window.close();
  setTimeout(function () { if (history.length > 1) history.back(); else location.replace('/'); }, 350);
}
function worked() {
  handedOff = true;
  var m = document.getElementById('msg'); if (m) m.textContent = '✓ Handed off — returning shortly.';
}
document.addEventListener('visibilitychange', function () {
  if (document.hidden) worked(); else if (handedOff) setTimeout(done, 800);
});
window.addEventListener('blur', worked);
window.addEventListener('focus', function () { if (handedOff) setTimeout(done, 800); });
function arm() { clearTimeout(t10); t10 = setTimeout(done, 10000); }
document.getElementById('openBtn').addEventListener('click', function () { handedOff = false; arm(); });
arm();
location.replace(__TARGET_JS__);
</script></body></html>"#
            .replace("__LABEL__", &html_escape(&label))
            .replace("__TARGET__", &html_escape(&t))
            .replace("__INNER__", &html_escape(&inner))
            .replace("__TARGET_JS__", &target_js);
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .header(header::REFERRER_POLICY, "no-referrer")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(html))
            .unwrap()
}

async fn go(State(st): State<Shared>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    // Resolve the session BEFORE taking the db lock (session lookup locks too).
    let session_uid = session_from_headers(&st, &headers).map(|s| s.user.id);
    let (target, label) = {
        let db = st.db.lock().unwrap();
        let row: Option<(String, String, Option<String>, String)> = db
            .query_row("SELECT owner_id, kind, url, label FROM links WHERE id = ?1", params![id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .ok();
        let admin_id = admin_user(&db).map(|u| u.id);
        let public_id = default_owner(&db);
        match row {
            Some((owner, kind, url, label)) => {
                let allowed = Some(&owner) == public_id.as_ref() || Some(&owner) == session_uid.as_ref();
                if !allowed {
                    (None, String::new())
                } else if kind == "admin" {
                    (Some(format!("/{}", admin_path(&db))), label)
                } else {
                    (url, label)
                }
            }
            None => (None, String::new()),
        }
    };
    match target {
        // Container links can't be a plain redirect — a cross-scheme 302
        // is unreliable. Land on a tiny page that hands the URL to the
        // OS protocol handler (with a real clickable button + a plain
        // fallback to the site itself if no handler is installed).
        Some(t) if is_container_url(&t) => container_handoff_response(&label, &t),
        Some(t) => Response::builder()
            .status(StatusCode::FOUND)
            .header(header::LOCATION, t)
            .header(header::REFERRER_POLICY, "no-referrer")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::empty())
            .unwrap(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

// ─────────────────────────────────────────────────────────────────────
// Favicons — fetched and cached by the SERVER, never the client
// ─────────────────────────────────────────────────────────────────────
fn looks_like_image(bytes: &[u8], ctype: &str) -> bool {
    if bytes.len() < 16 { return false; }
    if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) { return true; } // ICO
    if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) { return true; } // PNG
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) { return true; }       // JPEG
    if bytes.starts_with(b"GIF8") { return true; }
    if bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml") { return true; }
    ctype.starts_with("image/")
}

/// Last-resort icon: a bundled file named after the link's host —
/// src/icons/<host>.png (then .svg/.ico/.jpg/.webp), with a parent-domain
/// fallback for subdomains (chat.example.com -> example.com). Files dropped
/// into the icons folder by hand work exactly like shipped ones, and they
/// also appear in the panel's Bundled-icon dropdown.
fn bundled_icon_for(st: &Shared, u: &reqwest::Url) -> Option<(String, Vec<u8>)> {
    let host = u.host_str()?;
    let mut names = vec![host.to_string()];
    if let Some(h) = host.strip_prefix("www.") { names.push(h.to_string()); }
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() > 2 { names.push(parts[1..].join(".")); }
    for name in names {
        for (ext, ct) in [("png", "image/png"), ("svg", "image/svg+xml"), ("ico", "image/x-icon"), ("jpg", "image/jpeg"), ("jpeg", "image/jpeg"), ("webp", "image/webp")] {
            let p = st.web_dir.join(format!("src/icons/{}.{}", name, ext));
            if let Ok(bytes) = std::fs::read(&p) {
                if looks_like_image(&bytes, ct) { return Some((ct.to_string(), bytes)); }
            }
        }
    }
    None
}

async fn fetch_favicon(st: &Shared, page_url: &str) -> Option<(String, Vec<u8>)> {
    // Container links fetch the favicon of the site inside them.
    let inner = container_inner_url(page_url);
    let page_url = inner.as_deref().unwrap_or(page_url);
    let u = reqwest::Url::parse(page_url).ok()?;
    let mut origin = format!("{}://{}", u.scheme(), u.host_str()?);
    if let Some(p) = u.port() { origin = format!("{}:{}", origin, p); }
    for (path, fallback_ct) in [
        ("/favicon.ico", "image/x-icon"),
        ("/favicon.png", "image/png"),
        ("/apple-touch-icon.png", "image/png"),
    ] {
        let url = format!("{}{}", origin, path);
        let resp = match st.http.get(&url).send().await {
            Ok(r) if r.status().is_success() => r,
            _ => continue,
        };
        let ctype = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(';').next().unwrap_or("").trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| fallback_ct.to_string());
        let bytes = match resp.bytes().await {
            Ok(b) => b.to_vec(),
            Err(_) => continue,
        };
        if looks_like_image(&bytes, &ctype) {
            return Some((ctype, bytes));
        }
    }
    bundled_icon_for(st, &u)
}

async fn refresh_favicon(st: &Shared, id: &str, url: &str) -> bool {
    match fetch_favicon(st, url).await {
        Some((ct, bytes)) => {
            let db = st.db.lock().unwrap();
            let _ = db.execute(
                "INSERT INTO favicons(link_id, content_type, bytes) VALUES(?1, ?2, ?3) ON CONFLICT(link_id) DO UPDATE SET content_type = excluded.content_type, bytes = excluded.bytes",
                params![id, ct, bytes],
            );
            true
        }
        None => false,
    }
}


/// (id, url) pairs for auto-icon links with no cached favicon yet.
/// `owner` = None scans every user (used once after first setup).
fn faviconless_auto_links(db: &Connection, owner: Option<&str>) -> Vec<(String, String)> {
    let base = "SELECT l.id, l.url FROM links l LEFT JOIN favicons f ON f.link_id = l.id WHERE l.kind = 'url' AND l.url IS NOT NULL AND (l.icon = 'auto' OR l.icon LIKE '/icon/%') AND f.link_id IS NULL";
    let mut out = Vec::new();
    if let Some(o) = owner {
        if let Ok(mut stmt) = db.prepare(&format!("{} AND l.owner_id = ?1", base)) {
            if let Ok(q) = stmt.query_map(params![o], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))) {
                out = q.filter_map(|x| x.ok()).collect();
            }
        }
    } else if let Ok(mut stmt) = db.prepare(base) {
        if let Ok(q) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))) {
            out = q.filter_map(|x| x.ok()).collect();
        }
    }
    out
}

/// Fetch favicons off the request path — page loads and admin actions must
/// never wait on a third-party origin's favicon.
fn warm_favicons(st: &Shared, pairs: Vec<(String, String)>) {
    if pairs.is_empty() { return; }
    // Bounded concurrency: a dead or slow origin must not queue-block every
    // icon behind it (a fully sequential pass over a mixed LAN/public set
    // stalls for minutes and dies half-done at the next restart), but an
    // unbounded burst trips over proxies and rate limits. Four at a time.
    let st = st.clone();
    tokio::spawn(async move {
        let mut it = pairs.into_iter();
        loop {
            let mut handles = Vec::new();
            for _ in 0..4 {
                match it.next() {
                    Some((id, url)) => {
                        let st = st.clone();
                        handles.push(tokio::spawn(async move { refresh_favicon(&st, &id, &url).await; }));
                    }
                    None => break,
                }
            }
            if handles.is_empty() { break; }
            for h in handles { let _ = h.await; }
        }
    });
}

async fn icon(State(st): State<Shared>, Path(id): Path<String>) -> Response {
    {
        let db = st.db.lock().unwrap();
        let cached: Option<(String, Vec<u8>)> = db
            .query_row("SELECT content_type, bytes FROM favicons WHERE link_id = ?1", params![id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .ok();
        if let Some((ct, bytes)) = cached {
            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, ct)
                .header(header::CACHE_CONTROL, "public, max-age=604800, immutable")
                .body(Body::from(bytes))
                .unwrap();
        }
    }
    let url: Option<String> = {
        let db = st.db.lock().unwrap();
        db.query_row("SELECT url FROM links WHERE id = ?1 AND kind = 'url'", params![id], |r| r.get(0)).ok()
    };
    match url {
        Some(u) => {
            // Never block a tile on the live internet: serve the globe now
            // and warm the real favicon in the background — the next load
            // (which the browser also caches immutably) gets the real icon.
            let st2 = st.clone();
            tokio::spawn(async move { refresh_favicon(&st2, &id, &u).await; });
            let globe = st.web_dir.join("src/icons/globe.svg");
            match std::fs::read(&globe) {
                Ok(bytes) => Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "image/svg+xml")
                    .header(header::CACHE_CONTROL, "public, max-age=60")
                    .body(Body::from(bytes))
                    .unwrap(),
                Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
            }
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

// ─────────────────────────────────────────────────────────────────────
// Extension sync — the ONLY token-gated, URL-revealing route
// ─────────────────────────────────────────────────────────────────────
fn sync_payload(db: &Connection, owner: &str) -> serde_json::Value {
    let apath = admin_path(db);
    let mut desktop = Vec::new();
    let mut dock = Vec::new();
    let mut apps: Vec<(String, Vec<serde_json::Value>)> = Vec::new();
    let mut about: Vec<(String, Vec<serde_json::Value>)> = Vec::new();
    let rows = links_of(db, owner);
    // Folder members ride inside their folder's entry, not the top level.
    let mut members: std::collections::HashMap<String, Vec<serde_json::Value>> = std::collections::HashMap::new();
    for l in &rows {
        if l.section == "desktop" {
            if let Some(fid) = &l.folder_id {
                let (url, is_admin) = if l.kind == "admin" {
                    (format!("/{}", apath), true)
                } else {
                    (l.url.clone().unwrap_or_default(), false)
                };
                let mut m = serde_json::json!({ "label": l.label, "url": url, "icon": l.icon, "icon_bg": l.icon_bg });
                if is_admin { m["admin"] = serde_json::json!(true); }
                members.entry(fid.clone()).or_default().push(m);
            }
        }
    }
    for l in &rows {
        let (url, is_admin) = if l.kind == "admin" {
            (format!("/{}", apath), true)
        } else {
            (l.url.clone().unwrap_or_default(), false)
        };
        let mut entry = serde_json::json!({ "label": l.label, "url": url, "icon": l.icon, "icon_bg": l.icon_bg });
        if is_admin { entry["admin"] = serde_json::json!(true); }
        match l.section.as_str() {
            "desktop" => {
                if l.folder_id.is_some() { continue; }
                entry["col"] = serde_json::json!(l.col);
                entry["row"] = serde_json::json!(l.row);
                if l.kind == "folder" {
                    entry["folder"] = serde_json::json!(true);
                    entry["members"] = serde_json::json!(members.get(&l.id).cloned().unwrap_or_default());
                }
                desktop.push(entry);
            }
            "dock" => dock.push(entry),
            "about" => {
                let title = l.group.clone().unwrap_or_else(|| "Links".to_string());
                match about.iter_mut().find(|(t, _)| *t == title) {
                    Some((_, links)) => links.push(entry),
                    None => about.push((title, vec![entry])),
                }
            }
            _ => {
                let title = l.group.clone().unwrap_or_else(|| "Apps".to_string());
                match apps.iter_mut().find(|(t, _)| *t == title) {
                    Some((_, links)) => links.push(entry),
                    None => apps.push((title, vec![entry])),
                }
            }
        }
    }
    let group = |v: Vec<(String, Vec<serde_json::Value>)>| -> Vec<serde_json::Value> {
        v.into_iter().map(|(title, links)| serde_json::json!({ "title": title, "links": links })).collect()
    };
    // Version = hash of the canonical payload (without the version field).
    let body = serde_json::json!({
        "desktop": desktop, "dock": dock, "apps": group(apps), "about": group(about), "branding": get_branding(db)
    });
    let version = sha256_hex(&body.to_string());
    let mut full = body;
    full["version"] = serde_json::json!(version);
    full
}

async fn api_sync(State(st): State<Shared>, headers: HeaderMap) -> Response {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string());
    let token = match token {
        Some(t) if !t.is_empty() => t,
        _ => return err_json(StatusCode::UNAUTHORIZED, "sync token required"),
    };
    let hash = sha256_hex(&token);
    let (payload, etag) = {
        let db = st.db.lock().unwrap();
        let row: Option<(String, String)> = db
            .query_row("SELECT id, user_id FROM sync_tokens WHERE token_hash = ?1", params![hash], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .ok();
        let (token_id, user_id) = match row {
            Some(r) => r,
            None => return err_json(StatusCode::UNAUTHORIZED, "invalid sync token"),
        };
        match user_by_id(&db, &user_id) {
            Some(u) if !u.disabled => {}
            _ => return err_json(StatusCode::UNAUTHORIZED, "invalid sync token"),
        }
        let _ = db.execute("UPDATE sync_tokens SET last_used_at = ?1 WHERE id = ?2", params![now_secs(), token_id]);
        let payload = sync_payload(&db, &user_id);
        let etag = format!("\"{}\"", payload["version"].as_str().unwrap_or(""));
        (payload, etag)
    };
    if let Some(inm) = headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) {
        if inm.trim() == etag {
            return Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .header(header::ETAG, etag)
                .body(Body::empty())
                .unwrap();
        }
    }
    let mut r = json_response(StatusCode::OK, payload);
    r.headers_mut().insert(header::ETAG, HeaderValue::from_str(&etag).unwrap());
    r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

// ─────────────────────────────────────────────────────────────────────
// Branding (About card) — stored server-side in settings, defaults are the
// upstream original so the card starts exactly as Marc Cheng shipped it.
// ─────────────────────────────────────────────────────────────────────
fn default_branding() -> serde_json::Value {
    serde_json::json!({
        "product_name": "Bookmark OS",
        "tagline": "Your bookmarks as a desktop — from any browser, no account needed.",
        "about_note": "Hi there, I’m the developer who built this lightweight web OS. The idea was simple: to access my bookmarks using ANY browser on ANY computer, without logging in to a stupid FUCKING account. To learn more about me, you’re welcome to click one of the icons below.",
        "profile_name": "Marc Cheng",
        "profile_sub": "Zhongmang Cheng",
        "profile_rows": [
            { "label": "Industry", "value": "marccheng@outlook.com" },
            { "label": "Research", "value": "marc.cheng@alumni.utoronto.ca" }
        ],
        "app_icon": "src/app-icon.svg",
        "chrome_url": "",
        "edge_url": "",
        "firefox_url": ""
    })
}

fn get_branding(db: &Connection) -> serde_json::Value {
    let mut out = default_branding();
    if let Some(raw) = get_setting(db, "branding") {
        if let Ok(stored) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let (Some(o), Some(s)) = (out.as_object_mut(), stored.as_object()) {
                for (k, v) in s {
                    if k == "profile_rows" {
                        if v.is_array() { o.insert(k.clone(), v.clone()); }
                    } else if v.is_string() {
                        o.insert(k.clone(), v.clone());
                    }
                }
            }
        }
    }
    out
}

/// The upstream developer's About icon rows, seeded as the admin's
/// section='about' links (group_title = row title, bundled social icons).
const ABOUT_SEED: &[(&str, &str, &str, &str)] = &[
    // (group, label, url, icon)
    ("Career & Research", "LinkedIn", "https://www.linkedin.com/in/chengmarc/", "src/social/linkedin.svg"),
    ("Career & Research", "GitHub", "https://github.com/chengmarc", "src/social/github.svg"),
    ("Career & Research", "ORCID", "https://orcid.org/0009-0008-1783-514X", "src/social/orcid.svg"),
    ("Career & Research", "ResearchGate", "https://www.researchgate.net/profile/Zhongmang-Cheng", "src/social/researchgate.svg"),
    ("Life", "Instagram", "https://www.instagram.com/marcc.cheng/", "src/social/instagram.svg"),
    ("Life", "Bilibili", "https://space.bilibili.com/1351785", "src/social/bilibili.svg"),
    ("Life", "Xiaohongshu", "https://www.xiaohongshu.com/user/profile/5fea4e03000000000100ad52", "src/social/xiaohongshu.svg"),
    ("Life", "WeChat", "/src/social/wechat.jpg", "src/social/wechat.svg"),
    ("Crypto", "X", "https://x.com/chengmarc_", "src/social/x.svg"),
    ("Crypto", "Farcaster", "https://farcaster.xyz/chengmarc", "src/social/farcaster.svg"),
    ("Crypto", "Paragraph", "https://paragraph.com/@chengmarc", "src/social/paragraph.png"),
    ("Gaming", "Steam", "https://steamcommunity.com/id/everwith/", "src/social/steam.svg"),
    ("Gaming", "Discord", "https://discord.gg/PGE98Tu5j4", "src/social/discord.svg"),
];

fn seed_about_links(db: &Connection, owner: &str) {
    for (i, (group, label, url, icon)) in ABOUT_SEED.iter().enumerate() {
        insert_link(db, owner, "url", label, Some(url), icon, "about", None, None, i as i64, Some(group));
    }
}

// ─────────────────────────────────────────────────────────────────────
// Auth pages: obscure-path admin entry, setup, logins, account
// ─────────────────────────────────────────────────────────────────────
async fn admin_entry(State(st): State<Shared>, headers: HeaderMap, Path(seg): Path<String>) -> Response {
    let (apath, has_admin) = {
        let db = st.db.lock().unwrap();
        (admin_path(&db), admin_user(&db).is_some())
    };
    if seg != apath {
        // Single-segment static file (e.g. /index.html) or a plain 404.
        return match serve_static(&st, &seg) {
            Some(r) => r,
            None => (StatusCode::NOT_FOUND, "not found").into_response(),
        };
    }
    if !has_admin {
        return html_page(StatusCode::OK, admin_page::setup_page(), true);
    }
    match session_from_headers(&st, &headers) {
        Some(s) if s.user.role == "admin" => html_page(StatusCode::OK, admin_page::panel_page(), true),
        Some(_) => html_page(StatusCode::FORBIDDEN, admin_page::forbidden_page(), true),
        None => html_page(StatusCode::OK, admin_page::login_page(true), true),
    }
}

#[derive(Deserialize)]
struct SetupReq {
    username: String,
    password: String,
}

async fn admin_setup(State(st): State<Shared>, headers: HeaderMap, Path(seg): Path<String>, Json(body): Json<SetupReq>) -> Response {
    let mut resp = {
        let db = st.db.lock().unwrap();
        if seg != admin_path(&db) {
            return (StatusCode::NOT_FOUND, "not found").into_response();
        }
        if admin_user(&db).is_some() {
            return admin_json(StatusCode::FORBIDDEN, serde_json::json!({ "error": "setup already completed" }));
        }
        if !valid_username(body.username.trim()) {
            return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "invalid username" }));
        }
        if body.password.len() < 10 {
            return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "password must be at least 10 characters" }));
        }
        let hash = match hash_password(&body.password) {
            Ok(h) => h,
            Err(_) => return admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "hashing failed" })),
        };
        let uid = random_id();
        if db
            .execute(
                "INSERT INTO users(id, username, password_hash, role, disabled, created_at) VALUES(?1, ?2, ?3, 'admin', 0, ?4)",
                params![uid, body.username.trim(), hash, now_secs()],
            )
            .is_err()
        {
            return admin_json(StatusCode::CONFLICT, serde_json::json!({ "error": "username taken" }));
        }
        // First admin: adopt links parked by a RESET_ADMIN run, else seed the
        // default desktop/dock/apps set plus the About rows.
        let parked: i64 = db
            .query_row("SELECT COUNT(*) FROM links WHERE owner_id = ?1", params![PENDING_OWNER], |r| r.get(0))
            .unwrap_or(0);
        if parked > 0 {
            let _ = db.execute("UPDATE links SET owner_id = ?1 WHERE owner_id = ?2", params![uid, PENDING_OWNER]);
        } else {
            seed_links_for(&db, &uid);
            seed_about_links(&db, &uid);
        }
        admin_json(StatusCode::OK, serde_json::json!({ "ok": true, "user_id": uid }))
    };
    // Log the brand-new admin in straight away.
    if resp.status() == StatusCode::OK {
        let uid = {
            let db = st.db.lock().unwrap();
            admin_user(&db).map(|u| u.id).unwrap_or_default()
        };
        let cookie = create_session(&st, &uid, &headers);
        resp.headers_mut().insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());
        let pairs = {
            let db = st.db.lock().unwrap();
            faviconless_auto_links(&db, None)
        };
        warm_favicons(&st, pairs);
    }
    resp
}

#[derive(Deserialize)]
struct LoginReq {
    username: String,
    password: String,
}

/// Shared credential check for both login doors. `admin_only` restricts the
/// obscure-path door to the admin account.
fn do_login(st: &Shared, headers: &HeaderMap, conn: &SocketAddr, body: &LoginReq, admin_only: bool) -> Response {
    let ip = client_ip(headers, conn);
    if let Err(r) = login_gate(st, &ip) { return r; }
    let found = {
        let db = st.db.lock().unwrap();
        user_by_name(&db, body.username.trim())
    };
    let ok = match &found {
        Some((u, hash)) => {
            !u.disabled && (!admin_only || u.role == "admin") && verify_password(&body.password, hash)
        }
        None => false,
    };
    if !ok {
        return login_failed(st, &ip);
    }
    login_succeeded(st, &ip);
    let uid = found.unwrap().0.id;
    let cookie = create_session(st, &uid, headers);
    let mut r = admin_json(StatusCode::OK, serde_json::json!({ "ok": true }));
    r.headers_mut().insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());
    r
}

async fn admin_login(
    State(st): State<Shared>,
    headers: HeaderMap,
    conn: ConnectInfo<SocketAddr>,
    Path(seg): Path<String>,
    Json(body): Json<LoginReq>,
) -> Response {
    {
        let db = st.db.lock().unwrap();
        if seg != admin_path(&db) {
            return (StatusCode::NOT_FOUND, "not found").into_response();
        }
    }
    do_login(&st, &headers, &conn.0, &body, true)
}

async fn user_login_page(State(st): State<Shared>, headers: HeaderMap) -> Response {
    if session_from_headers(&st, &headers).is_some() {
        return redirect_to("/");
    }
    let open = {
        let db = st.db.lock().unwrap();
        registration_open(&db)
    };
    html_page(StatusCode::OK, admin_page::login_page_ext(false, open), true)
}

async fn user_login(
    State(st): State<Shared>,
    headers: HeaderMap,
    conn: ConnectInfo<SocketAddr>,
    Json(body): Json<LoginReq>,
) -> Response {
    do_login(&st, &headers, &conn.0, &body, false)
}

async fn logout(State(st): State<Shared>, headers: HeaderMap) -> Response {
    if let Some(sess) = session_from_headers(&st, &headers) {
        let db = st.db.lock().unwrap();
        let _ = db.execute("DELETE FROM sessions WHERE token_hash = ?1", params![sess.token_hash]);
    }
    let mut r = admin_json(StatusCode::OK, serde_json::json!({ "ok": true }));
    r.headers_mut().insert(header::SET_COOKIE, HeaderValue::from_str(&clear_cookie(&headers)).unwrap());
    r
}

async fn account_page(State(st): State<Shared>, headers: HeaderMap) -> Response {
    match session_from_headers(&st, &headers) {
        Some(_) => html_page(StatusCode::OK, admin_page::account_page(), true),
        None => redirect_to("/login"),
    }
}

// ─────────────────────────────────────────────────────────────────────
// Self-registration + password reset (public, rate-gated)
// ─────────────────────────────────────────────────────────────────────
#[derive(Deserialize)]
struct RegisterReq {
    username: String,
    email: String,
    password: String,
}

async fn register_page_h(State(st): State<Shared>) -> Response {
    let open = {
        let db = st.db.lock().unwrap();
        registration_open(&db)
    };
    html_page(StatusCode::OK, admin_page::register_page(open), true)
}

async fn register(
    State(st): State<Shared>,
    headers: HeaderMap,
    conn: ConnectInfo<SocketAddr>,
    Json(body): Json<RegisterReq>,
) -> Response {
    let ip = client_ip(&headers, &conn.0);
    if let Err(r) = login_gate(&st, &ip) {
        return r;
    }
    let username = body.username.trim().to_string();
    let email = body.email.trim().to_lowercase();
    if !valid_username(&username) {
        return err_json(StatusCode::BAD_REQUEST, "invalid username (letters, digits, _ . - , max 32)");
    }
    if body.password.len() < 10 {
        return err_json(StatusCode::BAD_REQUEST, "password must be at least 10 characters");
    }
    if !valid_email(&email) {
        return err_json(StatusCode::BAD_REQUEST, "enter a valid email address");
    }
    let hash = match hash_password(&body.password) {
        Ok(h) => h,
        Err(_) => return err_json(StatusCode::INTERNAL_SERVER_ERROR, "hashing failed"),
    };
    let base = request_base(&headers);
    let uid = random_id();
    {
        let db = st.db.lock().unwrap();
        if !registration_open(&db) {
            return err_json(StatusCode::FORBIDDEN, "registration is closed");
        }
        if db
            .execute(
                "INSERT INTO users(id, username, password_hash, role, disabled, created_at, email) VALUES(?1, ?2, ?3, 'user', 0, ?4, ?5)",
                params![uid, username, hash, now_secs(), email],
            )
            .is_err()
        {
            return err_json(StatusCode::CONFLICT, "that username or email is already registered");
        }
        // New users start with an EMPTY desktop and build their own
        // (v1.17.0 — no more cloning the admin's set into every account).
    }
    let cookie = create_session(&st, &uid, &headers);
    send_welcome(&st, email, username, base);
    let mut r = admin_json(StatusCode::OK, serde_json::json!({ "ok": true }));
    r.headers_mut()
        .insert(header::SET_COOKIE, HeaderValue::from_str(&cookie).unwrap());
    r
}

async fn forgot_page_h(State(st): State<Shared>) -> Response {
    let ready = {
        let db = st.db.lock().unwrap();
        smtp_configured(&smtp_settings(&db))
    };
    html_page(StatusCode::OK, admin_page::forgot_page(ready), true)
}

#[derive(Deserialize)]
struct ForgotReq {
    identifier: String,
}

async fn forgot(
    State(st): State<Shared>,
    headers: HeaderMap,
    conn: ConnectInfo<SocketAddr>,
    Json(body): Json<ForgotReq>,
) -> Response {
    let ip = client_ip(&headers, &conn.0);
    if let Err(r) = login_gate(&st, &ip) {
        return r;
    }
    let ident = body.identifier.trim().to_string();
    let base = request_base(&headers);
    // Build the mail job under the lock; send after releasing it. The
    // response is identical whether or not anything was sent.
    let job = {
        let db = st.db.lock().unwrap();
        let s = smtp_settings(&db);
        let found: Option<(String, String)> = db
            .query_row(
                "SELECT id, email FROM users WHERE (username = ?1 OR LOWER(email) = LOWER(?1)) AND email IS NOT NULL AND disabled = 0 LIMIT 1",
                params![ident],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        match (found, smtp_configured(&s)) {
            (Some((uid, email)), true) => {
                let token = random_hex(32);
                let _ = db.execute("DELETE FROM password_resets WHERE user_id = ?1", params![uid]);
                let _ = db.execute(
                    "INSERT INTO password_resets(token_hash, user_id, expires) VALUES(?1, ?2, ?3)",
                    params![sha256_hex(&token), uid, now_secs() + 3600],
                );
                Some((s, email, token, product_name(&db)))
            }
            _ => None,
        }
    };
    if let Some((s, email, token, product)) = job {
        let link = format!("{base}/reset?token={token}");
        let text = format!(
            "Someone asked to reset the password for your {product} account.\n\nUse this link within the next hour:\n{link}\n\nIf that wasn't you, ignore this email — nothing changes until the link is used.\n"
        );
        let _ = send_mail(&s, &email, &format!("Reset your {product} password"), &text).await;
    }
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true }))
}

#[derive(Deserialize)]
struct ResetQuery {
    token: Option<String>,
}

async fn reset_page_h(State(st): State<Shared>, Query(q): Query<ResetQuery>) -> Response {
    let token = q.token.unwrap_or_default();
    let valid = {
        let db = st.db.lock().unwrap();
        db.query_row(
            "SELECT expires FROM password_resets WHERE token_hash = ?1",
            params![sha256_hex(token.trim())],
            |r| r.get::<_, i64>(0),
        )
        .map(|exp| exp > now_secs())
        .unwrap_or(false)
    };
    if valid {
        html_page(StatusCode::OK, admin_page::reset_page(&token), true)
    } else {
        html_page(StatusCode::OK, admin_page::reset_invalid_page(), true)
    }
}

#[derive(Deserialize)]
struct ResetReq {
    token: String,
    password: String,
}

async fn reset_password(
    State(st): State<Shared>,
    headers: HeaderMap,
    conn: ConnectInfo<SocketAddr>,
    Json(body): Json<ResetReq>,
) -> Response {
    let ip = client_ip(&headers, &conn.0);
    if let Err(r) = login_gate(&st, &ip) {
        return r;
    }
    if body.password.len() < 10 {
        return err_json(StatusCode::BAD_REQUEST, "password must be at least 10 characters");
    }
    let hash = match hash_password(&body.password) {
        Ok(h) => h,
        Err(_) => return err_json(StatusCode::INTERNAL_SERVER_ERROR, "hashing failed"),
    };
    let db = st.db.lock().unwrap();
    let row: Option<(String, i64)> = db
        .query_row(
            "SELECT user_id, expires FROM password_resets WHERE token_hash = ?1",
            params![sha256_hex(body.token.trim())],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    let uid = match row {
        Some((uid, exp)) if exp > now_secs() => uid,
        _ => return err_json(StatusCode::BAD_REQUEST, "that reset link is invalid or has expired"),
    };
    let _ = db.execute("UPDATE users SET password_hash = ?1 WHERE id = ?2", params![hash, uid]);
    let _ = db.execute("DELETE FROM password_resets WHERE user_id = ?1", params![uid]);
    // A reset signs the account out everywhere — the old password may
    // be compromised.
    let _ = db.execute("DELETE FROM sessions WHERE user_id = ?1", params![uid]);
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true }))
}

// ─────────────────────────────────────────────────────────────────────
// Link reports (public create; admin review)
// ─────────────────────────────────────────────────────────────────────
#[derive(Deserialize)]
struct ReportReq {
    link_id: Option<String>,
    username: Option<String>,
    reason: Option<String>,
    /// Where the report was filed from: a share landing page (token)
    /// or a profile page (owner username). Validated server-side
    /// against the reported item; anything else = the startpage.
    share_token: Option<String>,
    via_profile: Option<String>,
}

async fn report_create(
    State(st): State<Shared>,
    headers: HeaderMap,
    conn: ConnectInfo<SocketAddr>,
    Json(body): Json<ReportReq>,
) -> Response {
    let ip = client_ip(&headers, &conn.0);
    let reason: String = body
        .reason
        .unwrap_or_default()
        .trim()
        .chars()
        .take(500)
        .collect();
    let reporter = session_from_headers(&st, &headers)
        .map(|s| s.user.username)
        .unwrap_or_else(|| "visitor".to_string());
    let db = st.db.lock().unwrap();
    // Two reportable things: a link (link_id) or a public profile
    // (username — stored with an empty link_id and a "@name (profile)"
    // label snapshot). The resolution also yields the owner's name and
    // the item's address, so the notification email can tell apart two
    // users' identically-named links (v1.17.2).
    let base = request_base(&headers);
    let (label, owner, stored_link_id, owner_name, address, link_folder_id) = match body
        .username
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(uname) => {
            let u: Option<(String, String)> = db
                .query_row(
                    "SELECT id, username FROM users WHERE LOWER(username) = LOWER(?1)",
                    params![uname],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .ok();
            match u {
                Some((uid, canonical)) => (
                    format!("@{canonical} (profile)"),
                    uid,
                    String::new(),
                    canonical.clone(),
                    format!("{base}/u/{canonical}"),
                    None,
                ),
                None => return err_json(StatusCode::NOT_FOUND, "user not found"),
            }
        }
        None => {
            let link_id = body.link_id.clone().unwrap_or_default();
            let link: Option<(String, String, Option<String>, String, Option<String>)> = db
                .query_row("SELECT label, owner_id, url, kind, folder_id FROM links WHERE id = ?1", params![link_id], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })
                .ok();
            match link {
                Some((label, owner, url, kind, folder_id)) => {
                    let owner_name: String = db
                        .query_row("SELECT username FROM users WHERE id = ?1", params![owner], |r| r.get(0))
                        .unwrap_or_else(|_| "unknown".to_string());
                    let address = match url {
                        Some(u) => u,
                        None if kind == "folder" => "(a folder — no web address)".to_string(),
                        None => "(no address)".to_string(),
                    };
                    (label, owner, link_id, owner_name, address, folder_id)
                }
                None => return err_json(StatusCode::NOT_FOUND, "link not found"),
            }
        }
    };
    // Where was it reported FROM? Claims are validated against the
    // item itself, so a forged context is ignored, never stored.
    let source: String = if stored_link_id.is_empty() {
        format!("profile:/u/{owner_name}")
    } else if let Some(tok) = body.share_token.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        match share_by_token(&db, tok) {
            Some((_sid, shared_link, exp)) if share_live(exp) && (shared_link == stored_link_id || link_folder_id.as_deref() == Some(shared_link.as_str())) => {
                format!("share:/s/{tok}")
            }
            _ => "startpage".to_string(),
        }
    } else if let Some(uname) = body.via_profile.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let owner_of_link: Option<String> = db
            .query_row("SELECT username FROM users WHERE LOWER(username) = LOWER(?1)", params![uname], |r| r.get(0))
            .ok();
        if owner_of_link.as_deref() == Some(owner_name.as_str()) {
            format!("profile:/u/{owner_name}")
        } else {
            "startpage".to_string()
        }
    } else {
        "startpage".to_string()
    };
    let recent: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM reports WHERE ip = ?1 AND created_at > ?2",
            params![ip, now_secs() - 3600],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if recent >= 5 {
        return err_json(StatusCode::TOO_MANY_REQUESTS, "too many reports — try again later");
    }
    let _ = db.execute(
        "INSERT INTO reports(id, link_id, label, owner_id, reporter, reason, ip, status, created_at, source) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, 'open', ?8, ?9)",
        params![random_id(), stored_link_id, label.clone(), owner, reporter.clone(), reason.clone(), ip, now_secs(), source.clone()],
    );
    // Email the admins (spawned — the panel remains the record if mail fails).
    let admin_emails: Vec<String> = db
        .prepare("SELECT email FROM users WHERE role = 'admin' AND email IS NOT NULL AND email <> ''")
        .and_then(|mut stmt| stmt.query_map([], |r| r.get::<_, String>(0)).map(|q| q.filter_map(|x| x.ok()).collect()))
        .unwrap_or_default();
    if !admin_emails.is_empty() {
        let s = smtp_settings(&db);
        let subject = format!("New report: {label} ({owner_name})");
        let reason_txt = if reason.is_empty() { "(no reason given)".to_string() } else { reason };
        let from_txt = if let Some(p) = source.strip_prefix("share:") {
            format!("{base}{p} (share link)")
        } else if let Some(p) = source.strip_prefix("profile:") {
            format!("{base}{p} (profile page)")
        } else {
            "the public startpage".to_string()
        };
        let text = format!("Someone reported an item on your startpage.\n\nItem: {label}\nBelongs to: {owner_name}\nAddress: {address}\nReported by: {reporter}\nReported from: {from_txt}\nReason: {reason_txt}\n\nReview it in the admin panel, under Reports.");
        tokio::spawn(async move {
            for to in &admin_emails {
                let _ = send_mail(&s, to, &subject, &text).await;
            }
        });
    }
    admin_json(StatusCode::CREATED, serde_json::json!({ "ok": true }))
}

async fn admin_reports(State(st): State<Shared>, headers: HeaderMap) -> Response {
    if let Err(r) = require_admin(&st, &headers) {
        return r;
    }
    let db = st.db.lock().unwrap();
    let mut stmt = match db.prepare(
        "SELECT r.id, r.link_id, r.label, r.reporter, r.reason, r.status, r.created_at, COALESCE(u.username, 'unknown'), l.icon, l.url, r.source \
         FROM reports r LEFT JOIN users u ON u.id = r.owner_id LEFT JOIN links l ON l.id = r.link_id ORDER BY r.created_at DESC LIMIT 100",
    ) {
        Ok(s) => s,
        Err(_) => return admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "db error" })),
    };
    let reports: Vec<serde_json::Value> = stmt
        .query_map([], |r| {
            let icon: Option<String> = r.get(8)?;
            let icon_upload = icon.as_deref().map(|i| i.starts_with("/uploads/")).unwrap_or(false);
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?, "link_id": r.get::<_, String>(1)?, "label": r.get::<_, String>(2)?,
                "reporter": r.get::<_, String>(3)?, "reason": r.get::<_, String>(4)?, "status": r.get::<_, String>(5)?,
                "created_at": r.get::<_, i64>(6)?, "owner": r.get::<_, String>(7)?,
                "icon": icon, "icon_upload": icon_upload, "url": r.get::<_, Option<String>>(9)?,
                "source": r.get::<_, Option<String>>(10)?
            }))
        })
        .map(|q| q.filter_map(|x| x.ok()).collect())
        .unwrap_or_default();
    admin_json(StatusCode::OK, serde_json::json!({ "reports": reports }))
}

/// One-click answer to an icon report (v1.17.0): the reported link's
/// icon is a user upload — clear it from the link, delete the file,
/// and resolve the report.
async fn admin_report_remove_icon(
    State(st): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let sess = match require_admin(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(r) = require_csrf(&sess, &headers) {
        return r;
    }
    let (link_id, icon) = {
        let db = st.db.lock().unwrap();
        let link_id: Option<String> = db
            .query_row("SELECT link_id FROM reports WHERE id = ?1", params![id], |r| r.get(0))
            .ok();
        let link_id = match link_id.filter(|s| !s.is_empty()) {
            Some(l) => l,
            None => return admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "report or link not found" })),
        };
        let icon: Option<String> = db
            .query_row("SELECT icon FROM links WHERE id = ?1", params![link_id], |r| r.get(0))
            .ok();
        let icon = match icon {
            Some(i) if i.starts_with("/uploads/") => i,
            _ => return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "this link's icon is not an upload" })),
        };
        let _ = db.execute("UPDATE links SET icon = 'auto' WHERE id = ?1", params![link_id]);
        let _ = db.execute("DELETE FROM favicons WHERE link_id = ?1", params![link_id]);
        let _ = db.execute("UPDATE reports SET status = 'resolved' WHERE id = ?1", params![id]);
        (link_id, icon)
    };
    let _ = link_id;
    if let Some(file) = icon.strip_prefix("/uploads/") {
        if is_icon_filename(file) {
            let _ = std::fs::remove_file(st.data_dir.join("uploads/icons").join(file));
        }
    }
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true }))
}

#[derive(Deserialize)]
struct ReportStatusReq {
    status: String,
}

async fn admin_report_status(
    State(st): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<ReportStatusReq>,
) -> Response {
    let sess = match require_admin(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(r) = require_csrf(&sess, &headers) {
        return r;
    }
    if !["open", "resolved", "dismissed"].contains(&body.status.as_str()) {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "bad status" }));
    }
    let db = st.db.lock().unwrap();
    let _ = db.execute("UPDATE reports SET status = ?1 WHERE id = ?2", params![body.status, id]);
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true }))
}

async fn admin_report_delete(
    State(st): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let sess = match require_admin(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(r) = require_csrf(&sess, &headers) {
        return r;
    }
    let db = st.db.lock().unwrap();
    let n = db.execute("DELETE FROM reports WHERE id = ?1", params![id]).unwrap_or(0);
    if n == 0 {
        return admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "report not found" }));
    }
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true }))
}

// ─────────────────────────────────────────────────────────────────────
// Share links — a token URL (/s/<token>) opens a public, read-only
// landing page for one app or one folder. The token IS the credential:
// it authorizes opening exactly that item (or a folder's members) via
// /s/<token>/go/<link_id>, bypassing the /go owner rule. Disguised
// admin links can never be shared. Managed from the panel.
// ─────────────────────────────────────────────────────────────────────
fn share_by_token(db: &Connection, token: &str) -> Option<(String, String, Option<i64>)> {
    db.query_row(
        "SELECT id, link_id, expires_at FROM shares WHERE token = ?1",
        params![token],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .ok()
}

fn share_live(expires_at: Option<i64>) -> bool {
    expires_at.map(|e| e > now_secs()).unwrap_or(true)
}

fn share_icon_src(icon: &str, id: &str) -> String {
    if icon.is_empty() {
        format!("/icon/{id}")
    } else if icon.starts_with('/') || icon.starts_with("http") {
        icon.to_string()
    } else {
        format!("/{icon}")
    }
}

async fn share_landing(State(st): State<Shared>, Path(token): Path<String>) -> Response {
    let db = st.db.lock().unwrap();
    let product = product_name(&db);
    let gone = |db: &Connection| {
        html_page(StatusCode::NOT_FOUND, admin_page::share_gone_page(&product_name(db)), true)
    };
    let (sid, link_id, exp) = match share_by_token(&db, &token) {
        Some(s) if share_live(s.2) => s,
        _ => return gone(&db),
    };
    let item = match link_full(&db, &link_id) {
        Some(l) if l.kind != "admin" => l,
        _ => return gone(&db),
    };
    let _ = db.execute("UPDATE shares SET views = views + 1 WHERE id = ?1", params![sid]);
    let owner_name = user_by_id(&db, &item.owner_id).map(|u| u.username).unwrap_or_default();
    let mut tiles: Vec<(String, String, String, Option<String>)> = Vec::new();
    if item.kind == "folder" {
        if let Ok(mut stmt) = db.prepare(
            "SELECT id, label, icon, icon_bg FROM links WHERE folder_id = ?1 AND kind != 'admin' ORDER BY sort, label",
        ) {
            if let Ok(q) = stmt.query_map(params![item.id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?))
            }) {
                tiles = q
                    .filter_map(|x| x.ok())
                    .map(|(id, label, icon, bg)| (id.clone(), label, share_icon_src(&icon, &id), bg))
                    .collect();
            }
        }
    } else {
        tiles.push((item.id.clone(), item.label.clone(), share_icon_src(&item.icon, &item.id), item.icon_bg.clone()));
    }
    html_page(
        StatusCode::OK,
        admin_page::share_page(&product, &item.id, &item.label, item.kind == "folder", &owner_name, &token, &tiles),
        true,
    )
}

async fn share_open(State(st): State<Shared>, Path((token, link_id)): Path<(String, String)>) -> Response {
    let db = st.db.lock().unwrap();
    let shared_link = match share_by_token(&db, &token) {
        Some(s) if share_live(s.2) => s.1,
        _ => return err_json(StatusCode::NOT_FOUND, "share not found"),
    };
    let row: Option<(String, Option<String>, Option<String>, String)> = db
        .query_row(
            "SELECT kind, url, folder_id, label FROM links WHERE id = ?1",
            params![link_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .ok();
    let (kind, url, folder_id, label) = match row {
        Some(v) => v,
        None => return err_json(StatusCode::NOT_FOUND, "link not found"),
    };
    let authorized = link_id == shared_link || folder_id.as_deref() == Some(shared_link.as_str());
    if kind == "admin" || !authorized {
        return err_json(StatusCode::NOT_FOUND, "link not found");
    }
    match url {
        Some(t) if is_container_url(&t) => container_handoff_response(&label, &t),
        Some(t) => Response::builder()
            .status(StatusCode::FOUND)
            .header(header::LOCATION, t)
            .header(header::REFERRER_POLICY, "no-referrer")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::empty())
            .unwrap(),
        None => err_json(StatusCode::NOT_FOUND, "link not found"),
    }
}

#[derive(Deserialize)]
struct ShareListQuery {
    owner: Option<String>,
}

async fn admin_shares_list(
    State(st): State<Shared>,
    headers: HeaderMap,
    Query(q): Query<ShareListQuery>,
) -> Response {
    if let Err(r) = require_admin(&st, &headers) {
        return r;
    }
    let db = st.db.lock().unwrap();
    let owner = q.owner.filter(|o| !o.is_empty()).unwrap_or_else(|| admin_user(&db).map(|u| u.id).unwrap_or_default());
    let mut stmt = match db.prepare(
        "SELECT s.id, s.token, s.link_id, l.label, l.kind, s.expires_at, s.views, s.created_at \
         FROM shares s JOIN links l ON l.id = s.link_id WHERE s.owner_id = ?1 ORDER BY s.created_at DESC",
    ) {
        Ok(s) => s,
        Err(_) => return admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "db error" })),
    };
    let shares: Vec<serde_json::Value> = stmt
        .query_map(params![owner], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?, "token": r.get::<_, String>(1)?, "link_id": r.get::<_, String>(2)?,
                "label": r.get::<_, String>(3)?, "kind": r.get::<_, String>(4)?,
                "expires_at": r.get::<_, Option<i64>>(5)?, "views": r.get::<_, i64>(6)?, "created_at": r.get::<_, i64>(7)?
            }))
        })
        .map(|q| q.filter_map(|x| x.ok()).collect())
        .unwrap_or_default();
    admin_json(StatusCode::OK, serde_json::json!({ "shares": shares }))
}

#[derive(Deserialize)]
struct ShareCreateReq {
    link_id: String,
    expires_days: Option<i64>,
}

async fn admin_share_create(
    State(st): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<ShareCreateReq>,
) -> Response {
    let sess = match require_admin(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(r) = require_csrf(&sess, &headers) {
        return r;
    }
    let db = st.db.lock().unwrap();
    let link = match link_full(&db, &body.link_id) {
        Some(l) => l,
        None => return admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "link not found" })),
    };
    if link.kind == "admin" {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "admin links can't be shared" }));
    }
    // One live share per item: creating a new one replaces the old token.
    let _ = db.execute("DELETE FROM shares WHERE link_id = ?1", params![link.id]);
    let token = random_hex(16);
    let expires_at = body
        .expires_days
        .filter(|d| *d > 0)
        .map(|d| now_secs() + d * 86400);
    let _ = db.execute(
        "INSERT INTO shares(id, token, link_id, owner_id, expires_at, views, created_at) VALUES(?1, ?2, ?3, ?4, ?5, 0, ?6)",
        params![random_id(), token, link.id, link.owner_id, expires_at, now_secs()],
    );
    admin_json(StatusCode::CREATED, serde_json::json!({ "ok": true, "token": token, "path": format!("/s/{token}") }))
}

async fn admin_share_revoke(
    State(st): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let sess = match require_admin(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(r) = require_csrf(&sess, &headers) {
        return r;
    }
    let db = st.db.lock().unwrap();
    let n = db.execute("DELETE FROM shares WHERE id = ?1", params![id]).unwrap_or(0);
    if n == 0 {
        return admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "share not found" }));
    }
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true }))
}

// ── Share links for regular users (My account) ────────────────────
// Same feature, scoped hard to the caller's own links: list, create
// and revoke only ever touch shares whose owner is the session user.
async fn me_links(State(st): State<Shared>, headers: HeaderMap) -> Response {
    let sess = match require_session(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let db = st.db.lock().unwrap();
    let ids: Vec<String> = {
        let mut stmt = match db.prepare("SELECT id FROM links WHERE owner_id = ?1 ORDER BY section, sort, row, col") {
            Ok(s) => s,
            Err(_) => return admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "db error" })),
        };
        let m = stmt.query_map(params![sess.user.id], |r| r.get::<_, String>(0));
        match m { Ok(q) => q.filter_map(|x| x.ok()).collect(), Err(_) => Vec::new() }
    };
    let links: Vec<LinkFull> = ids.iter().filter_map(|id| link_full(&db, id)).collect();
    admin_json(StatusCode::OK, serde_json::json!({ "links": links }))
}

// ── Build-your-own links (v1.17.0) ────────────────────────────────
// The desktop editor's CRUD: same validation pipeline as the panel
// (validate_link / insert_link), but the owner is FORCED to the
// session user and every row touched must already be theirs. The
// admin uses these too — on their own desktop; the panel remains
// the cross-account tool.
fn next_free_cell(db: &Connection, owner: &str) -> (i64, i64) {
    let taken: std::collections::HashSet<(i64, i64)> = {
        let mut stmt = match db.prepare(
            "SELECT col, row FROM links WHERE owner_id = ?1 AND section = 'desktop' AND folder_id IS NULL AND col IS NOT NULL AND row IS NOT NULL",
        ) {
            Ok(s) => s,
            Err(_) => return (0, 0),
        };
        let m = stmt.query_map(params![owner], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)));
        match m { Ok(q) => q.filter_map(|x| x.ok()).collect(), Err(_) => std::collections::HashSet::new() }
    };
    for row in 0..1000i64 {
        for col in 0..12i64 {
            if !taken.contains(&(col, row)) {
                return (col, row);
            }
        }
    }
    (0, 1000)
}

async fn me_links_create(State(st): State<Shared>, headers: HeaderMap, Json(mut input): Json<LinkInput>) -> Response {
    let sess = match require_session(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(r) = require_csrf(&sess, &headers) {
        return r;
    }
    input.owner = Some(sess.user.id.clone());
    let (id, url_for_icon) = {
        let db = st.db.lock().unwrap();
        let admin_id = admin_user(&db).map(|u| u.id).unwrap_or_default();
        // Placement defaults: desktop → first free grid cell, dock → end.
        if input.section == "desktop" && input.col.is_none() && input.row.is_none() && input.folder_id.is_none() {
            let (c, r) = next_free_cell(&db, &sess.user.id);
            input.col = Some(c);
            input.row = Some(r);
        }
        if ["dock", "apps", "about"].contains(&input.section.as_str()) && input.sort.is_none() {
            let max: i64 = db
                .query_row("SELECT COALESCE(MAX(sort), 0) FROM links WHERE owner_id = ?1 AND section = ?2", params![sess.user.id, input.section], |r| r.get(0))
                .unwrap_or(0);
            input.sort = Some(max + 1);
        }
        // The About card is the software-branding surface — admin-only.
        if input.section == "about" && sess.user.role != "admin" {
            return admin_json(StatusCode::FORBIDDEN, serde_json::json!({ "error": "Only the admin can put links in the About card." }));
        }
        let (owner, kind, label, url, icon_mode, icon, section, col, row, sort, group) = match validate_link(&db, &input, &sess.user.id, &admin_id) {
            Ok(v) => v,
            Err(msg) => return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": msg })),
        };
        let fid = match resolve_folder(&db, &owner, &input.folder_id, &kind) {
            Ok(f) => f,
            Err(msg) => return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": msg })),
        };
        let icon_stored = if icon_mode == "auto" { "auto" } else { icon.as_deref().unwrap_or("src/icons/globe.svg") };
        let id = insert_link(&db, &owner, &kind, &label, url.as_deref(), icon_stored, &section, col, row, sort, group.as_deref());
        if let Some(fid) = &fid {
            let _ = db.execute("UPDATE links SET folder_id = ?1 WHERE id = ?2", params![fid, id]);
        }
        if let Some(bg) = sanitize_icon_bg(&input.icon_bg) {
            let _ = db.execute("UPDATE links SET icon_bg = ?1 WHERE id = ?2", params![bg, id]);
        }
        let fetch = if icon_mode == "auto" && kind == "url" { url.clone() } else { None };
        (id, fetch)
    };
    if let Some(u) = url_for_icon {
        let st2 = st.clone();
        let id2 = id.clone();
        tokio::spawn(async move { refresh_favicon(&st2, &id2, &u).await; });
    }
    let db = st.db.lock().unwrap();
    match link_full(&db, &id) {
        Some(l) => admin_json(StatusCode::CREATED, serde_json::json!({ "link": l })),
        None => admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "create failed" })),
    }
}

async fn me_links_update(State(st): State<Shared>, headers: HeaderMap, Path(id): Path<String>, Json(mut input): Json<LinkInput>) -> Response {
    let sess = match require_session(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(r) = require_csrf(&sess, &headers) {
        return r;
    }
    let fetch = {
        let db = st.db.lock().unwrap();
        let existing = match link_full(&db, &id) {
            Some(l) => l,
            None => return admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "no such link" })),
        };
        if existing.owner_id != sess.user.id {
            return admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "no such link" }));
        }
        let admin_id = admin_user(&db).map(|u| u.id).unwrap_or_default();
        input.owner = Some(sess.user.id.clone());
        // Keep placement unless the editor sent new values.
        if input.col.is_none() { input.col = existing.col; }
        if input.row.is_none() { input.row = existing.row; }
        if input.sort.is_none() { input.sort = Some(existing.sort); }
        // folder_id: "" is the editor's explicit "remove from folder";
        // absent means "keep as-is".
        let clear_folder = input.folder_id.as_deref() == Some("");
        if clear_folder {
            input.folder_id = None;
            if existing.section == "desktop" {
                let (c, r) = next_free_cell(&db, &sess.user.id);
                input.col = Some(c);
                input.row = Some(r);
            }
        }
        if input.folder_id.is_none() && !clear_folder { input.folder_id = existing.folder_id.clone(); }
        if input.group_title.is_none() { input.group_title = existing.group_title.clone(); }
        if input.section != existing.section && input.section == "desktop" && existing.section != "desktop" {
            let (c, r) = next_free_cell(&db, &sess.user.id);
            input.col = Some(c);
            input.row = Some(r);
            input.folder_id = None;
        }
        if input.section != existing.section && input.section != "desktop" {
            // Leaving the desktop (to dock / All Apps / About) leaves any folder.
            input.folder_id = None;
        }
        // The About card is the software-branding surface — admin-only.
        if input.section == "about" && sess.user.role != "admin" {
            return admin_json(StatusCode::FORBIDDEN, serde_json::json!({ "error": "Only the admin can put links in the About card." }));
        }
        let (_owner, kind, label, url, icon_mode, icon, section, col, row, sort, group) = match validate_link(&db, &input, &sess.user.id, &admin_id) {
            Ok(v) => v,
            Err(msg) => return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": msg })),
        };
        let fid = match resolve_folder(&db, &sess.user.id, &input.folder_id, &kind) {
            Ok(f) => f,
            Err(msg) => return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": msg })),
        };
        let icon_stored = if icon_mode == "auto" { format!("/icon/{}", id) } else { icon.unwrap_or_else(|| "src/icons/globe.svg".to_string()) };
        let _ = db.execute(
            "UPDATE links SET kind = ?1, label = ?2, url = ?3, icon = ?4, section = ?5, col = ?6, row = ?7, sort = ?8, group_title = ?9, folder_id = ?10, icon_bg = ?11 WHERE id = ?12",
            params![kind, label, url, icon_stored, section, col, row, sort, group, fid, sanitize_icon_bg(&input.icon_bg), id],
        );
        let _ = db.execute("DELETE FROM favicons WHERE link_id = ?1", params![id]);
        if icon_mode == "auto" && kind == "url" { url } else { None }
    };
    if let Some(u) = fetch {
        let st2 = st.clone();
        let id2 = id.clone();
        tokio::spawn(async move { refresh_favicon(&st2, &id2, &u).await; });
    }
    let db = st.db.lock().unwrap();
    match link_full(&db, &id) {
        Some(l) => admin_json(StatusCode::OK, serde_json::json!({ "link": l })),
        None => admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "update failed" })),
    }
}

async fn me_links_delete(State(st): State<Shared>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let sess = match require_session(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(r) = require_csrf(&sess, &headers) {
        return r;
    }
    let db = st.db.lock().unwrap();
    let existing = match link_full(&db, &id) {
        Some(l) => l,
        None => return admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "no such link" })),
    };
    if existing.owner_id != sess.user.id {
        return admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "no such link" }));
    }
    if existing.kind == "folder" {
        // Deleting a folder ungroups its members — they become loose apps.
        let _ = db.execute("UPDATE links SET folder_id = NULL WHERE folder_id = ?1", params![id]);
    }
    let _ = db.execute("DELETE FROM favicons WHERE link_id = ?1", params![id]);
    let _ = db.execute("DELETE FROM shares WHERE link_id = ?1", params![id]);
    let _ = db.execute("DELETE FROM links WHERE id = ?1", params![id]);
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true }))
}

// ── Per-user icon uploads (v1.17.0) ───────────────────────────────
// Same sniff/size discipline as the panel's icon uploads, but the
// stored name is namespaced to the uploader (u<user-id>-<file>) so
// the panel can tell whose upload an icon is — and reports on a link
// carrying one can offer the admin a one-click removal.
async fn me_icons_list(State(st): State<Shared>, headers: HeaderMap) -> Response {
    let sess = match require_session(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let mut bundled: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(st.web_dir.join("src/icons")) {
        for ent in rd.flatten() {
            let name = ent.file_name().to_string_lossy().to_string();
            if is_icon_filename(&name) {
                bundled.push(format!("src/icons/{name}"));
            }
        }
    }
    bundled.sort();
    // Uploads split three ways: the caller's OWN (u<their-id>-…), the
    // shared LIBRARY (panel uploads — not namespaced to any user; this
    // is the pre-uploaded list everyone can pick from), and other
    // users' personal uploads (never listed).
    let user_ids: Vec<String> = {
        let db = st.db.lock().unwrap();
        let mut stmt = match db.prepare("SELECT id FROM users") {
            Ok(s) => s,
            Err(_) => return admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "db error" })),
        };
        let m = stmt.query_map([], |r| r.get::<_, String>(0));
        match m { Ok(q) => q.filter_map(|x| x.ok()).collect(), Err(_) => Vec::new() }
    };
    let own_prefix = format!("u{}-", sess.user.id);
    let mut uploads: Vec<String> = Vec::new();
    let mut library: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(st.data_dir.join("uploads/icons")) {
        for ent in rd.flatten() {
            let name = ent.file_name().to_string_lossy().to_string();
            if !is_icon_filename(&name) {
                continue;
            }
            if name.starts_with(&own_prefix) {
                uploads.push(format!("/uploads/{name}"));
            } else if user_ids.iter().any(|uid| name.starts_with(&format!("u{uid}-"))) {
                // someone else's personal upload — private to them
            } else {
                library.push(format!("/uploads/{name}"));
            }
        }
    }
    uploads.sort();
    library.sort();
    admin_json(StatusCode::OK, serde_json::json!({ "icons": bundled, "library": library, "uploads": uploads }))
}

async fn me_icons_upload(State(st): State<Shared>, headers: HeaderMap, Json(input): Json<IconUploadReq>) -> Response {
    let sess = match require_session(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(r) = require_csrf(&sess, &headers) {
        return r;
    }
    let name = match sanitize_icon_name(&input.filename) {
        Some(n) => n,
        None => return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "filename must end in .png, .svg, .ico, .jpg, .jpeg, .webp or .gif" })),
    };
    let stored = format!("u{}-{}", sess.user.id, name);
    let cleaned_b64: String = input.data_base64.chars().filter(|c| !c.is_whitespace()).collect();
    let payload = cleaned_b64.rsplit(',').next().unwrap_or("");
    use base64::Engine;
    let bytes = match base64::engine::general_purpose::STANDARD.decode(payload) {
        Ok(b) => b,
        Err(_) => return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "could not decode the file data" })),
    };
    if bytes.is_empty() || bytes.len() > 512 * 1024 {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "icon must be between 1 byte and 512 KB" }));
    }
    let ext = stored.rsplit('.').next().unwrap_or("").to_lowercase();
    if !icon_content_matches(&ext, &bytes) {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "the file's content does not match its extension" }));
    }
    let dir = st.data_dir.join("uploads/icons");
    if std::fs::create_dir_all(&dir).is_err() {
        return admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "could not create the uploads dir" }));
    }
    match std::fs::write(dir.join(&stored), &bytes) {
        Ok(()) => admin_json(StatusCode::OK, serde_json::json!({ "ok": true, "path": format!("/uploads/{stored}") })),
        Err(e) => admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": format!("write failed: {e}") })),
    }
}

async fn me_shares_list(State(st): State<Shared>, headers: HeaderMap) -> Response {
    let sess = match require_session(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let db = st.db.lock().unwrap();
    let mut stmt = match db.prepare(
        "SELECT s.id, s.token, s.link_id, l.label, l.kind, s.expires_at, s.views, s.created_at \
         FROM shares s JOIN links l ON l.id = s.link_id WHERE s.owner_id = ?1 ORDER BY s.created_at DESC",
    ) {
        Ok(s) => s,
        Err(_) => return admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "db error" })),
    };
    let shares: Vec<serde_json::Value> = stmt
        .query_map(params![sess.user.id], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?, "token": r.get::<_, String>(1)?, "link_id": r.get::<_, String>(2)?,
                "label": r.get::<_, String>(3)?, "kind": r.get::<_, String>(4)?,
                "expires_at": r.get::<_, Option<i64>>(5)?, "views": r.get::<_, i64>(6)?, "created_at": r.get::<_, i64>(7)?
            }))
        })
        .map(|q| q.filter_map(|x| x.ok()).collect())
        .unwrap_or_default();
    admin_json(StatusCode::OK, serde_json::json!({ "shares": shares }))
}

async fn me_share_create(
    State(st): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<ShareCreateReq>,
) -> Response {
    let sess = match require_session(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(r) = require_csrf(&sess, &headers) {
        return r;
    }
    let db = st.db.lock().unwrap();
    let link = match link_full(&db, &body.link_id) {
        Some(l) => l,
        None => return admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "link not found" })),
    };
    if link.owner_id != sess.user.id {
        return admin_json(StatusCode::FORBIDDEN, serde_json::json!({ "error": "you can only share your own links" }));
    }
    if link.kind == "admin" {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "admin links can't be shared" }));
    }
    let _ = db.execute("DELETE FROM shares WHERE link_id = ?1", params![link.id]);
    let token = random_hex(16);
    let expires_at = body
        .expires_days
        .filter(|d| *d > 0)
        .map(|d| now_secs() + d * 86400);
    let _ = db.execute(
        "INSERT INTO shares(id, token, link_id, owner_id, expires_at, views, created_at) VALUES(?1, ?2, ?3, ?4, ?5, 0, ?6)",
        params![random_id(), token, link.id, sess.user.id, expires_at, now_secs()],
    );
    admin_json(StatusCode::CREATED, serde_json::json!({ "ok": true, "token": token, "path": format!("/s/{token}") }))
}

async fn me_share_revoke(
    State(st): State<Shared>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let sess = match require_session(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(r) = require_csrf(&sess, &headers) {
        return r;
    }
    let db = st.db.lock().unwrap();
    let n = db
        .execute("DELETE FROM shares WHERE id = ?1 AND owner_id = ?2", params![id, sess.user.id])
        .unwrap_or(0);
    if n == 0 {
        return admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "share not found" }));
    }
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true }))
}

// ─────────────────────────────────────────────────────────────────────
// Public profiles — /u/<username>: profile + bio (ported from the
// user's LinkPlatform) with the user's startpage apps and folders on
// the same page — one link for everything. Two gates, both required:
// the admin enables profiles site-wide (Settings → Public profiles)
// AND the user switches their own profile public (My account).
// ─────────────────────────────────────────────────────────────────────
#[derive(Clone, Default)]
struct ProfileRow {
    display_name: String,
    bio: String,
    avatar_path: Option<String>,
    header_path: Option<String>,
    accent_color: Option<String>,
    profile_public: bool,
}

fn profile_of(db: &Connection, user_id: &str) -> ProfileRow {
    db.query_row(
        "SELECT display_name, bio, avatar_path, header_path, accent_color, profile_public FROM users WHERE id = ?1",
        params![user_id],
        |r| {
            Ok(ProfileRow {
                display_name: r.get(0)?,
                bio: r.get(1)?,
                avatar_path: r.get(2)?,
                header_path: r.get(3)?,
                accent_color: r.get(4)?,
                profile_public: r.get::<_, i64>(5)? != 0,
            })
        },
    )
    .unwrap_or_default()
}

async fn me_profile_get(State(st): State<Shared>, headers: HeaderMap) -> Response {
    let sess = match require_session(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let db = st.db.lock().unwrap();
    let p = profile_of(&db, &sess.user.id);
    admin_json(StatusCode::OK, serde_json::json!({
        "username": sess.user.username,
        "display_name": p.display_name, "bio": p.bio,
        "avatar_path": p.avatar_path, "header_path": p.header_path,
        "accent_color": p.accent_color, "profile_public": p.profile_public,
        "profiles_enabled": profiles_enabled(&db),
        "path": format!("/u/{}", sess.user.username),
    }))
}

#[derive(Deserialize)]
struct ProfileSaveReq {
    display_name: Option<String>,
    bio: Option<String>,
    accent_color: Option<String>,
    profile_public: Option<bool>,
}

async fn me_profile_save(
    State(st): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<ProfileSaveReq>,
) -> Response {
    let sess = match require_session(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(r) = require_csrf(&sess, &headers) {
        return r;
    }
    let display_name: String = body.display_name.unwrap_or_default().trim().chars().take(80).collect();
    let bio: String = body.bio.unwrap_or_default().trim().chars().take(500).collect();
    let accent = sanitize_icon_bg(&body.accent_color);
    let public = body.profile_public.unwrap_or(false);
    let db = st.db.lock().unwrap();
    let _ = db.execute(
        "UPDATE users SET display_name = ?1, bio = ?2, accent_color = ?3, profile_public = ?4 WHERE id = ?5",
        params![display_name, bio, accent, public as i64, sess.user.id],
    );
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true }))
}

#[derive(Deserialize)]
struct ProfileImageReq {
    kind: String,
    filename: String,
    data_base64: String,
}

async fn me_profile_image(
    State(st): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<ProfileImageReq>,
) -> Response {
    let sess = match require_session(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(r) = require_csrf(&sess, &headers) {
        return r;
    }
    let prefix = match body.kind.as_str() {
        "avatar" => "avatar",
        "header" => "header",
        _ => return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "kind must be avatar or header" })),
    };
    // Same discipline as icon uploads: safe name, base64, ≤2 MB,
    // content sniffed against the extension. The file name is forced
    // per user + kind, so re-uploading replaces the old image.
    let name = match sanitize_icon_name(&body.filename) {
        Some(n) => n,
        None => return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "filename must end in .png, .svg, .ico, .jpg, .jpeg, .webp or .gif" })),
    };
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    let cleaned_b64: String = body.data_base64.chars().filter(|c| !c.is_whitespace()).collect();
    let payload = cleaned_b64.rsplit(',').next().unwrap_or("");
    use base64::Engine;
    let bytes = match base64::engine::general_purpose::STANDARD.decode(payload) {
        Ok(b) => b,
        Err(_) => return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "could not decode the file data" })),
    };
    if bytes.is_empty() || bytes.len() > 2 * 1024 * 1024 {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "image must be between 1 byte and 2 MB" }));
    }
    if !icon_content_matches(&ext, &bytes) {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "the file's content does not match its extension" }));
    }
    let file = format!("{prefix}-{}.{ext}", sess.user.id);
    let dir = st.data_dir.join("uploads/icons");
    if std::fs::create_dir_all(&dir).is_err() {
        return admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "could not create the uploads dir" }));
    }
    if std::fs::write(dir.join(&file), &bytes).is_err() {
        return admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "write failed" }));
    }
    let path = format!("/uploads/{file}");
    let db = st.db.lock().unwrap();
    let col = if prefix == "avatar" { "avatar_path" } else { "header_path" };
    let _ = db.execute(
        &format!("UPDATE users SET {col} = ?1 WHERE id = ?2"),
        params![path, sess.user.id],
    );
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true, "path": path }))
}

/// The public-gate check shared by the profile page and its /go:
/// site profiles on + user exists, enabled and public.
fn public_profile_user(db: &Connection, username: &str) -> Option<(UserRow, ProfileRow)> {
    if !profiles_enabled(db) {
        return None;
    }
    let u = db
        .query_row(
            "SELECT id, username, role, disabled FROM users WHERE LOWER(username) = LOWER(?1)",
            params![username],
            |r| Ok(UserRow { id: r.get(0)?, username: r.get(1)?, role: r.get(2)?, disabled: r.get::<_, i64>(3)? != 0 }),
        )
        .ok()?;
    if u.disabled {
        return None;
    }
    let p = profile_of(db, &u.id);
    if !p.profile_public {
        return None;
    }
    Some((u, p))
}

async fn profile_landing(State(st): State<Shared>, Path(username): Path<String>) -> Response {
    let db = st.db.lock().unwrap();
    let product = product_name(&db);
    let (u, p) = match public_profile_user(&db, &username) {
        Some(v) => v,
        None => return html_page(StatusCode::NOT_FOUND, admin_page::profile_gone_page(&product), true),
    };
    // The user's startpage, profile-style: loose desktop apps first
    // (in grid order), then each folder as its own headed grid.
    type Tile = (String, String, String, Option<String>);
    let mut loose: Vec<Tile> = Vec::new();
    let mut folders: Vec<(String, String, Vec<Tile>)> = Vec::new();
    if let Ok(mut stmt) = db.prepare(
        "SELECT id, label, icon, icon_bg, kind, folder_id FROM links \
         WHERE owner_id = ?1 AND section = 'desktop' AND kind != 'admin' \
         ORDER BY COALESCE(row, 0), COALESCE(col, 0), sort, label",
    ) {
        if let Ok(q) = stmt.query_map(params![u.id], |r| {
            Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?, r.get::<_, String>(4)?, r.get::<_, Option<String>>(5)?,
            ))
        }) {
            let rows: Vec<(String, String, String, Option<String>, String, Option<String>)> =
                q.filter_map(|x| x.ok()).collect();
            let mut members: std::collections::HashMap<String, Vec<Tile>> = std::collections::HashMap::new();
            for (id, label, icon, bg, kind, folder_id) in &rows {
                let tile: Tile = (id.clone(), label.clone(), share_icon_src(icon, id), bg.clone());
                match (kind.as_str(), folder_id) {
                    ("folder", _) => folders.push((id.clone(), label.clone(), Vec::new())),
                    (_, Some(fid)) => members.entry(fid.clone()).or_default().push(tile),
                    _ => loose.push(tile),
                }
            }
            // Attach members to their folders (folder order = grid order).
            let folder_ids: Vec<String> = rows
                .iter()
                .filter(|r| r.4 == "folder")
                .map(|r| r.0.clone())
                .collect();
            for (i, (_, _, tiles)) in folders.iter_mut().enumerate() {
                if let Some(fid) = folder_ids.get(i) {
                    if let Some(m) = members.remove(fid) {
                        *tiles = m;
                    }
                }
            }
        }
    }
    html_page(
        StatusCode::OK,
        admin_page::profile_page(
            &product,
            &u.username,
            &p.display_name,
            &p.bio,
            p.avatar_path.as_deref(),
            p.header_path.as_deref(),
            p.accent_color.as_deref(),
            &loose,
            &folders,
        ),
        true,
    )
}

async fn profile_open(
    State(st): State<Shared>,
    Path((username, link_id)): Path<(String, String)>,
) -> Response {
    let db = st.db.lock().unwrap();
    let (u, _p) = match public_profile_user(&db, &username) {
        Some(v) => v,
        None => return err_json(StatusCode::NOT_FOUND, "profile not found"),
    };
    let row: Option<(String, Option<String>, String, String)> = db
        .query_row(
            "SELECT kind, url, label, owner_id FROM links WHERE id = ?1",
            params![link_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .ok();
    let (kind, url, label, owner_id) = match row {
        Some(v) => v,
        None => return err_json(StatusCode::NOT_FOUND, "link not found"),
    };
    if kind == "admin" || owner_id != u.id {
        return err_json(StatusCode::NOT_FOUND, "link not found");
    }
    match url {
        Some(t) if is_container_url(&t) => container_handoff_response(&label, &t),
        Some(t) => Response::builder()
            .status(StatusCode::FOUND)
            .header(header::LOCATION, t)
            .header(header::REFERRER_POLICY, "no-referrer")
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::empty())
            .unwrap(),
        None => err_json(StatusCode::NOT_FOUND, "link not found"),
    }
}

// ─────────────────────────────────────────────────────────────────────
// Admin: site settings (registration + SMTP)
// ─────────────────────────────────────────────────────────────────────
async fn admin_site(State(st): State<Shared>, headers: HeaderMap) -> Response {
    if let Err(r) = require_admin(&st, &headers) {
        return r;
    }
    let db = st.db.lock().unwrap();
    let s = smtp_settings(&db);
    admin_json(
        StatusCode::OK,
        serde_json::json!({
            "registration_open": registration_open(&db),
            "profiles_enabled": profiles_enabled(&db),
            "default_owner": default_owner(&db),
            "smtp": {
                "host": s.host, "port": s.port, "username": s.username,
                "from_addr": s.from_addr, "from_name": s.from_name, "encryption": s.encryption,
                "has_password": !s.password.is_empty(), "configured": smtp_configured(&s)
            }
        }),
    )
}

#[derive(Deserialize)]
struct RegReq {
    open: bool,
}

async fn admin_site_registration(
    State(st): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<RegReq>,
) -> Response {
    let sess = match require_admin(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(r) = require_csrf(&sess, &headers) {
        return r;
    }
    let db = st.db.lock().unwrap();
    set_setting(&db, "registration_open", if body.open { "1" } else { "0" });
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true, "registration_open": body.open }))
}

#[derive(Deserialize)]
struct ProfilesReq {
    enabled: bool,
}

async fn admin_site_profiles(
    State(st): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<ProfilesReq>,
) -> Response {
    let sess = match require_admin(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(r) = require_csrf(&sess, &headers) {
        return r;
    }
    let db = st.db.lock().unwrap();
    set_setting(&db, "profiles_enabled", if body.enabled { "1" } else { "0" });
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true, "profiles_enabled": body.enabled }))
}

#[derive(Deserialize)]
struct DefaultOwnerReq {
    user_id: Option<String>,
}

async fn admin_site_default_owner(
    State(st): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<DefaultOwnerReq>,
) -> Response {
    let sess = match require_admin(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(r) = require_csrf(&sess, &headers) {
        return r;
    }
    let db = st.db.lock().unwrap();
    match body.user_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(uid) => {
            if user_by_id(&db, uid).is_none() {
                return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "no such user" }));
            }
            set_setting(&db, "default_owner", uid);
        }
        None => {
            let _ = db.execute("DELETE FROM settings WHERE key = 'default_owner'", []);
        }
    }
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true, "default_owner": default_owner(&db) }))
}

#[derive(Deserialize)]
struct SmtpReq {
    host: String,
    port: u16,
    username: String,
    password: Option<String>,
    from_addr: String,
    from_name: String,
    encryption: String,
}

async fn admin_smtp_save(
    State(st): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<SmtpReq>,
) -> Response {
    let sess = match require_admin(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(r) = require_csrf(&sess, &headers) {
        return r;
    }
    if body.port == 0 {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "port required" }));
    }
    if !["starttls", "tls", "none"].contains(&body.encryption.as_str()) {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "bad encryption mode" }));
    }
    let from_addr = body.from_addr.trim().to_string();
    if !from_addr.is_empty() && !valid_email(&from_addr) {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "From address must be a valid email" }));
    }
    let db = st.db.lock().unwrap();
    set_setting(&db, "smtp_host", body.host.trim());
    set_setting(&db, "smtp_port", &body.port.to_string());
    set_setting(&db, "smtp_user", body.username.trim());
    if let Some(p) = &body.password {
        if !p.is_empty() {
            set_setting(&db, "smtp_pass", p);
        }
    }
    set_setting(&db, "smtp_from_addr", &from_addr);
    set_setting(&db, "smtp_from_name", body.from_name.trim());
    set_setting(&db, "smtp_encryption", &body.encryption);
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true }))
}

#[derive(Deserialize)]
struct SmtpTestReq {
    to: String,
}

async fn admin_smtp_test(
    State(st): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<SmtpTestReq>,
) -> Response {
    let sess = match require_admin(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(r) = require_csrf(&sess, &headers) {
        return r;
    }
    let to = body.to.trim().to_string();
    if !valid_email(&to) {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "enter a valid email address" }));
    }
    let (s, product) = {
        let db = st.db.lock().unwrap();
        (smtp_settings(&db), product_name(&db))
    };
    match send_mail(
        &s,
        &to,
        &format!("{product} — SMTP test"),
        "This is a test email from your startpage server.\n\nIf you can read it, outgoing mail is set up correctly — welcome emails and password resets will work.\n",
    )
    .await
    {
        Ok(()) => admin_json(StatusCode::OK, serde_json::json!({ "ok": true })),
        Err(e) => admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": e })),
    }
}

// ─────────────────────────────────────────────────────────────────────
// Admin + account APIs
// ─────────────────────────────────────────────────────────────────────
#[derive(Serialize)]
struct LinkFull {
    id: String,
    owner_id: String,
    kind: String,
    label: String,
    url: Option<String>,
    icon: String,
    icon_mode: String,
    section: String,
    col: Option<i64>,
    row: Option<i64>,
    sort: i64,
    group_title: Option<String>,
    folder_id: Option<String>,
    icon_bg: Option<String>,
}

fn link_full(db: &Connection, id: &str) -> Option<LinkFull> {
    db.query_row(
        "SELECT id, owner_id, kind, label, url, icon, section, col, row, sort, group_title, folder_id, icon_bg FROM links WHERE id = ?1",
        params![id],
        |r| {
            let icon: String = r.get(5)?;
            let mode = if icon == "auto" || icon.starts_with("/icon/") { "auto" } else { "bundled" };
            Ok(LinkFull {
                id: r.get(0)?,
                owner_id: r.get(1)?,
                kind: r.get(2)?,
                label: r.get(3)?,
                url: r.get(4)?,
                icon_mode: mode.to_string(),
                icon,
                section: r.get(6)?,
                col: r.get(7)?,
                row: r.get(8)?,
                sort: r.get(9)?,
                group_title: r.get(10)?,
                folder_id: r.get(11)?,
                icon_bg: r.get(12)?,
            })
        },
    )
    .ok()
}

async fn admin_session(State(st): State<Shared>, headers: HeaderMap) -> Response {
    match require_admin(&st, &headers) {
        Ok(s) => {
            let db = st.db.lock().unwrap();
            admin_json(StatusCode::OK, serde_json::json!({
                "csrf": s.csrf, "username": s.user.username, "user_id": s.user.id, "admin_path": admin_path(&db)
            }))
        }
        Err(r) => r,
    }
}

async fn me_session(State(st): State<Shared>, headers: HeaderMap) -> Response {
    match require_session(&st, &headers) {
        Ok(s) => {
            let email: Option<String> = {
                let db = st.db.lock().unwrap();
                db.query_row("SELECT email FROM users WHERE id = ?1", params![s.user.id], |r| r.get(0))
                    .ok()
                    .flatten()
            };
            admin_json(StatusCode::OK, serde_json::json!({
                "csrf": s.csrf, "username": s.user.username, "role": s.user.role, "user_id": s.user.id, "email": email
            }))
        }
        Err(r) => r,
    }
}

#[derive(Deserialize)]
struct MeEmailReq {
    email: Option<String>,
}

async fn me_email(
    State(st): State<Shared>,
    headers: HeaderMap,
    Json(body): Json<MeEmailReq>,
) -> Response {
    let sess = match require_session(&st, &headers) {
        Ok(s) => s,
        Err(r) => return r,
    };
    if let Err(r) = require_csrf(&sess, &headers) {
        return r;
    }
    let email = body
        .email
        .as_deref()
        .map(|e| e.trim().to_lowercase())
        .filter(|e| !e.is_empty());
    if let Some(e) = &email {
        if !valid_email(e) {
            return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "enter a valid email address" }));
        }
    }
    let db = st.db.lock().unwrap();
    if let Some(e) = &email {
        let taken = db
            .query_row(
                "SELECT COUNT(*) FROM users WHERE email = ?1 AND id <> ?2",
                params![e, sess.user.id],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap_or(false);
        if taken {
            return admin_json(StatusCode::CONFLICT, serde_json::json!({ "error": "that email is already used by another account" }));
        }
    }
    let _ = db.execute("UPDATE users SET email = ?1 WHERE id = ?2", params![email, sess.user.id]);
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true, "email": email }))
}

// ── Users ─────────────────────────────────────────────────────────────
async fn admin_users(State(st): State<Shared>, headers: HeaderMap) -> Response {
    if let Err(r) = require_admin(&st, &headers) { return r; }
    let db = st.db.lock().unwrap();
    let mut stmt = match db.prepare("SELECT id, username, role, disabled, created_at, email FROM users ORDER BY created_at") {
        Ok(s) => s,
        Err(_) => return admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "db error" })),
    };
    let users: Vec<serde_json::Value> = stmt
        .query_map([], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?, "username": r.get::<_, String>(1)?, "role": r.get::<_, String>(2)?,
                "disabled": r.get::<_, i64>(3)? != 0, "created_at": r.get::<_, i64>(4)?,
                "email": r.get::<_, Option<String>>(5)?
            }))
        })
        .map(|q| q.filter_map(|x| x.ok()).collect())
        .unwrap_or_default();
    admin_json(StatusCode::OK, serde_json::json!({ "users": users }))
}

#[derive(Deserialize)]
struct CreateUserReq {
    username: String,
    password: String,
    email: Option<String>,
}

async fn admin_users_create(State(st): State<Shared>, headers: HeaderMap, Json(body): Json<CreateUserReq>) -> Response {
    let sess = match require_admin(&st, &headers) { Ok(s) => s, Err(r) => return r };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    let username = body.username.trim().to_string();
    if !valid_username(&username) {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "invalid username (letters, digits, _ . - , max 32)" }));
    }
    if body.password.len() < 10 {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "password must be at least 10 characters" }));
    }
    let email = body
        .email
        .as_deref()
        .map(|e| e.trim().to_lowercase())
        .filter(|e| !e.is_empty());
    if let Some(e) = &email {
        if !valid_email(e) {
            return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "enter a valid email address" }));
        }
    }
    let hash = match hash_password(&body.password) {
        Ok(h) => h,
        Err(_) => return admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "hashing failed" })),
    };
    let base = request_base(&headers);
    let db = st.db.lock().unwrap();
    let uid = random_id();
    if db
        .execute(
            "INSERT INTO users(id, username, password_hash, role, disabled, created_at, email) VALUES(?1, ?2, ?3, 'user', 0, ?4, ?5)",
            params![uid, username, hash, now_secs(), email.as_deref()],
        )
        .is_err()
    {
        return admin_json(StatusCode::CONFLICT, serde_json::json!({ "error": "username or email taken" }));
    }
    // New users start with an EMPTY desktop and build their own
    // (v1.17.0 — no more cloning the admin's set into every account).
    drop(db);
    if let Some(e) = &email {
        send_welcome(&st, e.clone(), username.clone(), base);
    }
    admin_json(StatusCode::CREATED, serde_json::json!({
        "user": { "id": uid, "username": username, "role": "user", "disabled": false }
    }))
}

#[derive(Deserialize)]
struct PasswordReq {
    password: String,
}

async fn admin_user_password(State(st): State<Shared>, headers: HeaderMap, Path(id): Path<String>, Json(body): Json<PasswordReq>) -> Response {
    let sess = match require_admin(&st, &headers) { Ok(s) => s, Err(r) => return r };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    if body.password.len() < 10 {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "password must be at least 10 characters" }));
    }
    let hash = match hash_password(&body.password) {
        Ok(h) => h,
        Err(_) => return admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "hashing failed" })),
    };
    let db = st.db.lock().unwrap();
    if user_by_id(&db, &id).is_none() {
        return admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "no such user" }));
    }
    let _ = db.execute("UPDATE users SET password_hash = ?1 WHERE id = ?2", params![hash, id]);
    let _ = db.execute("DELETE FROM sessions WHERE user_id = ?1", params![id]);
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true }))
}

#[derive(Deserialize)]
struct DisabledReq {
    disabled: bool,
}

async fn admin_user_disabled(State(st): State<Shared>, headers: HeaderMap, Path(id): Path<String>, Json(body): Json<DisabledReq>) -> Response {
    let sess = match require_admin(&st, &headers) { Ok(s) => s, Err(r) => return r };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    let db = st.db.lock().unwrap();
    match user_by_id(&db, &id) {
        None => admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "no such user" })),
        Some(u) if u.role == "admin" => {
            admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "the admin account cannot be disabled" }))
        }
        Some(_) => {
            let _ = db.execute("UPDATE users SET disabled = ?1 WHERE id = ?2", params![body.disabled as i64, id]);
            if body.disabled {
                let _ = db.execute("DELETE FROM sessions WHERE user_id = ?1", params![id]);
            }
            admin_json(StatusCode::OK, serde_json::json!({ "ok": true }))
        }
    }
}

async fn admin_user_delete(State(st): State<Shared>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let sess = match require_admin(&st, &headers) { Ok(s) => s, Err(r) => return r };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    let db = st.db.lock().unwrap();
    match user_by_id(&db, &id) {
        None => admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "no such user" })),
        Some(u) if u.role == "admin" => {
            admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "the admin account cannot be deleted" }))
        }
        Some(_) => {
            delete_user_cascade(&db, &id);
            admin_json(StatusCode::OK, serde_json::json!({ "ok": true }))
        }
    }
}

// ── Links ─────────────────────────────────────────────────────────────
fn normalize_url(raw: &str) -> String {
    let u = raw.trim();
    if u.contains("://") || u.starts_with('/') { u.to_string() } else { format!("https://{}", u) }
}

// ── Container links ──────────────────────────────────────────────────
// `ext+container://NAME/https://site…` (and the Firefox form
// `ext+container:name=NAME&url=<encoded>`) open the site in a named
// browser container via an OS-registered protocol handler (Brave
// Containers). The stored URL is the container URL verbatim; the site
// inside it is what favicons and fallbacks care about.

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let Some(hex) = s.get(i + 1..i + 3) {
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// The http(s) URL inside a container link, if this is one.
fn container_inner_url(u: &str) -> Option<String> {
    let rest = u.strip_prefix("ext+container:")?;
    if let Some(path) = rest.strip_prefix("//") {
        let slash = path.find('/')?;
        if path[..slash].is_empty() {
            return None;
        }
        let inner = &path[slash + 1..];
        if inner.starts_with("http://") || inner.starts_with("https://") {
            Some(inner.to_string())
        } else {
            None
        }
    } else {
        for part in rest.split('&') {
            if let Some(v) = part.strip_prefix("url=") {
                let inner = percent_decode(v);
                if inner.starts_with("http://") || inner.starts_with("https://") {
                    return Some(inner);
                }
            }
        }
        None
    }
}

fn is_container_url(u: &str) -> bool {
    u.starts_with("ext+container:") && container_inner_url(u).is_some()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

// ── Accounts: registration, email, password reset ────────────────────

fn valid_email(s: &str) -> bool {
    let s = s.trim();
    if s.len() < 5 || s.len() > 120 || s.chars().any(|c| c.is_whitespace()) || s.matches('@').count() != 1 {
        return false;
    }
    match s.split('@').nth(1) {
        Some(d) => d.contains('.') && !d.starts_with('.') && !d.ends_with('.'),
        None => false,
    }
}

fn registration_open(db: &Connection) -> bool {
    get_setting(db, "registration_open").as_deref() == Some("1")
}

fn profiles_enabled(db: &Connection) -> bool {
    get_setting(db, "profiles_enabled").as_deref() == Some("1")
}

/// The public base URL of this request (behind the proxy: forwarded
/// proto/host win) — used to build links in outgoing email.
fn request_base(headers: &HeaderMap) -> String {
    let first = |v: &str| v.split(',').next().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .and_then(first)
        .unwrap_or_else(|| "http".to_string());
    let host = headers
        .get("x-forwarded-host")
        .and_then(|v| v.to_str().ok())
        .and_then(first)
        .or_else(|| headers.get(header::HOST).and_then(|v| v.to_str().ok()).map(|s| s.to_string()))
        .unwrap_or_else(|| "localhost".to_string());
    format!("{proto}://{host}")
}

#[derive(Clone, Default)]
struct SmtpSettings {
    host: String,
    port: u16,
    username: String,
    password: String,
    from_addr: String,
    from_name: String,
    encryption: String, // "starttls" | "tls" | "none"
}

fn smtp_settings(db: &Connection) -> SmtpSettings {
    let g = |k: &str| get_setting(db, k).unwrap_or_default();
    let enc = g("smtp_encryption");
    SmtpSettings {
        host: g("smtp_host"),
        port: g("smtp_port").parse().unwrap_or(587),
        username: g("smtp_user"),
        password: g("smtp_pass"),
        from_addr: g("smtp_from_addr"),
        from_name: g("smtp_from_name"),
        encryption: if enc.is_empty() { "starttls".to_string() } else { enc },
    }
}

fn smtp_configured(s: &SmtpSettings) -> bool {
    !s.host.is_empty() && !s.from_addr.is_empty()
}

/// Send one plain-text email via the configured SMTP server. Errors are
/// returned as readable strings; callers decide whether they matter.
async fn send_mail(s: &SmtpSettings, to: &str, subject: &str, body: &str) -> Result<(), String> {
    use lettre::message::header::ContentType;
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

    if !smtp_configured(s) {
        return Err("SMTP is not configured (set a host and a From address first)".to_string());
    }
    let from: lettre::message::Mailbox = if s.from_name.is_empty() {
        s.from_addr.parse().map_err(|e| format!("bad From address: {e}"))?
    } else {
        let clean_name: String = s.from_name.chars().filter(|c| *c != '<' && *c != '>').collect();
        format!("{clean_name} <{}>", s.from_addr)
            .parse()
            .map_err(|e| format!("bad From: {e}"))?
    };
    let msg = Message::builder()
        .from(from)
        .to(to.parse().map_err(|e| format!("bad recipient: {e}"))?)
        .subject(subject.to_string())
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())
        .map_err(|e| e.to_string())?;
    let mut builder = match s.encryption.as_str() {
        "tls" => AsyncSmtpTransport::<Tokio1Executor>::relay(&s.host).map_err(|e| e.to_string())?,
        "none" => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&s.host),
        _ => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&s.host).map_err(|e| e.to_string())?,
    };
    builder = builder.port(s.port);
    if !s.username.is_empty() {
        builder = builder.credentials(Credentials::new(s.username.clone(), s.password.clone()));
    }
    let mailer = builder.build();
    match tokio::time::timeout(std::time::Duration::from_secs(20), mailer.send(msg)).await {
        Ok(r) => r.map(|_| ()).map_err(|e| e.to_string()),
        Err(_) => Err("SMTP timed out after 20s".to_string()),
    }
}

fn product_name(db: &Connection) -> String {
    get_branding(db)
        .get("product_name")
        .and_then(|v| v.as_str())
        .unwrap_or("Startpage")
        .to_string()
}

/// Fire-and-forget welcome email (only when SMTP is configured).
fn send_welcome(st: &Shared, to: String, username: String, base: String) {
    let s = {
        let db = st.db.lock().unwrap();
        smtp_settings(&db)
    };
    if !smtp_configured(&s) || to.is_empty() {
        return;
    }
    let product = {
        let db = st.db.lock().unwrap();
        product_name(&db)
    };
    tokio::spawn(async move {
        let subject = format!("Welcome to {product}");
        let body = format!(
            "Hi {username},\n\nYour {product} account is ready. Sign in here:\n{base}/login\n\nYour desktop starts empty — use Edit Desktop to add your own links and make it yours.\n"
        );
        let _ = send_mail(&s, &to, &subject, &body).await;
    });
}

#[derive(Deserialize, Clone)]
struct MemberRef {
    label: String,
    url: Option<String>,
}

#[derive(Deserialize)]
struct LinkInput {
    owner: Option<String>,
    label: String,
    kind: String,
    url: Option<String>,
    icon_mode: Option<String>,
    icon: Option<String>,
    section: String,
    col: Option<i64>,
    row: Option<i64>,
    sort: Option<i64>,
    group_title: Option<String>,
    /// Import/export only: for kind = "folder", the members it should hold,
    /// referenced by (label, url) so files stay portable across instances.
    members: Option<Vec<MemberRef>>,
    /// Panel only: put this (desktop) link into the given folder. The link
    /// stays section = 'desktop'; folder_id carries the membership.
    folder_id: Option<String>,
    /// Optional plate color behind the icon artwork ("#rrggbb").
    icon_bg: Option<String>,
}

/// Validate a requested folder assignment: the folder must exist, belong to
/// the link's owner, and be a folder. Folders themselves can't be members.
fn resolve_folder(db: &Connection, owner: &str, folder_id: &Option<String>, kind: &str) -> Result<Option<String>, String> {
    if kind == "folder" {
        return Ok(None);
    }
    match folder_id {
        None => Ok(None),
        Some(fid) => match link_full(db, fid) {
            Some(f) if f.owner_id == owner && f.kind == "folder" => Ok(Some(fid.clone())),
            _ => Err("no such folder for this owner".to_string()),
        },
    }
}

/// Validate a link payload; returns (owner, kind, label, url, icon_mode, icon, section, col, row, sort, group).
#[allow(clippy::type_complexity)]
fn validate_link(
    db: &Connection,
    input: &LinkInput,
    default_owner: &str,
    admin_id: &str,
) -> Result<(String, String, String, Option<String>, String, Option<String>, String, Option<i64>, Option<i64>, i64, Option<String>), String> {
    let owner = input.owner.clone().unwrap_or_else(|| default_owner.to_string());
    if user_by_id(db, &owner).is_none() {
        return Err("no such owner".to_string());
    }
    let label = input.label.trim().to_string();
    if label.is_empty() {
        return Err("label required".to_string());
    }
    let kind = input.kind.clone();
    if kind != "url" && kind != "admin" && kind != "folder" {
        return Err("kind must be url, admin or folder".to_string());
    }
    if kind == "admin" && owner != admin_id {
        return Err("admin links can only belong to the admin".to_string());
    }
    let url = if kind == "admin" || kind == "folder" {
        None
    } else {
        match input.url.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(u) if u.starts_with("ext+container:") => {
                if is_container_url(u) {
                    Some(u.to_string())
                } else {
                    return Err("container links must look like ext+container://NAME/https://site".to_string());
                }
            }
            Some(u) => Some(normalize_url(u)),
            None => return Err("url required".to_string()),
        }
    };
    let section = if kind == "folder" { "desktop".to_string() } else { input.section.clone() };
    if !["desktop", "dock", "apps", "about"].contains(&section.as_str()) {
        return Err("bad section".to_string());
    }
    let icon_mode = input.icon_mode.clone().unwrap_or_else(|| {
        match input.icon.as_deref() {
            Some(i) if i.starts_with("src/") => "bundled".to_string(),
            _ => "auto".to_string(),
        }
    });
    let icon = if icon_mode == "bundled" {
        match input.icon.as_deref() {
            Some(i) if i.starts_with("src/") && !i.contains("..") => Some(i.to_string()),
            Some(i) if i.strip_prefix("/uploads/").map(is_icon_filename).unwrap_or(false) => Some(i.to_string()),
            _ => Some("src/icons/globe.svg".to_string()),
        }
    } else {
        None // stored as /icon/<id>
    };
    let group = input.group_title.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| s.to_string());
    Ok((owner, kind, label, url, icon_mode, icon, section, input.col, input.row, input.sort.unwrap_or(0), group))
}

async fn admin_links(State(st): State<Shared>, headers: HeaderMap, Query(q): Query<HashMap<String, String>>) -> Response {
    let sess = match require_admin(&st, &headers) { Ok(s) => s, Err(r) => return r };
    let owner = q.get("owner").cloned().unwrap_or_else(|| sess.user.id.clone());
    let db = st.db.lock().unwrap();
    if user_by_id(&db, &owner).is_none() {
        return admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "no such owner" }));
    }
    let ids: Vec<String> = {
        let mut stmt = match db.prepare("SELECT id FROM links WHERE owner_id = ?1 ORDER BY section, sort, row, col") {
            Ok(s) => s,
            Err(_) => return admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "db error" })),
        };
        let m = stmt.query_map(params![owner], |r| r.get::<_, String>(0));
        match m { Ok(q2) => q2.filter_map(|x| x.ok()).collect(), Err(_) => Vec::new() }
    };
    let links: Vec<LinkFull> = ids.iter().filter_map(|id| link_full(&db, id)).collect();
    admin_json(StatusCode::OK, serde_json::json!({ "links": links }))
}

async fn admin_links_create(State(st): State<Shared>, headers: HeaderMap, Json(input): Json<LinkInput>) -> Response {
    let sess = match require_admin(&st, &headers) { Ok(s) => s, Err(r) => return r };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    let (id, url_for_icon) = {
        let db = st.db.lock().unwrap();
        let admin_id = admin_user(&db).map(|u| u.id).unwrap_or_default();
        let (owner, kind, label, url, icon_mode, icon, section, col, row, sort, group) = match validate_link(&db, &input, &sess.user.id, &admin_id) {
            Ok(v) => v,
            Err(msg) => return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": msg })),
        };
        let fid = match resolve_folder(&db, &owner, &input.folder_id, &kind) {
            Ok(f) => f,
            Err(msg) => return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": msg })),
        };
        let icon_stored = if icon_mode == "auto" { "auto" } else { icon.as_deref().unwrap_or("src/icons/globe.svg") };
        let id = insert_link(&db, &owner, &kind, &label, url.as_deref(), icon_stored, &section, col, row, sort, group.as_deref());
        if let Some(fid) = &fid {
            let _ = db.execute("UPDATE links SET folder_id = ?1 WHERE id = ?2", params![fid, id]);
        }
        if let Some(bg) = sanitize_icon_bg(&input.icon_bg) {
            let _ = db.execute("UPDATE links SET icon_bg = ?1 WHERE id = ?2", params![bg, id]);
        }
        let fetch = if icon_mode == "auto" && kind == "url" { url.clone() } else { None };
        (id, fetch)
    };
    if let Some(u) = url_for_icon {
        let st2 = st.clone();
        let id2 = id.clone();
        tokio::spawn(async move { refresh_favicon(&st2, &id2, &u).await; });
    }
    let db = st.db.lock().unwrap();
    match link_full(&db, &id) {
        Some(l) => admin_json(StatusCode::CREATED, serde_json::json!({ "link": l })),
        None => admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "create failed" })),
    }
}

async fn admin_links_update(State(st): State<Shared>, headers: HeaderMap, Path(id): Path<String>, Json(input): Json<LinkInput>) -> Response {
    let sess = match require_admin(&st, &headers) { Ok(s) => s, Err(r) => return r };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    let fetch = {
        let db = st.db.lock().unwrap();
        let existing = match link_full(&db, &id) {
            Some(l) => l,
            None => return admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "no such link" })),
        };
        let admin_id = admin_user(&db).map(|u| u.id).unwrap_or_default();
        // Owner is fixed at creation; validate against the existing owner.
        let mut input2 = input;
        input2.owner = Some(existing.owner_id.clone());
        let (_owner, kind, label, url, icon_mode, icon, section, col, row, sort, group) = match validate_link(&db, &input2, &existing.owner_id, &admin_id) {
            Ok(v) => v,
            Err(msg) => return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": msg })),
        };
        let fid = match resolve_folder(&db, &existing.owner_id, &input2.folder_id, &kind) {
            Ok(f) => f,
            Err(msg) => return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": msg })),
        };
        let icon_stored = if icon_mode == "auto" { format!("/icon/{}", id) } else { icon.unwrap_or_else(|| "src/icons/globe.svg".to_string()) };
        let _ = db.execute(
            "UPDATE links SET kind = ?1, label = ?2, url = ?3, icon = ?4, section = ?5, col = ?6, row = ?7, sort = ?8, group_title = ?9, folder_id = ?10, icon_bg = ?11 WHERE id = ?12",
            params![kind, label, url, icon_stored, section, col, row, sort, group, fid, sanitize_icon_bg(&input2.icon_bg), id],
        );
        if icon_mode == "auto" {
            let _ = db.execute("DELETE FROM favicons WHERE link_id = ?1", params![id]);
            if kind == "url" { url } else { None }
        } else {
            let _ = db.execute("DELETE FROM favicons WHERE link_id = ?1", params![id]);
            None
        }
    };
    if let Some(u) = fetch {
        let st2 = st.clone();
        let id2 = id.clone();
        tokio::spawn(async move { refresh_favicon(&st2, &id2, &u).await; });
    }
    let db = st.db.lock().unwrap();
    match link_full(&db, &id) {
        Some(l) => admin_json(StatusCode::OK, serde_json::json!({ "link": l })),
        None => admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "update failed" })),
    }
}

async fn admin_links_delete(State(st): State<Shared>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let sess = match require_admin(&st, &headers) { Ok(s) => s, Err(r) => return r };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    let db = st.db.lock().unwrap();
    let _ = db.execute("DELETE FROM favicons WHERE link_id = ?1", params![id]);
    let n = db.execute("DELETE FROM links WHERE id = ?1", params![id]).unwrap_or(0);
    if n == 0 {
        return admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "no such link" }));
    }
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true }))
}


// ── Link import / export ─────────────────────────────────────────────
// Portable JSON for one owner's whole link set. The export shape is the
// import shape: { app, format, owner, exported_at, links: [ {label, kind,
// url, icon, icon_mode, section, col, row, sort, group_title} ] }.
#[derive(Deserialize)]
struct ImportInput {
    owner: Option<String>,
    mode: Option<String>,
    links: Vec<LinkInput>,
}

async fn admin_links_export(State(st): State<Shared>, headers: HeaderMap, Query(q): Query<HashMap<String, String>>) -> Response {
    let sess = match require_admin(&st, &headers) { Ok(s) => s, Err(r) => return r };
    let owner = q.get("owner").cloned().unwrap_or_else(|| sess.user.id.clone());
    let db = st.db.lock().unwrap();
    let user = match user_by_id(&db, &owner) {
        Some(u) => u,
        None => return admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "no such owner" })),
    };
    let ids: Vec<String> = {
        let mut stmt = match db.prepare("SELECT id FROM links WHERE owner_id = ?1 ORDER BY section, sort, row, col") {
            Ok(s) => s,
            Err(_) => return admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "db error" })),
        };
        let m = stmt.query_map(params![owner], |r| r.get::<_, String>(0));
        match m { Ok(q2) => q2.filter_map(|x| x.ok()).collect(), Err(_) => Vec::new() }
    };
    let links: Vec<serde_json::Value> = ids.iter().filter_map(|id| link_full(&db, id)).map(|l| {
        let (icon, icon_mode) = if l.icon.starts_with("/icon/") {
            (serde_json::Value::Null, "auto")
        } else {
            (serde_json::Value::String(l.icon.clone()), "bundled")
        };
        let mut entry = serde_json::json!({
            "label": l.label, "kind": l.kind, "url": l.url, "icon": icon, "icon_mode": icon_mode,
            "section": l.section, "col": l.col, "row": l.row, "sort": l.sort, "group_title": l.group_title,
            "icon_bg": l.icon_bg
        });
        if l.kind == "folder" {
            let mut stmt = match db.prepare("SELECT label, url FROM links WHERE folder_id = ?1 ORDER BY sort, label") {
                Ok(s) => s,
                Err(_) => return entry,
            };
            let refs: Vec<serde_json::Value> = stmt
                .query_map(params![l.id], |r| {
                    Ok(serde_json::json!({ "label": r.get::<_, String>(0)?, "url": r.get::<_, Option<String>>(1)? }))
                })
                .map(|q| q.filter_map(|x| x.ok()).collect())
                .unwrap_or_default();
            entry["members"] = serde_json::json!(refs);
        }
        entry
    }).collect();
    let payload = serde_json::json!({
        "app": "startpage", "format": 1, "owner": user.username,
        "exported_at": now_secs(), "links": links
    });
    let fname = format!("startpage-links-{}.json", user.username);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", fname))
        .header("X-Robots-Tag", "noindex")
        .body(Body::from(serde_json::to_string_pretty(&payload).unwrap_or_default()))
        .unwrap()
}

async fn admin_links_import(State(st): State<Shared>, headers: HeaderMap, Json(input): Json<ImportInput>) -> Response {
    let sess = match require_admin(&st, &headers) { Ok(s) => s, Err(r) => return r };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    let mode = input.mode.clone().unwrap_or_else(|| "replace".to_string());
    if mode != "replace" && mode != "append" {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "mode must be replace or append" }));
    }
    let (imported, skipped, errors, to_fetch) = {
        let db = st.db.lock().unwrap();
        let owner = input.owner.clone().unwrap_or_else(|| sess.user.id.clone());
        if user_by_id(&db, &owner).is_none() {
            return admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "no such owner" }));
        }
        let admin_id = admin_user(&db).map(|u| u.id).unwrap_or_default();
        if mode == "replace" {
            let _ = db.execute("DELETE FROM favicons WHERE link_id IN (SELECT id FROM links WHERE owner_id = ?1)", params![owner]);
            let _ = db.execute("DELETE FROM links WHERE owner_id = ?1", params![owner]);
        }
        // Existing (section, url) pairs drive append-mode dedupe.
        let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
        if mode == "append" {
            if let Ok(mut stmt) = db.prepare("SELECT section, url FROM links WHERE owner_id = ?1 AND url IS NOT NULL") {
                if let Ok(q) = stmt.query_map(params![owner], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))) {
                    for pair in q.flatten() { seen.insert(pair); }
                }
            }
        }
        let mut imported = 0usize;
        let mut skipped = 0usize;
        let mut errors: Vec<String> = Vec::new();
        let mut to_fetch: Vec<(String, String)> = Vec::new();
        let mut folder_specs: Vec<(String, String, Vec<MemberRef>)> = Vec::new();
        for item in &input.links {
            let it = LinkInput {
                owner: Some(owner.clone()),
                label: item.label.clone(),
                kind: item.kind.clone(),
                url: item.url.clone(),
                icon_mode: item.icon_mode.clone(),
                icon: item.icon.clone(),
                section: item.section.clone(),
                col: item.col,
                row: item.row,
                sort: item.sort,
                group_title: item.group_title.clone(),
                members: None,
                folder_id: None,
                icon_bg: None,
            };
            let (_o, kind, label, url, icon_mode, icon, section, col, row, sort, group) =
                match validate_link(&db, &it, &owner, &admin_id) {
                    Ok(v) => v,
                    Err(msg) => { errors.push(format!("{}: {}", item.label, msg)); continue; }
                };
            if mode == "append" {
                if let Some(u) = &url {
                    if !seen.insert((section.clone(), u.clone())) { skipped += 1; continue; }
                }
            }
            let icon_stored = if icon_mode == "auto" { "auto" } else { icon.as_deref().unwrap_or("src/icons/globe.svg") };
            let id = insert_link(&db, &owner, &kind, &label, url.as_deref(), icon_stored, &section, col, row, sort, group.as_deref());
            if icon_mode == "auto" {
                if let Some(u) = &url { to_fetch.push((id.clone(), u.clone())); }
            }
            if let Some(bg) = sanitize_icon_bg(&item.icon_bg) {
                let _ = db.execute("UPDATE links SET icon_bg = ?1 WHERE id = ?2", params![bg, id]);
            }
            if kind == "folder" {
                if let Some(mrefs) = &item.members {
                    folder_specs.push((id, label.clone(), mrefs.clone()));
                }
            }
            imported += 1;
        }
        // Folder membership resolves after all entries exist: members are
        // matched by (label, url) among the owner's ungrouped desktop links.
        for (fid, flabel, mrefs) in &folder_specs {
            for mref in mrefs {
                let cand: Option<String> = {
                    let mut stmt = match db.prepare(
                        "SELECT id, url FROM links WHERE owner_id = ?1 AND kind != 'folder' AND section = 'desktop' AND folder_id IS NULL AND label = ?2",
                    ) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    let rows: Vec<(String, Option<String>)> = stmt
                        .query_map(params![owner, mref.label], |r| Ok((r.get(0)?, r.get(1)?)))
                        .map(|q| q.filter_map(|x| x.ok()).collect())
                        .unwrap_or_default();
                    rows.into_iter().find(|(_, u)| *u == mref.url).map(|(i, _)| i)
                };
                match cand {
                    Some(mid) => {
                        let _ = db.execute("UPDATE links SET folder_id = ?1 WHERE id = ?2", params![fid, mid]);
                    }
                    None => errors.push(format!("folder '{}': member '{}' not found in the import", flabel, mref.label)),
                }
            }
        }
        (imported, skipped, errors, to_fetch)
    };
    warm_favicons(&st, to_fetch);
    admin_json(StatusCode::OK, serde_json::json!({
        "imported": imported, "skipped_duplicates": skipped, "errors": errors
    }))
}

// ── Sync tokens ───────────────────────────────────────────────────────
fn issue_sync_token(db: &Connection, user_id: &str, name: &str) -> (String, String) {
    let id = random_id();
    let token = random_hex(32);
    let _ = db.execute(
        "INSERT INTO sync_tokens(id, user_id, name, token_hash, created_at, last_used_at) VALUES(?1, ?2, ?3, ?4, ?5, NULL)",
        params![id, user_id, name, sha256_hex(&token), now_secs()],
    );
    (id, token)
}

fn token_json(db: &Connection, id: &str) -> Option<serde_json::Value> {
    db.query_row(
        "SELECT t.id, t.user_id, u.username, t.name, t.created_at, t.last_used_at FROM sync_tokens t JOIN users u ON u.id = t.user_id WHERE t.id = ?1",
        params![id],
        |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?, "user_id": r.get::<_, String>(1)?, "username": r.get::<_, String>(2)?,
                "name": r.get::<_, String>(3)?, "created_at": r.get::<_, i64>(4)?, "last_used_at": r.get::<_, Option<i64>>(5)?
            }))
        },
    )
    .ok()
}

fn list_tokens(db: &Connection, user_id: Option<&str>) -> Vec<serde_json::Value> {
    let sql = match user_id {
        Some(_) => "SELECT id FROM sync_tokens WHERE user_id = ?1 ORDER BY created_at",
        None => "SELECT id FROM sync_tokens ORDER BY created_at",
    };
    let mut stmt = match db.prepare(sql) { Ok(s) => s, Err(_) => return Vec::new() };
    let ids: Vec<String> = match user_id {
        Some(u) => stmt.query_map(params![u], |r| r.get::<_, String>(0)).map(|q| q.filter_map(|x| x.ok()).collect()).unwrap_or_default(),
        None => stmt.query_map([], |r| r.get::<_, String>(0)).map(|q| q.filter_map(|x| x.ok()).collect()).unwrap_or_default(),
    };
    drop(stmt);
    ids.iter().filter_map(|id| token_json(db, id)).collect()
}

#[derive(Deserialize)]
struct CreateTokenReq {
    name: String,
    user_id: Option<String>,
}

// ── Extension packaging ─────────────────────────────────────────────
// The panel builds ready-to-load extension zips from the very files this
// server serves (the same index.html + src/), plus the packaging/ extras:
// manifest, background sync worker, pairing page, icons. The product name
// and tagline from Branding are baked into the manifest, so the extension
// carries the same identity as the page. Mirrors packaging/build.py.

#[derive(Deserialize)]
struct ExtensionQuery {
    flavor: Option<String>,
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() { "startpage".to_string() } else { out }
}

fn collect_dir(root: &std::path::Path, base: &std::path::Path, out: &mut Vec<(String, Vec<u8>)>) {
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_dir(&p, base, out);
            } else if let Ok(bytes) = std::fs::read(&p) {
                if let Ok(rel) = p.strip_prefix(base) {
                    out.push((rel.to_string_lossy().replace('\\', "/"), bytes));
                }
            }
        }
    }
}

fn build_extension_zip(st: &Shared, flavor: &str) -> Result<(String, Vec<u8>), String> {
    use std::io::Write;
    let branding = {
        let db = st.db.lock().unwrap();
        get_branding(&db)
    };
    let name = branding
        .get("product_name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Startpage")
        .to_string();
    let desc = branding
        .get("tagline")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Your startpage links as a desktop on every new tab — synced from your own server and cached for offline use.".to_string());

    // Manifest: the packaging base, rebranded; Firefox gets its dialect.
    let manifest_raw = std::fs::read_to_string(st.packaging_dir.join("manifest.json"))
        .map_err(|e| format!("cannot read packaging/manifest.json: {e}"))?;
    let mut m: serde_json::Value = serde_json::from_str(&manifest_raw).map_err(|e| e.to_string())?;
    let obj = m.as_object_mut().ok_or("manifest is not a JSON object")?;
    obj.insert("name".into(), serde_json::Value::String(name.clone()));
    obj.insert("short_name".into(), serde_json::Value::String(name.clone()));
    obj.insert("description".into(), serde_json::Value::String(desc));
    obj.remove("homepage_url");
    if flavor == "firefox" {
        let host_perms = obj.remove("optional_host_permissions").unwrap_or(serde_json::json!([]));
        obj.remove("background");
        obj.insert("background".into(), serde_json::json!({ "scripts": ["background.js"] }));
        obj.insert("optional_permissions".into(), host_perms);
        obj.insert("chrome_settings_overrides".into(), serde_json::json!({ "homepage": "index.html" }));
        obj.insert(
            "browser_specific_settings".into(),
            serde_json::json!({ "gecko": {
                "id": "doxbox@danverskara",
                "strict_min_version": "140.0",
                "data_collection_permissions": { "required": ["none"] }
            } }),
        );
    }

    // The new-tab page reads "New Tab" in the tab strip, not the site title.
    let mut html = std::fs::read_to_string(st.web_dir.join("index.html")).map_err(|e| e.to_string())?;
    if let (Some(a), Some(b)) = (html.find("<title>"), html.find("</title>")) {
        if b > a {
            html = format!("{}<title>New Tab</title>{}", &html[..a], &html[b + 8..]);
        }
    }

    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    entries.push(("index.html".to_string(), html.into_bytes()));
    collect_dir(&st.web_dir.join("src"), &st.web_dir, &mut entries);
    for f in ["background.js", "options.html", "options.js"] {
        let bytes = std::fs::read(st.packaging_dir.join(f)).map_err(|e| format!("cannot read packaging/{f}: {e}"))?;
        entries.push((f.to_string(), bytes));
    }
    collect_dir(&st.packaging_dir.join("icons"), &st.packaging_dir, &mut entries);
    let manifest_text = serde_json::to_string_pretty(&m).map_err(|e| e.to_string())?;
    entries.push(("manifest.json".to_string(), manifest_text.into_bytes()));
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (path, bytes) in &entries {
        zw.start_file(path, opts).map_err(|e| e.to_string())?;
        zw.write_all(bytes).map_err(|e| e.to_string())?;
    }
    let cursor = zw.finish().map_err(|e| e.to_string())?;
    Ok((format!("{}-{}.zip", slugify(&name), flavor), cursor.into_inner()))
}

async fn admin_extension(State(st): State<Shared>, headers: HeaderMap, Query(q): Query<ExtensionQuery>) -> Response {
    // Any signed-in user may build one (the account page offers it too):
    // the package contains only the public page files and packaging assets.
    // Signed-out visitors following the About-card chip link land on the
    // sign-in page instead of a raw JSON error.
    if session_from_headers(&st, &headers).is_none() {
        let wants_html = headers
            .get(header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .map(|a| a.contains("text/html"))
            .unwrap_or(false);
        if wants_html {
            return redirect_to("/login");
        }
        if let Err(r) = require_session(&st, &headers) { return r; }
    }
    let flavor = if q.flavor.as_deref() == Some("firefox") { "firefox" } else { "chrome" };
    match build_extension_zip(&st, flavor) {
        Ok((fname, bytes)) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/zip")
            .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{fname}\""))
            .body(Body::from(bytes))
            .unwrap(),
        Err(e) => admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": e })),
    }
}

async fn admin_tokens(State(st): State<Shared>, headers: HeaderMap) -> Response {
    if let Err(r) = require_admin(&st, &headers) { return r; }
    let db = st.db.lock().unwrap();
    admin_json(StatusCode::OK, serde_json::json!({ "tokens": list_tokens(&db, None) }))
}

async fn admin_tokens_create(State(st): State<Shared>, headers: HeaderMap, Json(body): Json<CreateTokenReq>) -> Response {
    let sess = match require_admin(&st, &headers) { Ok(s) => s, Err(r) => return r };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "device name required" }));
    }
    let db = st.db.lock().unwrap();
    let target = body.user_id.clone().unwrap_or_else(|| sess.user.id.clone());
    if user_by_id(&db, &target).is_none() {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "no such user" }));
    }
    let (id, token) = issue_sync_token(&db, &target, &name);
    let mut out = token_json(&db, &id).unwrap_or(serde_json::json!({}));
    out["token"] = serde_json::json!(token); // plaintext — shown exactly once
    admin_json(StatusCode::CREATED, out)
}

async fn admin_tokens_delete(State(st): State<Shared>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let sess = match require_admin(&st, &headers) { Ok(s) => s, Err(r) => return r };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    let db = st.db.lock().unwrap();
    let n = db.execute("DELETE FROM sync_tokens WHERE id = ?1", params![id]).unwrap_or(0);
    if n == 0 {
        return admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "no such token" }));
    }
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true }))
}

async fn me_tokens(State(st): State<Shared>, headers: HeaderMap) -> Response {
    let sess = match require_session(&st, &headers) { Ok(s) => s, Err(r) => return r };
    let db = st.db.lock().unwrap();
    admin_json(StatusCode::OK, serde_json::json!({ "tokens": list_tokens(&db, Some(&sess.user.id)) }))
}

async fn me_tokens_create(State(st): State<Shared>, headers: HeaderMap, Json(body): Json<CreateTokenReq>) -> Response {
    let sess = match require_session(&st, &headers) { Ok(s) => s, Err(r) => return r };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    let name = body.name.trim().to_string();
    if name.is_empty() {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "device name required" }));
    }
    let db = st.db.lock().unwrap();
    let (id, token) = issue_sync_token(&db, &sess.user.id, &name);
    let mut out = token_json(&db, &id).unwrap_or(serde_json::json!({}));
    out["token"] = serde_json::json!(token);
    admin_json(StatusCode::CREATED, out)
}

async fn me_tokens_delete(State(st): State<Shared>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let sess = match require_session(&st, &headers) { Ok(s) => s, Err(r) => return r };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    let db = st.db.lock().unwrap();
    let n = db
        .execute("DELETE FROM sync_tokens WHERE id = ?1 AND user_id = ?2", params![id, sess.user.id])
        .unwrap_or(0);
    if n == 0 {
        return admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "no such token" }));
    }
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true }))
}

// ── Passwords / settings / branding / icons ───────────────────────────
#[derive(Deserialize)]
struct ChangePasswordReq {
    current: String,
    new: String,
}

fn change_own_password(st: &Shared, sess: &Session, body: &ChangePasswordReq) -> Response {
    if body.new.len() < 10 {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "password must be at least 10 characters" }));
    }
    let db = st.db.lock().unwrap();
    let current_hash: Option<String> = db
        .query_row("SELECT password_hash FROM users WHERE id = ?1", params![sess.user.id], |r| r.get(0))
        .ok();
    match current_hash {
        Some(h) if verify_password(&body.current, &h) => {}
        _ => return admin_json(StatusCode::UNAUTHORIZED, serde_json::json!({ "error": "current password is wrong" })),
    }
    let hash = match hash_password(&body.new) {
        Ok(h) => h,
        Err(_) => return admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "hashing failed" })),
    };
    let _ = db.execute("UPDATE users SET password_hash = ?1 WHERE id = ?2", params![hash, sess.user.id]);
    // Revoke the user's OTHER sessions; the current one survives.
    let _ = db.execute(
        "DELETE FROM sessions WHERE user_id = ?1 AND token_hash != ?2",
        params![sess.user.id, sess.token_hash],
    );
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true }))
}

async fn admin_password(State(st): State<Shared>, headers: HeaderMap, Json(body): Json<ChangePasswordReq>) -> Response {
    let sess = match require_admin(&st, &headers) { Ok(s) => s, Err(r) => return r };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    change_own_password(&st, &sess, &body)
}

async fn me_password(State(st): State<Shared>, headers: HeaderMap, Json(body): Json<ChangePasswordReq>) -> Response {
    let sess = match require_session(&st, &headers) { Ok(s) => s, Err(r) => return r };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    change_own_password(&st, &sess, &body)
}

async fn admin_settings(State(st): State<Shared>, headers: HeaderMap) -> Response {
    if let Err(r) = require_admin(&st, &headers) { return r; }
    let db = st.db.lock().unwrap();
    admin_json(StatusCode::OK, serde_json::json!({ "admin_path": admin_path(&db) }))
}

#[derive(Deserialize)]
struct SettingsReq {
    admin_path: String,
}

async fn admin_settings_update(State(st): State<Shared>, headers: HeaderMap, Json(body): Json<SettingsReq>) -> Response {
    let sess = match require_admin(&st, &headers) { Ok(s) => s, Err(r) => return r };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    let p = body.admin_path.trim().trim_matches('/');
    let valid = p.len() >= 8
        && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !RESERVED_PATHS.contains(&p);
    if !valid {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({
            "error": "admin path must be a single segment of [a-z0-9-], at least 8 characters, and not a reserved word"
        }));
    }
    let db = st.db.lock().unwrap();
    set_setting(&db, "admin_path", p);
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true, "admin_path": p }))
}

async fn admin_branding(State(st): State<Shared>, headers: HeaderMap) -> Response {
    if let Err(r) = require_admin(&st, &headers) { return r; }
    let db = st.db.lock().unwrap();
    admin_json(StatusCode::OK, serde_json::json!({ "branding": get_branding(&db) }))
}

async fn admin_branding_update(State(st): State<Shared>, headers: HeaderMap, Json(body): Json<serde_json::Value>) -> Response {
    let sess = match require_admin(&st, &headers) { Ok(s) => s, Err(r) => return r };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    let mut clean = serde_json::Map::new();
    for key in ["product_name", "tagline", "about_note", "profile_name", "profile_sub", "app_icon", "chrome_url", "edge_url", "firefox_url"] {
        if let Some(v) = body.get(key).and_then(|v| v.as_str()) {
            clean.insert(key.to_string(), serde_json::json!(v.trim()));
        }
    }
    for key in ["chrome_url", "edge_url", "firefox_url"] {
        if let Some(v) = clean.get(key).and_then(|v| v.as_str()) {
            if !v.is_empty() && !(v.starts_with("https://") || v.starts_with("http://") || v.starts_with('/')) {
                return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "browser links must start with https://, http:// or /" }));
            }
        }
    }
    if let Some(icon) = clean.get("app_icon").and_then(|v| v.as_str()) {
        if !icon.starts_with("src/") || icon.contains("..") {
            return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "app_icon must be a bundled asset path (src/…)" }));
        }
    }
    if let Some(rows) = body.get("profile_rows").and_then(|v| v.as_array()) {
        let rows: Vec<serde_json::Value> = rows
            .iter()
            .filter_map(|r| {
                let label = r.get("label")?.as_str()?.trim().to_string();
                let value = r.get("value")?.as_str()?.trim().to_string();
                Some(serde_json::json!({ "label": label, "value": value }))
            })
            .collect();
        clean.insert("profile_rows".to_string(), serde_json::json!(rows));
    }
    let db = st.db.lock().unwrap();
    set_setting(&db, "branding", &serde_json::Value::Object(clean).to_string());
    admin_json(StatusCode::OK, serde_json::json!({ "ok": true, "branding": get_branding(&db) }))
}

async fn admin_icons(State(st): State<Shared>, headers: HeaderMap) -> Response {
    if let Err(r) = require_admin(&st, &headers) { return r; }
    let mut icons = Vec::new();
    for sub in ["src/icons", "src/social"] {
        if let Ok(rd) = std::fs::read_dir(st.web_dir.join(sub)) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.ends_with(".svg") || name.ends_with(".png") || name.ends_with(".jpg") {
                    icons.push(format!("{}/{}", sub, name));
                }
            }
        }
    }
    // The app icon itself is also a valid pick for branding.
    if st.web_dir.join("src/app-icon.svg").exists() {
        icons.push("src/app-icon.svg".to_string());
    }
    // The admin's own uploaded icons (data dir — they survive rebuilds).
    if let Ok(rd) = std::fs::read_dir(st.data_dir.join("uploads/icons")) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if is_icon_filename(&name) {
                icons.push(format!("/uploads/{}", name));
            }
        }
    }
    icons.sort();
    icons.dedup();
    admin_json(StatusCode::OK, serde_json::json!({ "icons": icons }))
}

// ── Uploaded icons ───────────────────────────────────────────────────
// Admin-uploaded images live in the DATA dir (uploads/icons), so they
// survive container rebuilds and can be picked anywhere a bundled icon
// can (link icons, the branding app icon). Stored paths look like
// "/uploads/<file>" and are served by the public /uploads/:file route —
// icons are public assets, same as the bundled ones.

const ICON_EXTS: [&str; 7] = ["png", "svg", "ico", "jpg", "jpeg", "webp", "gif"];

fn is_icon_filename(name: &str) -> bool {
    if name.is_empty() || name.len() > 100 || name.starts_with('.') {
        return false;
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
        return false;
    }
    match name.rsplit('.').next() {
        Some(ext) => ICON_EXTS.contains(&ext.to_lowercase().as_str()),
        None => false,
    }
}

/// Reduce an uploaded filename to a safe bare icon filename (or reject).
fn sanitize_icon_name(raw: &str) -> Option<String> {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '-' })
        .collect();
    let cleaned = cleaned.trim_matches(|c| c == '.' || c == '-').to_string();
    if is_icon_filename(&cleaned) { Some(cleaned) } else { None }
}

fn icon_content_matches(ext: &str, bytes: &[u8]) -> bool {
    match ext {
        "png" => bytes.starts_with(b"\x89PNG"),
        "jpg" | "jpeg" => bytes.starts_with(b"\xff\xd8"),
        "ico" => bytes.starts_with(b"\x00\x00\x01\x00") || bytes.starts_with(b"\x00\x00\x02\x00"),
        "gif" => bytes.starts_with(b"GIF8"),
        "webp" => bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
        "svg" => {
            let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).to_lowercase();
            head.contains("<svg")
        }
        _ => false,
    }
}

fn icon_content_type(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ico" => "image/x-icon",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

async fn serve_upload(State(st): State<Shared>, Path(file): Path<String>) -> Response {
    if !is_icon_filename(&file) {
        return err_json(StatusCode::NOT_FOUND, "not found");
    }
    match std::fs::read(st.data_dir.join("uploads/icons").join(&file)) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, icon_content_type(&file))
            .header(header::CACHE_CONTROL, "no-cache")
            .header("X-Content-Type-Options", "nosniff")
            .header(header::CONTENT_SECURITY_POLICY, "script-src 'none'; sandbox")
            .body(Body::from(bytes))
            .unwrap(),
        Err(_) => err_json(StatusCode::NOT_FOUND, "not found"),
    }
}

#[derive(Deserialize)]
struct IconUploadReq {
    filename: String,
    data_base64: String,
}

async fn admin_icons_upload(State(st): State<Shared>, headers: HeaderMap, Json(input): Json<IconUploadReq>) -> Response {
    let sess = match require_admin(&st, &headers) { Ok(s) => s, Err(r) => return r };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    let name = match sanitize_icon_name(&input.filename) {
        Some(n) => n,
        None => return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "filename must end in .png, .svg, .ico, .jpg, .jpeg, .webp or .gif" })),
    };
    let cleaned_b64: String = input.data_base64.chars().filter(|c| !c.is_whitespace()).collect();
    let payload = cleaned_b64.rsplit(',').next().unwrap_or("");
    use base64::Engine;
    let bytes = match base64::engine::general_purpose::STANDARD.decode(payload) {
        Ok(b) => b,
        Err(_) => return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "could not decode the file data" })),
    };
    if bytes.is_empty() || bytes.len() > 512 * 1024 {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "icon must be between 1 byte and 512 KB" }));
    }
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    if !icon_content_matches(&ext, &bytes) {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "the file's content does not match its extension" }));
    }
    let dir = st.data_dir.join("uploads/icons");
    if std::fs::create_dir_all(&dir).is_err() {
        return admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": "could not create the uploads dir" }));
    }
    // Same filename = replace, by design.
    match std::fs::write(dir.join(&name), &bytes) {
        Ok(()) => admin_json(StatusCode::OK, serde_json::json!({ "ok": true, "path": format!("/uploads/{}", name) })),
        Err(e) => admin_json(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "error": format!("write failed: {e}") })),
    }
}

async fn admin_icons_delete(State(st): State<Shared>, headers: HeaderMap, Path(file): Path<String>) -> Response {
    let sess = match require_admin(&st, &headers) { Ok(s) => s, Err(r) => return r };
    if let Err(r) = require_csrf(&sess, &headers) { return r; }
    if !is_icon_filename(&file) {
        return admin_json(StatusCode::BAD_REQUEST, serde_json::json!({ "error": "bad filename" }));
    }
    match std::fs::remove_file(st.data_dir.join("uploads/icons").join(&file)) {
        Ok(()) => admin_json(StatusCode::OK, serde_json::json!({ "ok": true })),
        Err(_) => admin_json(StatusCode::NOT_FOUND, serde_json::json!({ "error": "no such upload" })),
    }
}

// ─────────────────────────────────────────────────────────────────────
// Static files (the public web root: index.html + src/)
// ─────────────────────────────────────────────────────────────────────
fn content_type_for(path: &FsPath) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "json" => "application/json",
        "md" => "text/markdown; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Serve `rel` from the web root, with a canonicalization guard so nothing
/// outside the root (no ../, no symlinks out) is ever reachable.
fn serve_static(st: &Shared, rel: &str) -> Option<Response> {
    if rel.is_empty() || rel.contains("..") || rel.contains('\\') {
        return None;
    }
    let root = std::fs::canonicalize(&st.web_dir).ok()?;
    let candidate = root.join(rel);
    let canon = std::fs::canonicalize(&candidate).ok()?;
    if !canon.starts_with(&root) || !canon.is_file() {
        return None;
    }
    let bytes = std::fs::read(&canon).ok()?;
    Some(
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type_for(&canon))
            // Page files must never go stale: with no cache headers at all,
            // browsers and the CDN edge heuristically cached old CSS/JS and
            // served them long after an update shipped. no-cache forces a
            // fresh copy to be confirmed on every load (the files are small).
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(bytes))
            .unwrap(),
    )
}

async fn fallback(State(st): State<Shared>, uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let rel = if path.is_empty() { "index.html" } else { path };
    match serve_static(&st, rel) {
        Some(r) => r,
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn healthz() -> &'static str {
    "ok"
}

// ─────────────────────────────────────────────────────────────────────
// Startup
// ─────────────────────────────────────────────────────────────────────
fn init_db(data_dir: &FsPath) -> Connection {
    std::fs::create_dir_all(data_dir).expect("create DATA_DIR");
    let db = Connection::open(data_dir.join("startpage.db")).expect("open sqlite db");
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS users(
            id TEXT PRIMARY KEY, username TEXT UNIQUE NOT NULL, password_hash TEXT NOT NULL,
            role TEXT NOT NULL, disabled INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT);
         CREATE TABLE IF NOT EXISTS links(
            id TEXT PRIMARY KEY, owner_id TEXT NOT NULL, kind TEXT NOT NULL, label TEXT NOT NULL,
            url TEXT, icon TEXT NOT NULL, section TEXT NOT NULL, col INTEGER, row INTEGER,
            sort INTEGER NOT NULL DEFAULT 0, group_title TEXT);
         CREATE TABLE IF NOT EXISTS favicons(link_id TEXT PRIMARY KEY, content_type TEXT NOT NULL, bytes BLOB NOT NULL);
         CREATE TABLE IF NOT EXISTS sessions(
            token_hash TEXT PRIMARY KEY, csrf TEXT NOT NULL, user_id TEXT NOT NULL, expires INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS sync_tokens(
            id TEXT PRIMARY KEY, user_id TEXT NOT NULL, name TEXT NOT NULL, token_hash TEXT NOT NULL,
            created_at INTEGER NOT NULL, last_used_at INTEGER);
         CREATE TABLE IF NOT EXISTS password_resets(
            token_hash TEXT PRIMARY KEY, user_id TEXT NOT NULL, expires INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS reports(
            id TEXT PRIMARY KEY, link_id TEXT NOT NULL, label TEXT NOT NULL, owner_id TEXT NOT NULL,
            reporter TEXT NOT NULL, reason TEXT NOT NULL, ip TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'open', created_at INTEGER NOT NULL, source TEXT);
         CREATE TABLE IF NOT EXISTS shares(
            id TEXT PRIMARY KEY, token TEXT NOT NULL UNIQUE, link_id TEXT NOT NULL,
            owner_id TEXT NOT NULL, expires_at INTEGER, views INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL);",
    )
    .expect("create tables");
    // Report origin (v1.17.3): where the report was filed from
    // ("share:/s/<token>", "profile:/u/<name>" or "startpage").
    {
        let mut stmt = db.prepare("PRAGMA table_info(reports)").expect("pragma table_info reports");
        let cols: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .expect("pragma rows")
            .filter_map(|c| c.ok())
            .collect();
        if !cols.iter().any(|c| c == "source") {
            db.execute("ALTER TABLE reports ADD COLUMN source TEXT", []).expect("add reports.source column");
        }
    }
    // Desktop folders: links.folder_id points at the folder's own link row
    // (kind = 'folder'). Guarded ALTER — idempotent, safe on every boot,
    // including databases created before folders existed.
    let has_folder_id = {
        let mut stmt = db.prepare("PRAGMA table_info(links)").expect("pragma table_info");
        let cols: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .expect("pragma rows")
            .filter_map(|c| c.ok())
            .collect();
        cols.iter().any(|c| c == "folder_id")
    };
    if !has_folder_id {
        db.execute("ALTER TABLE links ADD COLUMN folder_id TEXT", []).expect("add folder_id column");
    }
    // Icon background color (v1.7.0): a hex plate color rendered behind an
    // icon's artwork — light glyphs on transparency vanish against the
    // default white plate, so the owner can pick a contrasting one.
    let has_icon_bg = {
        let mut stmt = db.prepare("PRAGMA table_info(links)").expect("pragma table_info");
        let cols: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .expect("pragma rows")
            .filter_map(|c| c.ok())
            .collect();
        cols.iter().any(|c| c == "icon_bg")
    };
    if !has_icon_bg {
        db.execute("ALTER TABLE links ADD COLUMN icon_bg TEXT", []).expect("add icon_bg column");
    }
    // User email (v1.12.0): welcome mail + password reset need an address
    // on file. Guarded ALTER like the others; the partial unique index
    // keeps one account per email without constraining NULLs.
    let has_email = {
        let mut stmt = db.prepare("PRAGMA table_info(users)").expect("pragma table_info users");
        let cols: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .expect("pragma rows")
            .filter_map(|c| c.ok())
            .collect();
        cols.iter().any(|c| c == "email")
    };
    if !has_email {
        db.execute("ALTER TABLE users ADD COLUMN email TEXT", []).expect("add users.email column");
    }
    db.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email ON users(email) WHERE email IS NOT NULL",
        [],
    )
    .expect("users email index");
    // Profile fields (v1.15.0), ported from the user's LinkPlatform:
    // display name / bio / avatar / header / accent + a per-user public
    // switch. The feature as a whole is gated by the admin (site setting
    // profiles_enabled). Guarded ALTERs like every other migration here.
    {
        let mut stmt = db.prepare("PRAGMA table_info(users)").expect("pragma table_info users");
        let cols: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .expect("pragma rows")
            .filter_map(|c| c.ok())
            .collect();
        for (name, ddl) in [
            ("display_name", "TEXT NOT NULL DEFAULT ''"),
            ("bio", "TEXT NOT NULL DEFAULT ''"),
            ("avatar_path", "TEXT"),
            ("header_path", "TEXT"),
            ("accent_color", "TEXT"),
            ("profile_public", "INTEGER NOT NULL DEFAULT 0"),
        ] {
            if !cols.iter().any(|c| c == name) {
                db.execute(&format!("ALTER TABLE users ADD COLUMN {name} {ddl}"), [])
                    .expect("add users profile column");
            }
        }
    }
    db
}

/// Normalize an icon background value to "#rgb" / "#rrggbb", or None.
fn sanitize_icon_bg(raw: &Option<String>) -> Option<String> {
    let s = raw.as_deref()?.trim();
    let hex = s.strip_prefix('#')?;
    if (hex.len() == 6 || hex.len() == 3) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(format!("#{}", hex.to_lowercase()))
    } else {
        None
    }
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(8080);
    let data_dir = PathBuf::from(std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string()));
    let web_dir = match std::env::var("WEB_DIR") {
        Ok(w) => PathBuf::from(w),
        Err(_) => {
            // Repo layout: the static root is the project root. Prefer ./web
            // when it actually holds the page (the Docker image sets WEB_DIR
            // explicitly anyway).
            let dot = PathBuf::from(".");
            let web = PathBuf::from("./web");
            if web.join("index.html").exists() { web } else { dot }
        }
    };

    let db = init_db(&data_dir);

    // Password recovery: RESET_ADMIN=1 deletes the admin account (its links
    // are parked and re-adopted by the next setup) and every session.
    if std::env::var("RESET_ADMIN").map(|v| v == "1").unwrap_or(false) {
        if let Some(admin) = admin_user(&db) {
            let _ = db.execute("UPDATE links SET owner_id = ?1 WHERE owner_id = ?2", params![PENDING_OWNER, admin.id]);
            let _ = db.execute("DELETE FROM sessions WHERE user_id = ?1", params![admin.id]);
            let _ = db.execute("DELETE FROM users WHERE id = ?1", params![admin.id]);
            eprintln!("WARNING: RESET_ADMIN=1 — admin account deleted; visit the admin path to set up a new one. All sessions revoked.");
        } else {
            eprintln!("WARNING: RESET_ADMIN=1 set but no admin account exists.");
        }
    }

    // The obscure admin path is generated once and printed exactly once.
    if get_setting(&db, "admin_path").is_none() {
        let p = format!("panel-{}", random_hex(8));
        set_setting(&db, "admin_path", &p);
        println!("ADMIN LOGIN PATH: /{} (shown once — the disguised admin link also leads here)", p);
    }

    // Favicon fetcher. Honour the conventional proxy env vars so it also
    // works on networks where egress goes through an HTTP proxy.
    let mut http_builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .user_agent("Mozilla/5.0 (compatible; startpage-server/1.0)");
    if let Ok(p) = std::env::var("HTTPS_PROXY").or_else(|_| std::env::var("https_proxy")) {
        if let Ok(proxy) = reqwest::Proxy::https(&p) {
            http_builder = http_builder.proxy(proxy);
        }
    }
    if let Ok(p) = std::env::var("HTTP_PROXY").or_else(|_| std::env::var("http_proxy")) {
        if let Ok(proxy) = reqwest::Proxy::http(&p) {
            http_builder = http_builder.proxy(proxy);
        }
    }
    let http = http_builder.build().expect("http client");

    let packaging_dir = std::env::var("PACKAGING_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/app/packaging"));
    let state: Shared = Arc::new(AppState {
        db: Mutex::new(db),
        web_dir,
        packaging_dir,
        data_dir: data_dir.clone(),
        attempts: Mutex::new(HashMap::new()),
        http,
    });

    {
        let pairs = {
            let db = state.db.lock().unwrap();
            faviconless_auto_links(&db, None)
        };
        warm_favicons(&state, pairs);
    }

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/api/state", get(api_state))
        .route("/api/whoami", get(api_whoami))
        .route("/api/links/move", post(api_links_move))
        .route("/api/links/folder", post(api_links_folder))
        .route("/api/links/folder-rename", post(api_links_folder_rename))
        .route("/api/links/folder-delete", post(api_links_folder_delete))
        .route("/api/sync", get(api_sync))
        .route("/go/:id", get(go))
        .route("/icon/:id", get(icon))
        .route("/login", get(user_login_page).post(user_login))
        .route("/register", get(register_page_h).post(register))
        .route("/forgot", get(forgot_page_h).post(forgot))
        .route("/reset", get(reset_page_h).post(reset_password))
        .route("/api/report", post(report_create))
        .route("/logout", post(logout))
        .route("/account", get(account_page))
        // Admin API (admin session + CSRF on mutations)
        .route("/api/admin/session", get(admin_session))
        .route("/api/admin/users", get(admin_users).post(admin_users_create))
        .route("/api/admin/users/:id/password", post(admin_user_password))
        .route("/api/admin/users/:id/disabled", post(admin_user_disabled))
        .route("/api/admin/users/:id", delete(admin_user_delete))
        .route("/api/admin/links", get(admin_links).post(admin_links_create))
        .route("/api/admin/links/export", get(admin_links_export))
        .route("/api/admin/extension", get(admin_extension))
        .route("/api/admin/links/import", post(admin_links_import))
        .route("/api/admin/links/:id", put(admin_links_update).delete(admin_links_delete))
        .route("/api/admin/sync-tokens", get(admin_tokens).post(admin_tokens_create))
        .route("/api/admin/sync-tokens/:id", delete(admin_tokens_delete))
        .route("/api/admin/password", post(admin_password))
        .route("/api/admin/site", get(admin_site))
        .route("/api/admin/site/registration", post(admin_site_registration))
        .route("/api/admin/smtp", post(admin_smtp_save))
        .route("/api/admin/smtp/test", post(admin_smtp_test))
        .route("/api/admin/reports", get(admin_reports))
        .route("/api/admin/reports/:id/status", post(admin_report_status))
        .route("/api/admin/reports/:id/delete", post(admin_report_delete))
        .route("/api/admin/reports/:id/remove-icon", post(admin_report_remove_icon))
        .route("/s/:token", get(share_landing))
        .route("/s/:token/go/:link_id", get(share_open))
        .route("/api/admin/shares", get(admin_shares_list).post(admin_share_create))
        .route("/api/admin/shares/:id/revoke", post(admin_share_revoke))
        .route("/api/admin/site/profiles", post(admin_site_profiles))
        .route("/api/admin/site/default-owner", post(admin_site_default_owner))
        .route("/u/:username", get(profile_landing))
        .route("/u/:username/go/:link_id", get(profile_open))
        .route("/api/me/profile", get(me_profile_get).post(me_profile_save))
        .route("/api/me/profile/image", post(me_profile_image))
        .route("/api/admin/settings", get(admin_settings).post(admin_settings_update))
        .route("/api/admin/branding", get(admin_branding).post(admin_branding_update))
        .route("/api/admin/icons", get(admin_icons))
        .route("/api/admin/icons/upload", post(admin_icons_upload))
        .route("/api/admin/icons/:file", delete(admin_icons_delete))
        .route("/uploads/:file", get(serve_upload))
        // Account API (any session + CSRF on mutations)
        .route("/api/me/session", get(me_session))
        .route("/api/me/tokens", get(me_tokens).post(me_tokens_create))
        .route("/api/me/tokens/:id", delete(me_tokens_delete))
        .route("/api/me/password", post(me_password))
        .route("/api/me/email", post(me_email))
        .route("/api/me/links", get(me_links))
        .route("/api/me/links/create", post(me_links_create))
        .route("/api/me/links/:id/update", post(me_links_update))
        .route("/api/me/links/:id/delete", post(me_links_delete))
        .route("/api/me/icons", get(me_icons_list).post(me_icons_upload))
        .route("/api/me/shares", get(me_shares_list).post(me_share_create))
        .route("/api/me/shares/:id/revoke", post(me_share_revoke))
        // Obscure admin path (single segment, value lives in settings)
        .route("/:seg", get(admin_entry).post(admin_setup))
        .route("/:seg/login", post(admin_login))
        .route("/:seg/logout", post(logout))
        .fallback(fallback)
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("startpage-server listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .expect("serve");
}
