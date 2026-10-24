// Server-rendered pages for the admin panel, account page, and the two
// login doors. Single self-contained HTML documents (inline CSS/JS, no
// external assets), styled to echo the public page's glass look.

const CSS: &str = r#"
* { box-sizing: border-box; }
body { margin: 0; min-height: 100vh; background: #0b0b0f url('src/background.jpeg') center/cover fixed;
  font-family: system-ui, -apple-system, 'Segoe UI', sans-serif; color: #f2f2f5; }
body::before { content: ''; position: fixed; inset: 0; background: rgba(5,5,10,.55); }
main { position: relative; max-width: 980px; margin: 0 auto; padding: 28px 18px 60px; }
.glass { background: rgba(20,20,28,.66); backdrop-filter: blur(18px); -webkit-backdrop-filter: blur(18px);
  border: 1px solid rgba(255,255,255,.13); border-radius: 16px; box-shadow: 0 12px 40px rgba(0,0,0,.35); }
.card { padding: 20px 22px; margin-bottom: 18px; }
.center { max-width: 420px; margin: 12vh auto 0; }
h1 { font-size: 22px; margin: 0 0 4px; } h2 { font-size: 16px; margin: 0 0 12px; }
.sub { color: rgba(255,255,255,.6); font-size: 13px; margin: 0 0 16px; }
label { display: block; font-size: 12px; text-transform: uppercase; letter-spacing: .08em; color: rgba(255,255,255,.55); margin: 12px 0 5px; }
:root { color-scheme: dark; }
input, select, textarea { width: 100%; padding: 9px 11px; border-radius: 9px; border: 1px solid rgba(255,255,255,.18);
  background: rgba(255,255,255,.07); color: #fff; font-size: 14px; }
select option, select optgroup { background-color: #1c1d26; color: #f2f2f5; }
textarea { min-height: 74px; resize: vertical; }
input:focus, select:focus, textarea:focus { outline: 2px solid rgba(120,170,255,.7); }
button { cursor: pointer; border: 1px solid rgba(255,255,255,.2); background: rgba(255,255,255,.1); color: #fff;
  border-radius: 9px; padding: 8px 14px; font-size: 13.5px; }
button:hover { background: rgba(255,255,255,.18); }
button.primary { background: #2f6fed; border-color: #2f6fed; } button.primary:hover { background: #3b78ff; }
button.danger { background: rgba(200,50,50,.25); border-color: rgba(255,90,90,.4); }
button.small { padding: 4px 9px; font-size: 12px; border-radius: 7px; }
.row { display: flex; gap: 10px; flex-wrap: wrap; } .row > * { flex: 1; min-width: 130px; }
.err { display: none; background: rgba(200,50,50,.22); border: 1px solid rgba(255,90,90,.5); padding: 9px 12px;
  border-radius: 9px; font-size: 13.5px; margin-bottom: 12px; }
.ok { display: none; background: rgba(40,160,80,.2); border: 1px solid rgba(80,220,130,.45); padding: 9px 12px;
  border-radius: 9px; font-size: 13.5px; margin-bottom: 12px; word-break: break-all; }
table { width: 100%; border-collapse: collapse; font-size: 13.5px; }
th, td { text-align: left; padding: 7px 8px; border-bottom: 1px solid rgba(255,255,255,.09); vertical-align: middle; }
th { color: rgba(255,255,255,.55); font-size: 11.5px; text-transform: uppercase; letter-spacing: .07em; }
.badge { font-size: 11px; padding: 2px 7px; border-radius: 20px; background: rgba(255,255,255,.14); }
.badge.admin { background: rgba(237,170,40,.3); } .badge.off { background: rgba(200,50,50,.3); }
.linkrow { display: flex; align-items: center; gap: 10px; padding: 7px 4px; border-bottom: 1px solid rgba(255,255,255,.08); }
.linkrow img { width: 26px; height: 26px; border-radius: 7px; background: rgba(255,255,255,.1); }
.linkrow .meta { color: rgba(255,255,255,.5); font-size: 12px; }
.linkrow .grow { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
h3.sec { font-size: 13px; text-transform: uppercase; letter-spacing: .09em; color: rgba(255,255,255,.6); margin: 16px 0 4px; }
.topbar { display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px; }
code { background: rgba(255,255,255,.12); padding: 1px 6px; border-radius: 6px; font-size: 12.5px; }
.once { font-family: ui-monospace, monospace; font-size: 13px; background: rgba(0,0,0,.35); padding: 10px; border-radius: 9px; word-break: break-all; }
"#;

fn shell(title: &str, inner: &str) -> String {
    format!(
        "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
         <meta name=\"robots\" content=\"noindex, nofollow\"><title>{}</title>\
         <style>{}</style></head><body><main>{}</main></body></html>",
        title, CSS, inner
    )
}

pub fn setup_page() -> String {
    let inner = r#"
<div class="card glass center">
  <h1>Set up the admin account</h1>
  <p class="sub">This startpage has no admin yet. Create the owner account — it gets the default link set, and it is the only account that can open this panel.</p>
  <div class="err" id="err"></div>
  <form id="f">
    <label>Username</label>
    <input id="username" value="admin" autocomplete="username" required>
    <label>Password (min. 10 characters)</label>
    <input id="password" type="password" autocomplete="new-password" required minlength="10">
    <div style="margin-top:18px"><button class="primary" type="submit" style="width:100%">Create admin account</button></div>
  </form>
</div>
<script>
document.getElementById('f').addEventListener('submit', async e => {
  e.preventDefault();
  const err = document.getElementById('err');
  const r = await fetch(location.pathname, { method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username: document.getElementById('username').value, password: document.getElementById('password').value }) });
  if (r.ok) { location.reload(); return; }
  const d = await r.json().catch(() => ({}));
  err.textContent = d.error || ('failed (' + r.status + ')'); err.style.display = 'block';
});
</script>"#;
    shell("Set up — Startpage Admin", inner)
}

pub fn login_page(admin: bool) -> String {
    login_page_ext(admin, false)
}

pub fn login_page_ext(admin: bool, registration_open: bool) -> String {
    let heading = if admin { "Admin sign in" } else { "Sign in" };
    let sub = if admin {
        "This panel is for the owner only."
    } else {
        "Sign in to see your own startpage links."
    };
    let links = if admin {
        String::new()
    } else if registration_open {
        r#"<p class="sub" style="margin-top:16px"><a href="/forgot">Forgot password?</a> · <a href="/register">Create account</a></p>"#.to_string()
    } else {
        r#"<p class="sub" style="margin-top:16px"><a href="/forgot">Forgot password?</a></p>"#.to_string()
    };
    let inner = format!(
        r#"
<div class="card glass center">
  <h1>{}</h1>
  <p class="sub">{}</p>
  <div class="err" id="err"></div>
  <form id="f">
    <label>Username</label>
    <input id="username" autocomplete="username" required>
    <label>Password</label>
    <input id="password" type="password" autocomplete="current-password" required>
    <div style="margin-top:18px"><button class="primary" type="submit" style="width:100%">Sign in</button></div>
  </form>
  {}
</div>
<script>
const ADMIN = {};
document.getElementById('f').addEventListener('submit', async e => {{
  e.preventDefault();
  const err = document.getElementById('err');
  const url = ADMIN ? location.pathname + '/login' : '/login';
  const r = await fetch(url, {{ method: 'POST', headers: {{ 'Content-Type': 'application/json' }},
    body: JSON.stringify({{ username: document.getElementById('username').value, password: document.getElementById('password').value }}) }});
  if (r.ok) {{ location.href = ADMIN ? location.pathname : '/'; return; }}
  const d = await r.json().catch(() => ({{}}));
  err.textContent = d.error || ('failed (' + r.status + ')'); err.style.display = 'block';
}});
</script>"#,
        heading,
        sub,
        links,
        if admin { "true" } else { "false" }
    );
    shell("Sign in — Startpage", &inner)
}

pub fn register_page(open: bool) -> String {
    let inner = if !open {
        r#"
<div class="card glass center">
  <h1>Registration is closed</h1>
  <p class="sub">Accounts on this startpage are created by the admin. If you need one, ask them.</p>
  <p><a href="/login" style="color:#9db9ff">← Back to sign in</a></p>
</div>"#
        .to_string()
    } else {
        r#"
<div class="card glass center">
  <h1>Create account</h1>
  <p class="sub">Your desktop starts with a copy of the public link set.</p>
  <div class="err" id="err"></div>
  <form id="f">
    <label>Username</label>
    <input id="username" autocomplete="username" required>
    <label>Email</label>
    <input id="email" type="email" autocomplete="email" required>
    <label>Password (min. 10 characters)</label>
    <input id="password" type="password" autocomplete="new-password" required minlength="10">
    <div style="margin-top:18px"><button class="primary" type="submit" style="width:100%">Create account</button></div>
  </form>
  <p class="sub" style="margin-top:16px">Already have one? <a href="/login">Sign in</a></p>
</div>
<script>
document.getElementById('f').addEventListener('submit', async e => {
  e.preventDefault();
  const err = document.getElementById('err');
  const r = await fetch('/register', { method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username: document.getElementById('username').value, email: document.getElementById('email').value, password: <redacted> }) });
  if (r.ok) { location.href = '/'; return; }
  const d = await r.json().catch(() => ({}));
  err.textContent = d.error || ('failed (' + r.status + ')'); err.style.display = 'block';
});
</script>"#
            .to_string()
    };
    shell("Create account — Startpage", &inner)
}

pub fn forgot_page(smtp_ready: bool) -> String {
    let inner = if !smtp_ready {
        r#"
<div class="card glass center">
  <h1>Password reset</h1>
  <p class="sub">Email password reset isn't set up on this server yet — ask the admin to reset your password for you.</p>
  <p><a href="/login" style="color:#9db9ff">← Back to sign in</a></p>
</div>"#
        .to_string()
    } else {
        r#"
<div class="card glass center">
  <h1>Forgot password</h1>
  <p class="sub">Enter your username or the email on your account and we'll send a reset link (good for one hour).</p>
  <div class="err" id="err"></div><div class="ok" id="ok" style="display:none"></div>
  <form id="f">
    <label>Username or email</label>
    <input id="identifier" autocomplete="username" required>
    <div style="margin-top:18px"><button class="primary" type="submit" style="width:100%">Send reset link</button></div>
  </form>
  <p class="sub" style="margin-top:16px"><a href="/login">← Back to sign in</a></p>
</div>
<script>
document.getElementById('f').addEventListener('submit', async e => {
  e.preventDefault();
  const err = document.getElementById('err'), ok = document.getElementById('ok');
  const r = await fetch('/forgot', { method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ identifier: document.getElementById('identifier').value }) });
  if (r.ok) { ok.textContent = 'If that account exists, a reset link is on its way — check your inbox.'; ok.style.display = 'block'; document.getElementById('f').style.display = 'none'; return; }
  const d = await r.json().catch(() => ({}));
  err.textContent = d.error || ('failed (' + r.status + ')'); err.style.display = 'block';
});
</script>"#
            .to_string()
    };
    shell("Forgot password — Startpage", &inner)
}

pub fn reset_page(token: &str) -> String {
    let inner = format!(
        r#"
<div class="card glass center">
  <h1>Set a new password</h1>
  <div class="err" id="err"></div><div class="ok" id="ok" style="display:none"></div>
  <form id="f">
    <input type="hidden" id="token" value="{}">
    <label>New password (min. 10 characters)</label>
    <input id="password" type="password" autocomplete="new-password" required minlength="10">
    <label>Repeat it</label>
    <input id="password2" type="password" autocomplete="new-password" required minlength="10">
    <div style="margin-top:18px"><button class="primary" type="submit" style="width:100%">Set password</button></div>
  </form>
</div>
<script>
document.getElementById('f').addEventListener('submit', async e => {{
  e.preventDefault();
  const err = document.getElementById('err'), ok = document.getElementById('ok');
  const p1 = document.getElementById('password').value, p2 = document.getElementById('password2').value;
  if (p1 !== p2) {{ err.textContent = 'The two passwords do not match.'; err.style.display = 'block'; return; }}
  const r = await fetch('/reset', {{ method: 'POST', headers: {{ 'Content-Type': 'application/json' }},
    body: JSON.stringify({{ token: document.getElementById('token').value, password: p1 }}) }});
  if (r.ok) {{ ok.innerHTML = 'Password set — you can <a href="/login" style="color:#9db9ff">sign in</a> now. (You were signed out everywhere else.)'; ok.style.display = 'block'; document.getElementById('f').style.display = 'none'; return; }}
  const d = await r.json().catch(() => ({{}}));
  err.textContent = d.error || ('failed (' + r.status + ')'); err.style.display = 'block';
}});
</script>"#,
        html_escape_simple(token)
    );
    shell("Reset password — Startpage", &inner)
}

pub fn reset_invalid_page() -> String {
    let inner = r#"
<div class="card glass center">
  <h1>That link doesn't work</h1>
  <p class="sub">Reset links expire after an hour and work once. Ask for a fresh one.</p>
  <p><a href="/forgot" style="color:#9db9ff">Get a new reset link</a> · <a href="/login" style="color:#9db9ff">Sign in</a></p>
</div>"#;
    shell("Reset password — Startpage", inner)
}

fn html_escape_simple(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn forbidden_page() -> String {
    let inner = r#"
<div class="card glass center">
  <h1>Admins only</h1>
  <p class="sub">You are signed in, but this panel belongs to the admin account.</p>
  <p><a href="/" style="color:#9db9ff">← Back to the startpage</a></p>
</div>"#;
    shell("Admins only", inner)
}

/// Public landing for a share token (/s/<token>): one app, or a folder
/// as a grid of its members. Self-contained styling (glass card on a
/// dark gradient) so it looks like the product without the app shell.
pub fn share_page(
    product: &str,
    item_id: &str,
    title: &str,
    is_folder: bool,
    owner: &str,
    token: &str,
    tiles: &[(String, String, String, Option<String>)],
    locked: bool,
) -> String {
    let e = html_escape_simple;
    let tiles_html: String = tiles
        .iter()
        .map(|(id, label, icon, bg)| {
            let plate = bg
                .as_deref()
                .map(|c| format!(" style=\"background:{}\"", e(c)))
                .unwrap_or_default();
            format!(
                "<a class=\"tile\" href=\"/s/{}/go/{}\" target=\"_blank\" rel=\"noopener noreferrer\" data-id=\"{}\" data-label=\"{}\"><span class=\"plate\"{}><img src=\"{}\" alt=\"\" loading=\"lazy\"></span><span class=\"tlabel\">{}</span></a>",
                e(token),
                e(id),
                e(id),
                e(label),
                plate,
                e(icon),
                e(label)
            )
        })
        .collect();
    let sub = if locked {
        format!("🔒 This folder is password-protected — unlock it to see what's inside. Shared by {}.", e(owner))
    } else if is_folder {
        format!("A shared folder from {} — tap any app to open it.", e(owner))
    } else {
        format!("Shared by {}", e(owner))
    };
    let tiles_html = if locked {
        let target_js = serde_json::to_string(&format!("folder:{item_id}")).unwrap_or_else(|_| "\"\"".to_string());
        format!(
            "<div style=\"text-align:center;width:100%\">\
             <p style=\"font-size:2em;margin:6px 0\">🔒</p>\
             <p><input id=\"unlockPw\" type=\"password\" placeholder=\"Folder password\" autocomplete=\"current-password\" \
             style=\"width:min(260px,80%);padding:.65em;border-radius:10px;border:1px solid rgba(255,255,255,.25);background:rgba(255,255,255,.06);color:#fff;font-size:1em\"></p>\
             <p id=\"unlockErr\" style=\"color:#ff9d9d;min-height:1.2em\"></p>\
             <p><button id=\"unlockBtn\" style=\"background:#2f6bff;color:#fff;padding:.65em 1.4em;border:0;border-radius:12px;font-weight:600;cursor:pointer\">Unlock</button></p></div>\
             <script>var UTARGET = {target_js};\
             async function doUnlock() {{ var r = await fetch('/api/unlock', {{ method: 'POST', headers: {{ 'Content-Type': 'application/json' }}, body: JSON.stringify({{ target: UTARGET, password: document.getElementById('unlockPw').value }}) }}); if (r.ok) {{ location.reload(); }} else {{ document.getElementById('unlockErr').textContent = 'Wrong password.'; }} }}\
             document.getElementById('unlockBtn').addEventListener('click', doUnlock);\
             document.getElementById('unlockPw').addEventListener('keydown', function (ev) {{ if (ev.key === 'Enter') doUnlock(); }});</script>"
        )
    } else {
        tiles_html
    };
    let mode = if is_folder { "grid" } else { "grid single" };
    r#"<!doctype html><html><head><meta charset="utf-8">
<title>__TITLE__ — __PRODUCT__</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="referrer" content="no-referrer">
<style>
* { box-sizing: border-box; }
body { margin: 0; min-height: 100vh; font-family: system-ui, -apple-system, sans-serif; color: #f2f2f5;
  background: radial-gradient(1100px 600px at 20% -10%, #26355e 0%, transparent 60%), radial-gradient(900px 500px at 110% 110%, #3d2a55 0%, transparent 55%), #0b0d14;
  display: flex; align-items: flex-start; justify-content: center; padding: 48px 18px 40px; }
.card { width: 100%; max-width: 560px; background: rgba(18,19,28,.72); border: 1px solid rgba(255,255,255,.12);
  border-radius: 22px; padding: 28px 28px 20px; backdrop-filter: blur(18px); box-shadow: 0 18px 50px rgba(0,0,0,.45); }
.prod { font-size: 12px; letter-spacing: .22em; text-transform: uppercase; color: rgba(235,235,245,.5); margin: 0 0 6px; }
h1 { margin: 0 0 6px; font-size: 26px; font-weight: 700; overflow-wrap: anywhere; }
.sub { margin: 0 0 22px; font-size: 14px; color: rgba(235,235,245,.65); }
.grid { display: grid; grid-template-columns: repeat(auto-fill, 74px); gap: 12px 6px; justify-content: center; }
.tile { display: flex; flex-direction: column; align-items: center; gap: 5px; text-decoration: none; color: #f2f2f5; width: 74px; }
.plate { width: 54px; height: 54px; border-radius: 14px; background: rgba(255,255,255,.10); display: flex; align-items: center; justify-content: center; overflow: hidden; box-shadow: 0 3px 10px rgba(0,0,0,.35); }
.plate img { width: 100%; height: 100%; object-fit: cover; }
.tile:hover .plate { background: rgba(255,255,255,.18); }
.tlabel { font-size: 11px; text-align: center; line-height: 1.2; max-width: 74px; overflow-wrap: anywhere; color: rgba(235,235,245,.88); }
.single .plate { width: 96px; height: 96px; border-radius: 24px; }
.single { display: flex; justify-content: center; }
.foot { margin: 22px 0 0; text-align: center; font-size: 12px; color: rgba(235,235,245,.4); }
.foot a { color: #9db9ff; text-decoration: none; }
.repbtn { background: none; border: 0; padding: 0; font: inherit; color: #9db9ff; cursor: pointer; }
.repbtn:hover { text-decoration: underline; }
.rep { position: fixed; inset: 0; background: rgba(4,5,10,.6); display: flex; align-items: center; justify-content: center; padding: 18px; z-index: 50; }
.rep[hidden] { display: none; }
.repcard { width: 100%; max-width: 420px; background: rgba(22,23,34,.96); border: 1px solid rgba(255,255,255,.14); border-radius: 18px; padding: 22px; box-shadow: 0 18px 50px rgba(0,0,0,.5); }
.repcard h3 { margin: 0 0 6px; font-size: 17px; }
.repsub { margin: 0 0 12px; font-size: 13px; color: rgba(235,235,245,.65); line-height: 1.45; }
.repcard textarea { width: 100%; min-height: 84px; resize: vertical; border-radius: 10px; border: 1px solid rgba(255,255,255,.16);
  background: rgba(255,255,255,.06); color: #f2f2f5; padding: 10px 12px; font: inherit; font-size: 14px; }
.repmsg { min-height: 18px; font-size: 13px; color: #ffb02e; margin: 10px 0 0; }
.repactions { display: flex; justify-content: flex-end; gap: 10px; margin-top: 14px; }
.repactions button { cursor: pointer; border: 1px solid rgba(255,255,255,.18); background: rgba(255,255,255,.07); color: #f2f2f5;
  border-radius: 10px; padding: 9px 16px; font-size: 14px; }
.repactions button.primary { background: #2f6bff; border-color: #2f6bff; font-weight: 600; }
</style></head><body>
<div class="card">
<p class="prod">__PRODUCT__</p>
<h1>__TITLE__</h1>
<p class="sub">__SUB__</p>
<div class="__MODE__">__TILES__</div>
<p class="foot">Shared from <a href="/">__PRODUCT__</a> · <button class="repbtn" id="repOpen" data-id="__ITEM_ID__" data-label="__TITLE__">⚑ Report</button></p>
</div>
<div class="rep" id="repPop" hidden><div class="repcard">
<h3>⚑ Report</h3>
<p class="repsub" id="repSub"></p>
<textarea id="repReason" maxlength="500" placeholder="What is wrong with it? (optional) — broken link, wrong site, outdated…"></textarea>
<p class="repmsg" id="repMsg"></p>
<div class="repactions"><button id="repCancel" type="button">Cancel</button><button id="repSend" type="button" class="primary">Send report</button></div>
</div></div>
<script>
var SHARE_TOKEN = '__TOKEN__';
var repId = null;
function openRep(id, label) {
  repId = id;
  document.getElementById('repSub').textContent = '\u201C' + label + '\u201D — the admin sees your report in the panel.';
  document.getElementById('repReason').value = '';
  document.getElementById('repMsg').textContent = '';
  document.getElementById('repPop').hidden = false;
}
function closeRep() { document.getElementById('repPop').hidden = true; }
document.getElementById('repOpen').addEventListener('click', function () { openRep(this.dataset.id, this.dataset.label); });
document.querySelectorAll('.tile').forEach(function (t) {
  t.addEventListener('contextmenu', function (e) { e.preventDefault(); openRep(t.dataset.id, t.dataset.label); });
});
document.getElementById('repCancel').addEventListener('click', closeRep);
document.getElementById('repPop').addEventListener('pointerdown', function (e) { if (e.target === this) closeRep(); });
document.getElementById('repSend').addEventListener('click', async function () {
  var btn = this; btn.disabled = true;
  try {
    var r = await fetch('/api/report', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ link_id: repId, reason: document.getElementById('repReason').value, share_token: SHARE_TOKEN }) });
    if (!r.ok) { var d = await r.json().catch(function () { return {}; }); throw new Error(d.error || ('failed (' + r.status + ')')); }
    document.getElementById('repMsg').textContent = '\u2713 Sent — thanks. The admin will take a look.';
    setTimeout(closeRep, 1100);
  } catch (err) { document.getElementById('repMsg').textContent = 'Could not send: ' + err.message; }
  btn.disabled = false;
});
</script>
</body></html>"#
        .replace("__PRODUCT__", &e(product))
        .replace("__TITLE__", &e(title))
        .replace("__SUB__", &sub)
        .replace("__MODE__", mode)
        .replace("__ITEM_ID__", &e(item_id))
        .replace("__TOKEN__", &e(token))
        .replace("__TILES__", &tiles_html)
}

pub fn share_gone_page(product: &str) -> String {
    let e = html_escape_simple;
    r#"<!doctype html><html><head><meta charset="utf-8">
<title>Share unavailable — __PRODUCT__</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="referrer" content="no-referrer">
<style>
body { margin: 0; min-height: 100vh; font-family: system-ui, -apple-system, sans-serif; color: #f2f2f5; background: #0b0d14;
  display: flex; align-items: center; justify-content: center; padding: 24px 18px; }
.card { width: 100%; max-width: 440px; background: rgba(18,19,28,.72); border: 1px solid rgba(255,255,255,.12);
  border-radius: 22px; padding: 28px; backdrop-filter: blur(18px); text-align: center; }
h1 { margin: 0 0 8px; font-size: 22px; }
p { color: rgba(235,235,245,.65); font-size: 14px; line-height: 1.55; }
a { color: #9db9ff; text-decoration: none; }
</style></head><body>
<div class="card">
<h1>This share link is no longer available</h1>
<p>It may have expired, or its owner revoked it. Nothing else is exposed by this address.</p>
<p><a href="/">Go to __PRODUCT__</a></p>
</div>
</body></html>"#
        .replace("__PRODUCT__", &e(product))
}

/// Public profile page (/u/<username>): hero (header image or accent
/// gradient), avatar, name, bio — then the user's startpage: loose
/// apps, then each folder as a headed grid. Tiles report via
/// right-click/hold; the footer button reports the profile itself.
pub fn profile_page(
    product: &str,
    username: &str,
    display_name: &str,
    bio: &str,
    avatar_path: Option<&str>,
    header_path: Option<&str>,
    accent: Option<&str>,
    loose: &[(String, String, String, Option<String>)],
    folders: &[(String, String, Vec<(String, String, String, Option<String>)>)],
) -> String {
    let e = html_escape_simple;
    let name = if display_name.trim().is_empty() { username } else { display_name };
    let accent_css = accent.unwrap_or("#2f6bff");
    let hero = match header_path {
        Some(h) => format!("<div class=\"hero\"><img src=\"{}\" alt=\"\"></div>", e(h)),
        None => format!("<div class=\"hero hero-grad\" style=\"background:linear-gradient(120deg, {} 0%, #14161f 78%)\"></div>", e(accent_css)),
    };
    let initial = name.chars().next().unwrap_or('?').to_uppercase().to_string();
    let avatar = match avatar_path {
        Some(a) => format!("<div class=\"avatar\"><img src=\"{}\" alt=\"\"></div>", e(a)),
        None => format!("<div class=\"avatar avatar-init\" style=\"background:{}\">{}</div>", e(accent_css), e(&initial)),
    };
    let bio_html = if bio.trim().is_empty() {
        String::new()
    } else {
        format!("<p class=\"bio\">{}</p>", e(bio))
    };
    let tile = |id: &str, label: &str, icon: &str, bg: &Option<String>| {
        let plate = bg
            .as_deref()
            .map(|c| format!(" style=\"background:{}\"", e(c)))
            .unwrap_or_default();
        format!(
            "<a class=\"tile\" href=\"/u/{}/go/{}\" target=\"_blank\" rel=\"noopener noreferrer\" data-id=\"{}\" data-label=\"{}\"><span class=\"plate\"{}><img src=\"{}\" alt=\"\" loading=\"lazy\"></span><span class=\"tlabel\">{}</span></a>",
            e(username),
            e(id),
            e(id),
            e(label),
            plate,
            e(icon),
            e(label)
        )
    };
    let mut sections = String::new();
    if !loose.is_empty() {
        let tiles: String = loose.iter().map(|(id, label, icon, bg)| tile(id, label, icon, bg)).collect();
        sections.push_str(&format!("<div class=\"grid loose\">{tiles}</div>"));
    }
    if !folders.is_empty() {
        // Folders appear as folder tiles (a mosaic of the first four
        // member icons, like the startpage itself); tapping one expands
        // its members in a panel under the grid. One open at a time.
        let mut ftiles = String::new();
        let mut panels = String::new();
        for (i, (fid, fname, members)) in folders.iter().enumerate() {
            if members.is_empty() {
                continue;
            }
            let mosaic: String = members
                .iter()
                .take(4)
                .map(|m| format!("<img src=\"{}\" alt=\"\" loading=\"lazy\">", e(&m.2)))
                .collect();
            let mclass = if members.len() == 1 { "mosaic one" } else { "mosaic" };
            ftiles.push_str(&format!(
                "<button type=\"button\" class=\"tile ftile\" data-folder=\"{i}\" data-id=\"{}\" data-label=\"{}\"><span class=\"plate\"><span class=\"{mclass}\">{mosaic}</span></span><span class=\"tlabel\">{}</span><span class=\"tcount\">{} apps</span></button>",
                e(fid),
                e(fname),
                e(fname),
                members.len()
            ));
            let mtiles: String = members.iter().map(|(id, label, icon, bg)| tile(id, label, icon, bg)).collect();
            panels.push_str(&format!(
                "<div class=\"fpanel\" id=\"fp-{i}\" hidden><h3 class=\"fpname\">{}</h3><div class=\"grid\">{mtiles}</div></div>",
                e(fname)
            ));
        }
        if !ftiles.is_empty() {
            sections.push_str(&format!("<div class=\"grid fgrid\">{ftiles}</div>{panels}"));
        }
    }
    if sections.is_empty() {
        sections = "<p class=\"empty\">Nothing here yet.</p>".to_string();
    }
    r#"<!doctype html><html><head><meta charset="utf-8">
<title>__NAME__ (@__USER__) — __PRODUCT__</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="referrer" content="no-referrer">
<style>
* { box-sizing: border-box; }
body { margin: 0; min-height: 100vh; font-family: system-ui, -apple-system, sans-serif; color: #f2f2f5;
  background: radial-gradient(1100px 600px at 20% -10%, #26355e 0%, transparent 60%), radial-gradient(900px 500px at 110% 110%, #3d2a55 0%, transparent 55%), #0b0d14;
  display: flex; align-items: flex-start; justify-content: center; padding: 40px 18px; }
.card { width: 100%; max-width: 720px; background: rgba(18,19,28,.72); border: 1px solid rgba(255,255,255,.12);
  border-radius: 22px; padding: 0 28px 20px; backdrop-filter: blur(18px); box-shadow: 0 18px 50px rgba(0,0,0,.45); overflow: hidden; }
.hero { margin: 0 -28px; height: 168px; overflow: hidden; background: #14161f; }
.hero img { width: 100%; height: 100%; object-fit: cover; display: block; }
.avatar { width: 92px; height: 92px; border-radius: 50%; overflow: hidden; margin: -46px auto 0; border: 4px solid rgba(18,19,28,.9);
  background: rgba(255,255,255,.10); display: flex; align-items: center; justify-content: center; position: relative; }
.avatar img { width: 100%; height: 100%; object-fit: cover; }
.avatar-init { font-size: 38px; font-weight: 700; color: #fff; }
h1 { margin: 12px 0 2px; font-size: 26px; font-weight: 700; text-align: center; overflow-wrap: anywhere; }
.handle { margin: 0 0 10px; text-align: center; font-size: 14px; color: rgba(235,235,245,.55); }
.bio { margin: 0 auto 8px; max-width: 46ch; text-align: center; font-size: 14.5px; line-height: 1.55; color: rgba(235,235,245,.8); white-space: pre-line; overflow-wrap: anywhere; }
.grid { display: grid; grid-template-columns: repeat(auto-fill, 74px); gap: 12px 6px; justify-content: center; margin-top: 12px; }
.tile { display: flex; flex-direction: column; align-items: center; gap: 5px; text-decoration: none; color: #f2f2f5; width: 74px; }
.plate { width: 54px; height: 54px; border-radius: 14px; background: rgba(255,255,255,.10); display: flex; align-items: center; justify-content: center; overflow: hidden; box-shadow: 0 3px 10px rgba(0,0,0,.35); }
.plate img { width: 100%; height: 100%; object-fit: cover; }
.tile:hover .plate { background: rgba(255,255,255,.18); }
.tlabel { font-size: 11px; text-align: center; line-height: 1.2; max-width: 74px; overflow-wrap: anywhere; color: rgba(235,235,245,.88); }
.tcount { font-size: 10px; color: rgba(235,235,245,.45); margin-top: -3px; }
.ftile { background: none; border: 0; cursor: pointer; font: inherit; padding: 0; }
.ftile:hover .plate { background: rgba(255,255,255,.18); }
.ftile.open .plate { outline: 2px solid rgba(157,185,255,.75); outline-offset: 1px; }
.mosaic { display: grid; grid-template-columns: 1fr 1fr; grid-template-rows: 1fr 1fr; gap: 3px; width: 100%; height: 100%; padding: 5px; }
.mosaic img { width: 100%; height: 100%; object-fit: cover; border-radius: 5px; }
.mosaic.one { display: block; padding: 0; }
.mosaic.one img { border-radius: 0; }
.fpanel { margin-top: 14px; padding: 12px 8px 4px; border: 1px solid rgba(255,255,255,.10); border-radius: 16px; background: rgba(255,255,255,.04); }
.fpanel .grid { margin-top: 0; }
.fpname { margin: 0 0 10px; font-size: 11.5px; letter-spacing: .14em; text-transform: uppercase; color: rgba(235,235,245,.55); text-align: center; }
.empty { text-align: center; color: rgba(235,235,245,.5); font-size: 14px; margin: 26px 0 6px; }
.foot { margin: 26px 0 0; text-align: center; font-size: 12px; color: rgba(235,235,245,.4); }
.foot a { color: #9db9ff; text-decoration: none; }
.repbtn { background: none; border: 0; padding: 0; font: inherit; color: #9db9ff; cursor: pointer; }
.repbtn:hover { text-decoration: underline; }
.rep { position: fixed; inset: 0; background: rgba(4,5,10,.6); display: flex; align-items: center; justify-content: center; padding: 18px; z-index: 50; }
.rep[hidden] { display: none; }
.repcard { width: 100%; max-width: 420px; background: rgba(22,23,34,.96); border: 1px solid rgba(255,255,255,.14); border-radius: 18px; padding: 22px; box-shadow: 0 18px 50px rgba(0,0,0,.5); }
.repcard h3 { margin: 0 0 6px; font-size: 17px; }
.repsub { margin: 0 0 12px; font-size: 13px; color: rgba(235,235,245,.65); line-height: 1.45; }
.repcard textarea { width: 100%; min-height: 84px; resize: vertical; border-radius: 10px; border: 1px solid rgba(255,255,255,.16);
  background: rgba(255,255,255,.06); color: #f2f2f5; padding: 10px 12px; font: inherit; font-size: 14px; }
.repmsg { min-height: 18px; font-size: 13px; color: #ffb02e; margin: 10px 0 0; }
.repactions { display: flex; justify-content: flex-end; gap: 10px; margin-top: 14px; }
.repactions button { cursor: pointer; border: 1px solid rgba(255,255,255,.18); background: rgba(255,255,255,.07); color: #f2f2f5;
  border-radius: 10px; padding: 9px 16px; font-size: 14px; }
.repactions button.primary { background: #2f6bff; border-color: #2f6bff; font-weight: 600; }
</style></head><body>
<div class="card">
__HERO__
__AVATAR__
<h1>__NAME__</h1>
<p class="handle">@__USER__</p>
__BIO__
__SECTIONS__
<p class="foot"><a href="/">__PRODUCT__</a> · <button class="repbtn" id="repOpen">⚑ Report this profile</button></p>
</div>
<div class="rep" id="repPop" hidden><div class="repcard">
<h3>⚑ Report</h3>
<p class="repsub" id="repSub"></p>
<textarea id="repReason" maxlength="500" placeholder="What is wrong with it? (optional)"></textarea>
<p class="repmsg" id="repMsg"></p>
<div class="repactions"><button id="repCancel" type="button">Cancel</button><button id="repSend" type="button" class="primary">Send report</button></div>
</div></div>
<script>
var repBody = null;
function openRep(body, label) {
  repBody = body;
  document.getElementById('repSub').textContent = '\u201C' + label + '\u201D — the admin sees your report in the panel.';
  document.getElementById('repReason').value = '';
  document.getElementById('repMsg').textContent = '';
  document.getElementById('repPop').hidden = false;
}
function closeRep() { document.getElementById('repPop').hidden = true; }
document.getElementById('repOpen').addEventListener('click', function () {
  openRep({ username: '__USER_JSON__' }, '@__USER__ (profile)');
});
document.querySelectorAll('.tile').forEach(function (t) {
  if (!t.dataset.id) return;
  t.addEventListener('contextmenu', function (e) { e.preventDefault(); openRep({ link_id: t.dataset.id, via_profile: '__USER_JSON__' }, t.dataset.label); });
});
document.querySelectorAll('.ftile').forEach(function (b) {
  b.addEventListener('click', function () {
    var panel = document.getElementById('fp-' + b.dataset.folder);
    var wasHidden = panel.hidden;
    document.querySelectorAll('.fpanel').forEach(function (p) { p.hidden = true; });
    document.querySelectorAll('.ftile').forEach(function (x) { x.classList.remove('open'); });
    if (wasHidden) {
      panel.hidden = false;
      b.classList.add('open');
      panel.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    }
  });
});
document.getElementById('repCancel').addEventListener('click', closeRep);
document.getElementById('repPop').addEventListener('pointerdown', function (e) { if (e.target === this) closeRep(); });
document.getElementById('repSend').addEventListener('click', async function () {
  var btn = this; btn.disabled = true;
  try {
    var payload = Object.assign({}, repBody, { reason: document.getElementById('repReason').value });
    var r = await fetch('/api/report', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload) });
    if (!r.ok) { var d = await r.json().catch(function () { return {}; }); throw new Error(d.error || ('failed (' + r.status + ')')); }
    document.getElementById('repMsg').textContent = '\u2713 Sent — thanks. The admin will take a look.';
    setTimeout(closeRep, 1100);
  } catch (err) { document.getElementById('repMsg').textContent = 'Could not send: ' + err.message; }
  btn.disabled = false;
});
</script>
</body></html>"#
        .replace("__PRODUCT__", &e(product))
        .replace("__NAME__", &e(name))
        .replace("__USER_JSON__", &e(username))
        .replace("__USER__", &e(username))
        .replace("__HERO__", &hero)
        .replace("__AVATAR__", &avatar)
        .replace("__BIO__", &bio_html)
        .replace("__SECTIONS__", &sections)
}

pub fn profile_gone_page(product: &str) -> String {
    let e = html_escape_simple;
    r#"<!doctype html><html><head><meta charset="utf-8">
<title>Profile unavailable — __PRODUCT__</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="referrer" content="no-referrer">
<style>
body { margin: 0; min-height: 100vh; font-family: system-ui, -apple-system, sans-serif; color: #f2f2f5; background: #0b0d14;
  display: flex; align-items: center; justify-content: center; padding: 24px 18px; }
.card { width: 100%; max-width: 440px; background: rgba(18,19,28,.72); border: 1px solid rgba(255,255,255,.12);
  border-radius: 22px; padding: 28px; backdrop-filter: blur(18px); text-align: center; }
h1 { margin: 0 0 8px; font-size: 22px; }
p { color: rgba(235,235,245,.65); font-size: 14px; line-height: 1.55; }
a { color: #9db9ff; text-decoration: none; }
</style></head><body>
<div class="card">
<h1>This profile isn't available</h1>
<p>Profiles may be turned off, or this user hasn't made theirs public.</p>
<p><a href="/">Go to __PRODUCT__</a></p>
</div>
</body></html>"#
        .replace("__PRODUCT__", &e(product))
}

pub fn account_page() -> String {
    let inner = r#"
<div class="topbar"><div><h1>My account</h1><p class="sub" style="margin:0">Signed in as <b id="who"></b> · <a href="/" style="color:#9db9ff">startpage</a></p></div>
<button id="logout">Sign out</button></div>

<div class="card glass">
  <h2>Browser extension</h2>
  <p class="sub">Build a ready-to-load new-tab extension from this server's own page files, with your Branding (product name + tagline) baked into it. Unzip the download, then in Brave/Chrome open <b>brave://extensions</b>, enable Developer mode, choose <b>Load unpacked</b> and select the unzipped folder. In Firefox use <b>about:debugging → Load Temporary Add-on</b> and pick the manifest inside it. Finally open the extension's Options page and pair it with your server address and a sync token from the card below — it then mirrors your links and keeps a cached copy for offline use. (Publishing to the web stores instead? The listing copy lives in the project at <b>packaging/store-listing.md</b>.)</p>
  <div class="row">
    <div style="flex:0"><button class="primary" type="button" onclick="location.href='/api/admin/extension?flavor=chrome'">Download for Brave / Chrome (.zip)</button></div>
    <div style="flex:0"><button type="button" onclick="location.href='/api/admin/extension?flavor=firefox'">Download for Firefox (.zip)</button></div>
  </div>
</div>

<div class="card glass">
  <h2>Extension sync tokens</h2>
  <p class="sub">Pair the browser extension per device. A token lets the extension cache YOUR links for offline new tabs. Each token is shown once — revoke it here if a device is lost.</p>
  <div class="err" id="tokErr"></div><div class="ok" id="tokOnce"></div>
  <form id="tokForm" class="row" style="align-items:flex-end">
    <div><label>Device name</label><input id="tokName" placeholder="Brave — Dell" required></div>
    <div style="flex:0"><button class="primary" type="submit">Create token</button></div>
  </form>
  <table><thead><tr><th>Device</th><th>Created</th><th>Last used</th><th></th></tr></thead><tbody id="tokBody"></tbody></table>
</div>

<div class="card glass">
  <h2>Public profile</h2>
  <p class="sub">Your profile page puts your bio and your startpage apps under one link. <span id="pfGate"></span></p>
  <div class="err" id="pfErr"></div><div class="ok" id="pfOk"></div>
  <form id="pfForm">
    <label>Display name</label><input id="pfName" maxlength="80" placeholder="Your name">
    <label>Bio</label><textarea id="pfBio" maxlength="500" rows="3" placeholder="A line or two about you"></textarea>
    <label>Accent color</label><input id="pfAccent" type="color" style="max-width:120px;padding:4px;height:38px">
    <label style="display:flex;gap:8px;align-items:center;font-weight:400;margin-top:10px"><input type="checkbox" id="pfPublic" style="width:auto"> Make my profile public</label>
    <div class="row" style="align-items:flex-end;margin-top:6px">
      <div><label>Your profile link</label><input id="pfUrl" readonly></div>
      <div style="flex:0"><button id="pfCopy" type="button">Copy</button></div>
      <div style="flex:0"><button class="primary" type="submit">Save profile</button></div>
    </div>
  </form>
  <div class="row" style="margin-top:12px">
    <div><label>Avatar image</label><input id="pfAvatarFile" type="file" accept="image/*"></div>
    <div><label>Header image</label><input id="pfHeaderFile" type="file" accept="image/*"></div>
  </div>
  <p class="sub">Images up to 2 MB (png, jpg, webp, gif, svg). Picking a file uploads it right away; it appears on your profile.</p>
</div>

<div class="card glass">
  <h2>Share links</h2>
  <p class="sub">Share one of YOUR apps or folders with a public link — it opens a read-only landing page at <code>/s/…</code>. Anyone holding the link can open the shared items until you revoke it or it expires, and visitors can report a shared item from its page. Creating a new link for an item replaces its old one.</p>
  <div class="err" id="shErr"></div><div class="ok" id="shOk"></div>
  <div class="row" style="align-items:flex-end">
    <div><label>App or folder</label><select id="shItem"></select></div>
    <div><label>Expires</label><select id="shExpiry"><option value="">Never</option><option value="7">In 7 days</option><option value="30">In 30 days</option></select></div>
    <div style="flex:0"><button class="primary" id="shCreate" type="button">Create share link</button></div>
  </div>
  <div class="row" id="shNewRow" style="display:none;align-items:flex-end">
    <div><label>Your share link</label><input id="shNewUrl" readonly></div>
    <div style="flex:0"><button id="shCopy" type="button">Copy</button></div>
  </div>
  <table><thead><tr><th>Item</th><th>Share link</th><th>Expires</th><th>Views</th><th>Created</th><th></th></tr></thead><tbody id="shBody"></tbody></table>
</div>

<div class="card glass">
  <h2>Email</h2>
  <p class="sub">Used for password resets — and if you're an admin, you also get an email whenever a visitor reports a link, app or folder.</p>
  <div class="err" id="emErr"></div><div class="ok" id="emOk"></div>
  <form id="emForm" class="row" style="align-items:flex-end">
    <div><label>Email address</label><input id="emAddr" type="email" placeholder="you@example.com"></div>
    <div style="flex:0"><button class="primary" type="submit">Save email</button></div>
  </form>
</div>

<div class="card glass">
  <h2>Change my password</h2>
  <div class="err" id="pwErr"></div><div class="ok" id="pwOk"></div>
  <form id="pwForm">
    <label>Current password</label><input id="pwCur" type="password" autocomplete="current-password" required>
    <label>New password (min. 10 characters)</label><input id="pwNew" type="password" autocomplete="new-password" required minlength="10">
    <div style="margin-top:14px"><button class="primary" type="submit">Change password</button></div>
  </form>
</div>
<script>
let CSRF = '';
const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const fmt = ts => ts ? new Date(ts * 1000).toLocaleString() : 'never';
async function api(method, url, body) {
  const h = { 'Content-Type': 'application/json' };
  if (method !== 'GET') h['X-CSRF-Token'] = CSRF;
  const r = await fetch(url, { method, headers: h, body: body ? JSON.stringify(body) : undefined });
  const d = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(d.error || ('HTTP ' + r.status));
  return d;
}
function show(id, msg) { const e = document.getElementById(id); e.textContent = msg; e.style.display = 'block'; }
async function loadTokens() {
  const d = await api('GET', '/api/me/tokens');
  document.getElementById('tokBody').innerHTML = d.tokens.map(t =>
    '<tr><td>' + esc(t.name) + '</td><td>' + fmt(t.created_at) + '</td><td>' + fmt(t.last_used_at) +
    '</td><td><button class="small danger" data-revoke="' + t.id + '">Revoke</button></td></tr>').join('')
    || '<tr><td colspan="4" style="color:rgba(255,255,255,.5)">No tokens yet</td></tr>';
}
document.getElementById('tokBody').addEventListener('click', async e => {
  const b = e.target.closest('[data-revoke]'); if (!b) return;
  if (!confirm('Revoke this token? The paired extension will stop syncing.')) return;
  await api('DELETE', '/api/me/tokens/' + b.dataset.revoke); loadTokens();
});
document.getElementById('tokForm').addEventListener('submit', async e => {
  e.preventDefault();
  try {
    const d = await api('POST', '/api/me/tokens', { name: document.getElementById('tokName').value });
    show('tokOnce', 'Token for “' + d.name + '” — copy it now, it will not be shown again:');
    const box = document.createElement('div'); box.className = 'once'; box.textContent = d.token;
    document.getElementById('tokOnce').appendChild(box);
    document.getElementById('tokName').value = ''; loadTokens();
  } catch (err) { show('tokErr', err.message); }
});
async function loadProfile() {
  const d = await api('GET', '/api/me/profile');
  document.getElementById('pfName').value = d.display_name || '';
  document.getElementById('pfBio').value = d.bio || '';
  document.getElementById('pfAccent').value = d.accent_color || '#2f6bff';
  document.getElementById('pfPublic').checked = !!d.profile_public;
  document.getElementById('pfUrl').value = location.origin + d.path;
  document.getElementById('pfGate').textContent = d.profiles_enabled
    ? 'The admin has enabled public profiles.'
    : 'The admin has not enabled public profiles yet — set everything up now; the page goes live when they switch it on.';
}
document.getElementById('pfForm').addEventListener('submit', async e => {
  e.preventDefault();
  try {
    await api('POST', '/api/me/profile', {
      display_name: document.getElementById('pfName').value,
      bio: document.getElementById('pfBio').value,
      accent_color: document.getElementById('pfAccent').value,
      profile_public: document.getElementById('pfPublic').checked,
    });
    show('pfOk', 'Profile saved.');
  } catch (err) { show('pfErr', err.message); }
});
document.getElementById('pfCopy').addEventListener('click', async () => {
  const v = document.getElementById('pfUrl').value;
  try { await navigator.clipboard.writeText(v); } catch (_) { document.getElementById('pfUrl').select(); document.execCommand('copy'); }
  show('pfOk', 'Profile link copied.');
});
function wireProfileImage(inputId, kind) {
  document.getElementById(inputId).addEventListener('change', async e => {
    const f = e.target.files[0]; e.target.value = '';
    if (!f) return;
    try {
      const dataUrl = await new Promise((res, rej) => { const r = new FileReader(); r.onload = () => res(r.result); r.onerror = rej; r.readAsDataURL(f); });
      const d = await api('POST', '/api/me/profile/image', { kind, filename: f.name, data_base64: dataUrl });
      show('pfOk', (kind === 'avatar' ? 'Avatar' : 'Header') + ' uploaded: ' + d.path);
    } catch (err) { show('pfErr', err.message); }
  });
}
wireProfileImage('pfAvatarFile', 'avatar');
wireProfileImage('pfHeaderFile', 'header');
async function loadMyShares() {
  const mine = await api('GET', '/api/me/links');
  const sel = document.getElementById('shItem'); const cur = sel.value;
  sel.innerHTML = mine.links.filter(l => l.kind === 'url' || l.kind === 'folder').map(l => '<option value="' + l.id + '">' + esc(l.label) + (l.kind === 'folder' ? ' (folder)' : '') + '</option>').join('') || '<option value="">(no apps yet)</option>';
  if (cur) sel.value = cur;
  const d = await api('GET', '/api/me/shares');
  document.getElementById('shBody').innerHTML = d.shares.map(s =>
    '<tr><td><b>' + esc(s.label) + '</b>' + (s.kind === 'folder' ? ' <span style="opacity:.6">(folder)</span>' : '') + '</td>' +
    '<td><code>/s/' + esc(s.token) + '</code></td><td>' + (s.expires_at ? fmt(s.expires_at) : 'never') + '</td><td>' + s.views + '</td><td>' + fmt(s.created_at) + '</td>' +
    '<td><button class="small danger" data-shrevoke="' + s.id + '">Revoke</button></td></tr>').join('') ||
    '<tr><td colspan="6" style="opacity:.55">No share links yet.</td></tr>';
}
document.getElementById('shCreate').addEventListener('click', async () => {
  const linkId = document.getElementById('shItem').value;
  if (!linkId) { show('shErr', 'Pick an app or folder first.'); return; }
  try {
    const body = { link_id: linkId };
    const days = parseInt(document.getElementById('shExpiry').value, 10);
    if (days) body.expires_days = days;
    const d = await api('POST', '/api/me/shares', body);
    document.getElementById('shNewUrl').value = location.origin + d.path;
    document.getElementById('shNewRow').style.display = '';
    show('shOk', 'Share link created — copy it below.');
    loadMyShares();
  } catch (err) { show('shErr', err.message); }
});
document.getElementById('shCopy').addEventListener('click', async () => {
  const v = document.getElementById('shNewUrl').value;
  try { await navigator.clipboard.writeText(v); } catch (_) { document.getElementById('shNewUrl').select(); document.execCommand('copy'); }
  show('shOk', 'Copied to clipboard.');
});
document.getElementById('shBody').addEventListener('click', async e => {
  const b = e.target.closest('[data-shrevoke]'); if (!b) return;
  if (!confirm('Revoke this share link? It stops working immediately.')) return;
  try { await api('POST', '/api/me/shares/' + b.dataset.shrevoke + '/revoke', {}); loadMyShares(); }
  catch (err) { show('shErr', err.message); }
});
document.getElementById('emForm').addEventListener('submit', async e => {
  e.preventDefault();
  try {
    await api('POST', '/api/me/email', { email: document.getElementById('emAddr').value });
    show('emOk', 'Email saved.');
  } catch (err) { show('emErr', err.message); }
});
document.getElementById('pwForm').addEventListener('submit', async e => {
  e.preventDefault();
  try {
    await api('POST', '/api/me/password', { current: document.getElementById('pwCur').value, new: document.getElementById('pwNew').value });
    show('pwOk', 'Password changed. Other sessions were signed out.'); e.target.reset();
  } catch (err) { show('pwErr', err.message); }
});
document.getElementById('logout').addEventListener('click', async () => {
  await fetch('/logout', { method: 'POST' }); location.href = '/';
});
(async () => {
  const s = await api('GET', '/api/me/session');
  CSRF = s.csrf; document.getElementById('who').textContent = s.username + ' (' + s.role + ')';
  document.getElementById('emAddr').value = s.email || '';
  loadTokens(); loadMyShares().catch(() => {}); loadProfile().catch(() => {});
})().catch(err => { document.getElementById('who').textContent = '—'; show('tokErr', err.message); });
</script>"#;
    shell("My account — Startpage", inner)
}

pub fn panel_page() -> String {
    let inner = r##"
<div class="topbar"><div><h1>Startpage admin</h1><p class="sub" style="margin:0">Signed in as <b id="who"></b> · panel at <code id="pathChip"></code></p></div>
<button id="logout">Sign out</button></div>

<div class="card glass">
  <h2>Links</h2>
  <p class="sub">Every tile on the public page is an opaque <code>/go/…</code> doorway — real URLs exist only here and in the server database. A <b>disguised admin link</b> looks like any other tile but opens this panel's login.</p>
  <div class="row" style="align-items:flex-end">
    <div><label>Editing link set of</label><select id="ownerSel"></select></div>
    <div style="flex:0"><button id="disguise" type="button">＋ New disguised admin link</button></div>
  </div>
  <div class="row" style="align-items:flex-end;margin-top:6px">
    <div style="flex:0"><button id="exportBtn" type="button">⬇ Export links (JSON)</button></div>
    <div><label>Import mode</label><select id="importMode"><option value="replace">Replace this user's whole set</option><option value="append">Append (skip duplicates)</option></select></div>
    <div style="flex:0"><button id="importBtn" type="button">⬆ Import links…</button><input type="file" id="importFile" accept="application/json,.json" hidden></div>
  </div>
  <p class="sub">Export downloads the selected user's complete link set as one JSON file. Import reads that format (hand-written files work too — see the README for the shape). Replace wipes the user's current set first; Append adds what's new and skips links whose section + URL already exist.</p>
  <div class="err" id="linkErr"></div><div class="ok" id="linkOk"></div>
  <div id="linkList"></div>
  <h3 class="sec" id="formTitle">Add a link</h3>
  <form id="linkForm">
    <input type="hidden" id="fId">
    <div class="row">
      <div><label>Label</label><input id="fLabel" required placeholder="Gmail"></div>
      <div><label>Kind</label><select id="fKind"><option value="url">Normal link</option><option value="container">Container link (ext+container://)</option><option value="admin">Admin login link (disguised)</option><option value="folder">Folder (desktop)</option></select></div>
    </div>
    <div id="containerRow" style="display:none"><label>Container name — pick a premade one or type a new one</label><input id="fContainerName" list="containerNames" placeholder="SOCIALS"><datalist id="containerNames"></datalist><div class="meta" style="margin-top:.2rem">The site below opens inside this Brave/Firefox container. The container must already exist in the browser.</div></div>
    <div id="urlRow"><label>URL</label><input id="fUrl" placeholder="https://example.com/"></div>
    <div class="row">
      <div><label>Icon</label><select id="fIconMode"><option value="auto">Auto — server fetches the site's favicon</option><option value="bundled">Bundled icon</option></select></div>
      <div id="iconPickRow"><label>Bundled icon</label><select id="fIcon"></select>
        <div style="margin-top:6px">
          <input type="file" id="fIconFile" accept=".png,.svg,.ico,.jpg,.jpeg,.webp,.gif" multiple>
          <button type="button" id="fIconUploadBtn">Upload icon(s)</button>
          <button type="button" id="fIconDeleteBtn" style="display:none">Delete selected upload</button>
          <div class="sub" id="iconUpMsg" style="margin-top:4px">Uploads live in the data folder (they survive rebuilds) and appear above as /uploads/… They also form the SHARED LIBRARY: every user can pick these images for their own link icons from the desktop editor. (Images users upload themselves stay private to them.)</div>
        </div>
      </div>
    </div>
    <div class="row">
      <div><label>Icon background (optional — makes transparent logos visible)</label>
        <div><input type="color" id="fIconBg" value="#2563eb"> <button type="button" class="small" id="fIconBgClear">None</button> <span class="meta" id="fIconBgState">none</span></div>
      </div>
    </div>
    <div class="row">
      <div><label>Section</label><select id="fSection"><option value="desktop">Desktop</option><option value="dock">Dock</option><option value="apps">All Apps</option><option value="about">About row</option><option value="folder">Folder</option></select></div>
      <div id="groupRow"><label>Group / row title (Apps &amp; About)</label><input id="fGroup" placeholder="Search &amp; AI"></div>
      <div id="folderPickRow" style="display:none"><label>Which folder</label><select id="fFolder"></select></div>
    </div>
    <div class="row">
      <div id="colRow"><label>Column (Desktop)</label><input id="fCol" type="number" min="0"></div>
      <div id="rowRow"><label>Row (Desktop)</label><input id="fRow" type="number" min="0"></div>
      <div><label>Order</label><input id="fSort" type="number" value="0"></div>
    </div>
    <div style="margin-top:14px" class="row">
      <button class="primary" type="submit">Save link</button>
      <button type="button" id="formCancel">Clear</button>
    </div>
  </form>
</div>

<div class="card glass">
  <h2>Users</h2>
  <p class="sub">Create accounts here, or let people register themselves (Settings → Registration, off by default). A new user starts with an EMPTY startpage and builds their own — add/edit links right from their desktop (Edit Desktop) or here in the panel. Email is used for the welcome message and password resets.</p>
  <div class="err" id="userErr"></div><div class="ok" id="userOk"></div>
  <table><thead><tr><th>Username</th><th>Email</th><th>Role</th><th>Status</th><th>Created</th><th></th></tr></thead><tbody id="userBody"></tbody></table>
  <form id="userForm" class="row" style="align-items:flex-end;margin-top:8px">
    <div><label>New username</label><input id="uName" required></div>
    <div><label>Temporary password</label><input id="uPass" type="text" required minlength="10"></div>
    <div><label>Email (optional)</label><input id="uEmail" type="email" placeholder="them@example.com"></div>
    <div style="flex:0"><button class="primary" type="submit">Create user</button></div>
  </form>
</div>

<div class="card glass">
  <h2>Reports <span class="badge" id="repCount" style="display:none"></span></h2>
  <p class="sub">Links, apps and folders your users flagged (right-click a tile, or the ⚑ button in a folder). Resolved and dismissed reports stay here for the record.</p>
  <div class="err" id="repErr"></div>
  <p class="sub" id="repMailHint" style="display:none;color:#ffb02e"></p>
  <table><thead><tr><th>When</th><th>Link</th><th>Belongs to</th><th>Reported by</th><th>Reason</th><th>Status</th><th></th></tr></thead><tbody id="repBody"></tbody></table>
</div>

<div class="card glass">
  <h2>Browser extension</h2>
  <p class="sub">Build a ready-to-load new-tab extension from this server's own page files, with your Branding (product name + tagline) baked into it. Unzip the download, then in Brave/Chrome open <b>brave://extensions</b>, enable Developer mode, choose <b>Load unpacked</b> and select the unzipped folder. In Firefox use <b>about:debugging → Load Temporary Add-on</b> and pick the manifest inside it. Finally open the extension's Options page and pair it with your server address and a sync token from the card below — it then mirrors your links and keeps a cached copy for offline use. (Publishing to the web stores instead? The listing copy lives in the project at <b>packaging/store-listing.md</b>.)</p>
  <div class="row">
    <div style="flex:0"><button class="primary" type="button" onclick="location.href='/api/admin/extension?flavor=chrome'">Download for Brave / Chrome (.zip)</button></div>
    <div style="flex:0"><button type="button" onclick="location.href='/api/admin/extension?flavor=firefox'">Download for Firefox (.zip)</button></div>
  </div>
</div>

<div class="card glass">
  <h2>Extension sync tokens</h2>
  <p class="sub">Tokens pair the browser extension to one person's link set. Plaintext is shown once, at creation. Revoke per device.</p>
  <div class="err" id="tokErr"></div><div class="ok" id="tokOnce"></div>
  <form id="tokForm" class="row" style="align-items:flex-end">
    <div><label>For user</label><select id="tokOwner"></select></div>
    <div><label>Device name</label><input id="tokName" placeholder="Brave — Dell" required></div>
    <div style="flex:0"><button class="primary" type="submit">Create token</button></div>
  </form>
  <table><thead><tr><th>Device</th><th>Owner</th><th>Created</th><th>Last used</th><th></th></tr></thead><tbody id="tokBody"></tbody></table>
</div>

<div class="card glass">
  <h2>Branding — About card</h2>
  <div class="err" id="brandErr"></div><div class="ok" id="brandOk"></div>
  <form id="brandForm">
    <div class="row">
      <div><label>Product name (heading + page title)</label><input id="bProduct"></div>
      <div><label>App icon</label><select id="bIcon"></select></div>
    </div>
    <label>Tagline</label><input id="bTagline">
    <label>About note</label><textarea id="bNote"></textarea>
    <div class="row">
      <div><label>Profile name</label><input id="bName"></div>
      <div><label>Profile sub-line</label><input id="bSub"></div>
    </div>
    <label>Browser chip links (About card) — auto-filled with this server's extension download links; replace with store URLs any time</label>
    <div class="row">
      <div><label>Chrome link</label><input id="bChrome" placeholder="https://… (blank = extension download)"></div>
      <div><label>Edge link</label><input id="bEdge" placeholder="https://… (blank = extension download)"></div>
      <div><label>Firefox link</label><input id="bFirefox" placeholder="https://… (blank = extension download)"></div>
    </div>
    <label>Profile rows (label / value)</label>
    <div id="bRows"></div>
    <button type="button" class="small" id="bAddRow">+ Add row</button>
    <div style="margin-top:14px"><button class="primary" type="submit">Save branding</button></div>
  </form>
</div>

<div class="card glass">
  <h2>Settings</h2>
  <div class="err" id="setErr"></div><div class="ok" id="setOk"></div>
  <form id="pathForm" class="row" style="align-items:flex-end">
    <div><label>Admin panel path (single segment, a-z 0-9 -, min 8 chars)</label><input id="sPath"></div>
    <div style="flex:0"><button type="submit">Change path</button></div>
  </form>
  <form id="pwForm" style="margin-top:10px">
    <h3 class="sec">Change my password</h3>
    <div class="row">
      <div><label>Current password</label><input id="pwCur" type="password" autocomplete="current-password" required></div>
      <div><label>New password (min. 10)</label><input id="pwNew" type="password" autocomplete="new-password" required minlength="10"></div>
    </div>
    <div style="margin-top:12px"><button type="submit">Change password</button></div>
  </form>
  <form id="regForm" style="margin-top:10px">
    <h3 class="sec">Registration</h3>
    <label style="display:flex;gap:8px;align-items:center;font-weight:400"><input type="checkbox" id="sRegOpen" style="width:auto"> Allow self-registration (anyone with the address can create an account; off by default)</label>
    <div style="margin-top:10px"><button type="submit">Save registration</button></div>
  </form>
  <form id="profForm" style="margin-top:10px">
    <h3 class="sec">Public profiles</h3>
    <label style="display:flex;gap:8px;align-items:center;font-weight:400"><input type="checkbox" id="sProfiles" style="width:auto"> Allow public profiles — one link per user (<code>/u/username</code>) with their profile, bio and startpage apps. Users opt in themselves from My account; off by default.</label>
    <div style="margin-top:10px"><button type="submit">Save profiles</button></div>
  </form>
  <form id="defOwnerForm" style="margin-top:10px">
    <h3 class="sec">Public startpage</h3>
    <p class="sub" style="margin:0 0 8px">Whose startpage signed-out visitors see at the home address. Signed-in users always see their own. Pick the account whose page should be the public face.</p>
    <div class="row" style="align-items:flex-end">
      <div><label>Default startpage owner</label><select id="sDefOwner"></select></div>
      <div style="flex:0"><button type="submit">Save startpage</button></div>
    </div>
  </form>
</div>

<div class="card glass">
  <h2>Share links</h2>
  <p class="sub">Give someone a public link to one app or a whole folder — it opens a read-only landing page at <code>/s/…</code>. The token is the key: anyone holding it can open the shared items until you revoke it or it expires. Creating a new link for an item replaces its old one. Disguised admin links can't be shared, and LAN-only addresses (192.168…) only work for visitors on your network.</p>
  <div class="err" id="shareErr"></div><div class="ok" id="shareOk"></div>
  <div class="row" style="align-items:flex-end">
    <div><label>App or folder (from the set above)</label><select id="shareItem"></select></div>
    <div><label>Expires</label><select id="shareExpiry"><option value="">Never</option><option value="7">In 7 days</option><option value="30">In 30 days</option></select></div>
    <div style="flex:0"><button class="primary" id="shareCreate" type="button">Create share link</button></div>
  </div>
  <div class="row" id="shareNewRow" style="display:none;align-items:flex-end">
    <div><label>Your share link</label><input id="shareNewUrl" readonly></div>
    <div style="flex:0"><button id="shareCopy" type="button">Copy</button></div>
  </div>
  <table><thead><tr><th>Item</th><th>Share link</th><th>Expires</th><th>Views</th><th>Created</th><th></th></tr></thead><tbody id="shareBody"></tbody></table>
</div>

<div class="card glass">
  <h2>Email (SMTP)</h2>
  <p class="sub">Outgoing mail for welcome messages, password-reset links and report notifications (reports email every admin who has an address on their account — set yours under My account → Email). Leave the password blank to keep the saved one. Your mail provider's app password works here.</p>
  <div class="err" id="smtpErr"></div><div class="ok" id="smtpOk"></div>
  <form id="smtpForm">
    <div class="row">
      <div><label>SMTP host</label><input id="mHost" placeholder="smtp.example.com"></div>
      <div><label>Port</label><input id="mPort" type="number" min="1" max="65535" placeholder="587"></div>
      <div><label>Encryption</label><select id="mEnc"><option value="starttls">STARTTLS (587)</option><option value="tls">SSL/TLS (465)</option><option value="none">None (plain)</option></select></div>
    </div>
    <div class="row">
      <div><label>Username</label><input id="mUser" autocomplete="off"></div>
      <div><label>Password</label><input id="mPass" type="password" autocomplete="new-password" placeholder="(unchanged)"></div>
    </div>
    <div class="row">
      <div><label>From address</label><input id="mFromAddr" type="email" placeholder="startpage@example.com"></div>
      <div><label>From name</label><input id="mFromName" placeholder="DOX BOX"></div>
    </div>
    <div style="margin-top:12px"><button class="primary" type="submit">Save SMTP</button></div>
  </form>
  <div class="row" style="align-items:flex-end;margin-top:14px">
    <div><label>Send a test email to</label><input id="mTestTo" type="email" placeholder="you@example.com"></div>
    <div style="flex:0"><button type="button" id="mTestBtn">Send test</button></div>
  </div>
</div>

<script>
let CSRF = '', ME = null, USERS = [], ICONS = [], LINKS = [];
const $ = id => document.getElementById(id);
const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const fmt = ts => ts ? new Date(ts * 1000).toLocaleString() : 'never';
function show(id, msg) { const e = $(id); e.textContent = msg; e.style.display = 'block'; }
function hide(id) { $(id).style.display = 'none'; }
async function api(method, url, body) {
  const h = { 'Content-Type': 'application/json' };
  if (method !== 'GET') h['X-CSRF-Token'] = CSRF;
  const r = await fetch(url, { method, headers: h, body: body ? JSON.stringify(body) : undefined });
  const d = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(d.error || ('HTTP ' + r.status));
  return d;
}
const ownerName = id => (USERS.find(u => u.id === id) || {}).username || id;

// ── Links ──────────────────────────────────────────────────────
function safeDec(s) { try { return decodeURIComponent(s); } catch { return s; } }
function parseContainer(u) {
  const rest = u.slice('ext+container:'.length);
  if (rest.startsWith('//')) {
    const path = rest.slice(2); const i = path.indexOf('/');
    if (i > 0) return { name: safeDec(path.slice(0, i)), inner: path.slice(i + 1) };
    return null;
  }
  const m = /(?:^|&)name=([^&]*)/.exec(rest); const v = /(?:^|&)url=([^&]*)/.exec(rest);
  if (v) return { name: m ? safeDec(m[1]) : '', inner: safeDec(v[1]) };
  return null;
}
function fillContainerNames() {
  const names = new Set(['SOCIALS', 'GOOGLE', 'PERSONAL', 'WORK', 'SHOPPING', 'BANKING']);
  for (const l of LINKS) {
    if (l.url && l.url.startsWith('ext+container:')) { const p = parseContainer(l.url); if (p && p.name) names.add(p.name); }
  }
  $('containerNames').innerHTML = [...names].map(n => '<option value="' + n.replace(/"/g, '&quot;') + '">').join('');
}
function kindSync() {
  const k = $('fKind').value;
  $('urlRow').style.display = k === 'admin' ? 'none' : '';
  $('containerRow').style.display = k === 'container' ? '' : 'none';
  $('fUrl').disabled = k === 'folder';
  $('fUrl').placeholder = k === 'folder' ? 'Folders have no URL — they hold other links' : (k === 'container' ? 'https://social.chiefgyk3d.com/public — the site to open in the container' : 'https://example.com/');
  $('iconPickRow').style.display = $('fIconMode').value === 'bundled' ? '' : 'none';
  const desk = $('fSection').value === 'desktop' || $('fSection').value === 'folder';
  $('colRow').style.display = desk ? '' : 'none'; $('rowRow').style.display = desk ? '' : 'none';
  $('groupRow').style.display = desk || $('fSection').value === 'dock' ? 'none' : '';
  $('folderPickRow').style.display = $('fSection').value === 'folder' ? '' : 'none';
}
function fillFolderPick() {
  const folders = LINKS.filter(l => l.kind === 'folder');
  $('fFolder').innerHTML = folders.length
    ? folders.map(f => '<option value="' + esc(f.id) + '">' + esc(f.label) + '</option>').join('')
    : '<option value="">(no folders yet — create one with Kind = Folder)</option>';
}
['fKind', 'fIconMode', 'fSection'].forEach(id => $(id).addEventListener('change', kindSync));
function linkFormReset() {
  $('linkForm').reset(); $('fId').value = ''; $('fSort').value = 0; $('formTitle').textContent = 'Add a link'; setIconBg(null); kindSync();
}
$('formCancel').addEventListener('click', linkFormReset);
function renderLinks() {
  const secs = [['desktop', 'Desktop'], ['dock', 'Dock'], ['apps', 'All Apps'], ['about', 'About rows']];
  $('linkList').innerHTML = secs.map(([sec, title]) => {
    const items = LINKS.filter(l => l.section === sec);
    if (!items.length) return '';
    return '<h3 class="sec">' + title + '</h3>' + items.map(l =>
      '<div class="linkrow"><img src="' + esc(l.icon.startsWith('/') ? l.icon : '/' + l.icon) + '" alt="">' +
      '<div class="grow"><b>' + esc(l.label) + '</b> ' +
      (l.kind === 'admin' ? '<span class="badge admin">admin link → /' + esc(ME.admin_path) + '</span>' : '<span class="meta">' + esc(l.url || '') + '</span>') +
      '<div class="meta">' + (l.folder_id ? 'in folder: ' + esc((LINKS.find(x => x.id === l.folder_id) || {}).label || '?') + ' · ' : '') + (l.group_title ? 'group: ' + esc(l.group_title) + ' · ' : '') +
      (sec === 'desktop' ? 'col ' + (l.col ?? '—') + ', row ' + (l.row ?? '—') + ' · ' : '') + 'order ' + l.sort + '</div></div>' +
      '<button class="small" data-edit="' + l.id + '">Edit</button>' +
      '<button class="small danger" data-del="' + l.id + '">Delete</button></div>').join('');
  }).join('') || '<p class="sub">No links yet.</p>';
}
$('linkList').addEventListener('click', async e => {
  const ed = e.target.closest('[data-edit]'), del = e.target.closest('[data-del]');
  if (ed) {
    const l = LINKS.find(x => x.id === ed.dataset.edit); if (!l) return;
    $('fId').value = l.id; $('fLabel').value = l.label;
    let kindForForm = l.kind, urlForForm = l.url || '', containerName = '';
    if (urlForForm.startsWith('ext+container:')) {
      const parsed = parseContainer(urlForForm);
      if (parsed) { kindForForm = 'container'; containerName = parsed.name; urlForForm = parsed.inner; }
    }
    $('fKind').value = kindForForm; $('fContainerName').value = containerName;
    $('fUrl').value = urlForForm; $('fIconMode').value = l.icon_mode;
    if (l.icon_mode === 'bundled') $('fIcon').value = l.icon;
    $('fSection').value = l.section; $('fGroup').value = l.group_title || '';
    if (l.folder_id) { $('fSection').value = 'folder'; fillFolderPick(); $('fFolder').value = l.folder_id; }
    setIconBg(l.icon_bg || null);
    $('fCol').value = l.col ?? ''; $('fRow').value = l.row ?? ''; $('fSort').value = l.sort;
    $('formTitle').textContent = 'Edit link'; kindSync(); window.scrollTo({ top: $('linkForm').offsetTop - 120, behavior: 'smooth' });
  }
  if (del) {
    const l = LINKS.find(x => x.id === del.dataset.del);
    if (!confirm('Delete “' + (l ? l.label : '') + '”?')) return;
    try { await api('DELETE', '/api/admin/links/' + del.dataset.del); loadLinks(); }
    catch (err) { show('linkErr', err.message); }
  }
});
$('exportBtn').addEventListener('click', async () => {
  hide('linkErr'); hide('linkOk');
  try {
    const r = await fetch('/api/admin/links/export?owner=' + encodeURIComponent($('ownerSel').value));
    if (!r.ok) throw new Error('HTTP ' + r.status);
    const blob = await r.blob();
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = 'startpage-links-' + ownerName($('ownerSel').value) + '.json';
    document.body.appendChild(a); a.click(); a.remove();
    setTimeout(() => URL.revokeObjectURL(a.href), 2000);
    show('linkOk', 'Exported ' + ownerName($('ownerSel').value) + '’s links.');
  } catch (e) { show('linkErr', String(e.message || e)); }
});
$('importBtn').addEventListener('click', () => $('importFile').click());
$('importFile').addEventListener('change', async () => {
  const f = $('importFile').files[0]; $('importFile').value = '';
  if (!f) return;
  hide('linkErr'); hide('linkOk');
  try {
    const parsed = JSON.parse(await f.text());
    if (!parsed || !Array.isArray(parsed.links)) throw new Error('That file has no "links" array — not a startpage export.');
    const mode = $('importMode').value, who = ownerName($('ownerSel').value);
    if (mode === 'replace' && !confirm('Replace ALL of ' + who + '’s current links with the ' + parsed.links.length + ' links in this file?')) return;
    const d = await api('POST', '/api/admin/links/import', { owner: $('ownerSel').value, mode, links: parsed.links });
    let msg = 'Imported ' + d.imported + ' links for ' + who + (d.skipped_duplicates ? ' · skipped ' + d.skipped_duplicates + ' duplicates' : '') + '.';
    show('linkOk', msg);
    if (d.errors && d.errors.length) show('linkErr', 'Skipped ' + d.errors.length + ' bad entries: ' + d.errors.slice(0, 4).join(' · ') + (d.errors.length > 4 ? ' …' : ''));
    loadLinks();
  } catch (e) { show('linkErr', String(e.message || e)); }
});
async function loadLinks() {
  const d = await api('GET', '/api/admin/links?owner=' + encodeURIComponent($('ownerSel').value));
  LINKS = d.links; renderLinks(); fillFolderPick(); fillContainerNames(); loadShares();
}
$('ownerSel').addEventListener('change', () => { linkFormReset(); loadLinks(); });
let iconBgChoice = null;
function expandHex(h) { return h.length === 4 ? '#' + h[1] + h[1] + h[2] + h[2] + h[3] + h[3] : h; }
function setIconBg(v) {
  iconBgChoice = v || null;
  if (iconBgChoice) { $('fIconBg').value = expandHex(iconBgChoice); $('fIconBgState').textContent = iconBgChoice; }
  else $('fIconBgState').textContent = 'none';
}
$('fIconBg').addEventListener('input', e => setIconBg(e.target.value));
$('fIconBgClear').addEventListener('click', () => setIconBg(null));
$('linkForm').addEventListener('submit', async e => {
  e.preventDefault(); hide('linkErr'); hide('linkOk');
  let kindVal = $('fKind').value;
  let urlVal = $('fUrl').value;
  if (kindVal === 'container') {
    const cname = $('fContainerName').value.trim().replace(/\s+/g, '%20');
    if (!cname) { show('linkErr', 'Pick or type a container name first.'); return; }
    let inner = urlVal.trim();
    if (inner && !inner.includes('://')) inner = 'https://' + inner;
    urlVal = 'ext+container://' + cname + '/' + inner;
    kindVal = 'url';
  }
  const body = {
    owner: $('ownerSel').value, label: $('fLabel').value, kind: kindVal,
    url: (kindVal === 'url') ? urlVal : null,
    icon_mode: $('fIconMode').value, icon: $('fIconMode').value === 'bundled' ? $('fIcon').value : null,
    section: $('fSection').value === 'folder' ? 'desktop' : $('fSection').value,
    folder_id: $('fSection').value === 'folder' ? ($('fFolder').value || null) : null,
    icon_bg: iconBgChoice,
    group_title: $('fGroup').value || null,
    col: $('fCol').value === '' ? null : +$('fCol').value,
    row: $('fRow').value === '' ? null : +$('fRow').value,
    sort: +$('fSort').value || 0,
  };
  try {
    const id = $('fId').value;
    if (id) await api('PUT', '/api/admin/links/' + id, body); else await api('POST', '/api/admin/links', body);
    show('linkOk', 'Saved.'); linkFormReset(); loadLinks();
  } catch (err) { show('linkErr', err.message); }
});
$('disguise').addEventListener('click', async () => {
  hide('linkErr'); hide('linkOk');
  const occ = new Set(LINKS.filter(l => l.owner_id === ME.user_id && l.section === 'desktop' && l.col != null).map(l => l.col + ',' + l.row));
  let col = 0, row = 0, found = false;
  for (let c = 0; c < 8 && !found; c++) for (let r = 0; r < 30 && !found; r++) if (!occ.has(c + ',' + r)) { col = c; row = r; found = true; }
  try {
    await api('POST', '/api/admin/links', { owner: ME.user_id, label: 'Gmail', kind: 'admin', url: null,
      icon_mode: 'bundled', icon: 'src/icons/mail.svg', section: 'desktop', col, row, sort: 0, group_title: null });
    $('ownerSel').value = ME.user_id; show('linkOk', 'Disguised admin link created on your desktop — it looks like Gmail and opens this panel’s login.'); loadLinks();
  } catch (err) { show('linkErr', err.message); }
});

// ── Users ──────────────────────────────────────────────────────
function renderUsers() {
  $('userBody').innerHTML = USERS.map(u =>
    '<tr><td><b>' + esc(u.username) + '</b></td><td>' + esc(u.email || '—') + '</td><td>' + (u.role === 'admin' ? '<span class="badge admin">admin</span>' : 'user') + '</td>' +
    '<td>' + (u.disabled ? '<span class="badge off">disabled</span>' : 'active') + '</td><td>' + fmt(u.created_at) + '</td>' +
    '<td style="white-space:nowrap">' + (u.role !== 'admin'
      ? '<button class="small" data-toggle="' + u.id + '">' + (u.disabled ? 'Enable' : 'Disable') + '</button> ' +
        '<button class="small" data-reset="' + u.id + '">Reset PW</button> ' +
        '<button class="small danger" data-udel="' + u.id + '">Delete</button>' : '') + '</td></tr>').join('');
  $('ownerSel').innerHTML = USERS.map(u => '<option value="' + u.id + '"' + (u.id === ME.user_id ? ' selected' : '') + '>' + esc(u.username) + (u.role === 'admin' ? ' (admin)' : '') + '</option>').join('');
  $('tokOwner').innerHTML = USERS.map(u => '<option value="' + u.id + '">' + esc(u.username) + '</option>').join('');
}
async function loadUsers() { USERS = (await api('GET', '/api/admin/users')).users; renderUsers(); }
$('userBody').addEventListener('click', async e => {
  const t = e.target.closest('[data-toggle]'), r = e.target.closest('[data-reset]'), d = e.target.closest('[data-udel]');
  try {
    if (t) { const u = USERS.find(x => x.id === t.dataset.toggle); await api('POST', '/api/admin/users/' + u.id + '/disabled', { disabled: !u.disabled }); }
    if (r) { const pw = prompt('New temporary password for this user (min. 10 chars):'); if (!pw) return; await api('POST', '/api/admin/users/' + r.dataset.reset + '/password', { password: pw }); show('userOk', 'Password reset; the user’s sessions were signed out.'); }
    if (d) { const u = USERS.find(x => x.id === d.dataset.udel); if (!confirm('Delete user “' + u.username + '” and all their links and tokens?')) return; await api('DELETE', '/api/admin/users/' + u.id); }
    if (t || r || d) { await loadUsers(); loadLinks(); loadTokens(); }
  } catch (err) { show('userErr', err.message); }
});
$('userForm').addEventListener('submit', async e => {
  e.preventDefault(); hide('userErr'); hide('userOk');
  try {
    const d = await api('POST', '/api/admin/users', { username: $('uName').value, password: $('uPass').value, email: $('uEmail').value || null });
    show('userOk', 'User “' + d.user.username + '” created with a copy of your current link set.'); e.target.reset();
    await loadUsers(); loadTokens();
  } catch (err) { show('userErr', err.message); }
});

// ── Tokens ─────────────────────────────────────────────────────
async function loadTokens() {
  const d = await api('GET', '/api/admin/sync-tokens');
  $('tokBody').innerHTML = d.tokens.map(t =>
    '<tr><td>' + esc(t.name) + '</td><td>' + esc(t.username) + '</td><td>' + fmt(t.created_at) + '</td><td>' + fmt(t.last_used_at) +
    '</td><td><button class="small danger" data-revoke="' + t.id + '">Revoke</button></td></tr>').join('')
    || '<tr><td colspan="5" style="color:rgba(255,255,255,.5)">No tokens yet</td></tr>';
}
$('tokBody').addEventListener('click', async e => {
  const b = e.target.closest('[data-revoke]'); if (!b) return;
  if (!confirm('Revoke this token?')) return;
  await api('DELETE', '/api/admin/sync-tokens/' + b.dataset.revoke); loadTokens();
});
$('tokForm').addEventListener('submit', async e => {
  e.preventDefault(); hide('tokErr'); hide('tokOnce');
  try {
    const d = await api('POST', '/api/admin/sync-tokens', { name: $('tokName').value, user_id: $('tokOwner').value });
    show('tokOnce', 'Token for “' + d.name + '” (' + esc(d.username || '') + ') — copy it now, it will not be shown again:');
    const box = document.createElement('div'); box.className = 'once'; box.textContent = d.token;
    $('tokOnce').appendChild(box);
    $('tokName').value = ''; loadTokens();
  } catch (err) { show('tokErr', err.message); }
});

// ── Branding ───────────────────────────────────────────────────
function brandRow(label, value) {
  const div = document.createElement('div'); div.className = 'row'; div.style.marginBottom = '8px';
  div.innerHTML = '<div><input data-bl value="' + esc(label) + '" placeholder="Label"></div>' +
    '<div><input data-bv value="' + esc(value) + '" placeholder="Value"></div>' +
    '<div style="flex:0"><button type="button" class="small danger">✕</button></div>';
  div.querySelector('button').addEventListener('click', () => div.remove());
  $('bRows').appendChild(div);
}
$('bAddRow').addEventListener('click', () => brandRow('', ''));
async function loadBranding() {
  const b = (await api('GET', '/api/admin/branding')).branding;
  $('bProduct').value = b.product_name; $('bTagline').value = b.tagline; $('bNote').value = b.about_note;
  $('bName').value = b.profile_name; $('bSub').value = b.profile_sub; $('bIcon').value = b.app_icon;
  $('bChrome').value = b.chrome_url || ''; $('bEdge').value = b.edge_url || ''; $('bFirefox').value = b.firefox_url || '';
  // Auto-fill blanks with this server's own extension-download paths.
  // Relative on purpose: the chip then works from the public domain and
  // the LAN alike, for any signed-in user (they pair with their token).
  if (!$('bChrome').value) $('bChrome').value = '/api/admin/extension?flavor=chrome';
  if (!$('bEdge').value) $('bEdge').value = '/api/admin/extension?flavor=chrome';
  if (!$('bFirefox').value) $('bFirefox').value = '/api/admin/extension?flavor=firefox';
  $('bRows').innerHTML = ''; (b.profile_rows || []).forEach(r => brandRow(r.label, r.value));
}
$('brandForm').addEventListener('submit', async e => {
  e.preventDefault(); hide('brandErr'); hide('brandOk');
  const rows = [...$('bRows').children].map(div => ({
    label: div.querySelector('[data-bl]').value, value: div.querySelector('[data-bv]').value,
  })).filter(r => r.label || r.value);
  try {
    await api('POST', '/api/admin/branding', { product_name: $('bProduct').value, tagline: $('bTagline').value,
      about_note: $('bNote').value, profile_name: $('bName').value, profile_sub: $('bSub').value,
      app_icon: $('bIcon').value, profile_rows: rows,
      chrome_url: $('bChrome').value, edge_url: $('bEdge').value, firefox_url: $('bFirefox').value });
    show('brandOk', 'Branding saved.');
  } catch (err) { show('brandErr', err.message); }
});

// ── Settings ───────────────────────────────────────────────────
// ── Site: registration + SMTP ──────────────────────────────────
async function loadSite() {
  const d = await api('GET', '/api/admin/site');
  $('sRegOpen').checked = !!d.registration_open;
  $('sProfiles').checked = !!d.profiles_enabled;
  try {
    const us = await api('GET', '/api/admin/users');
    $('sDefOwner').innerHTML = us.users.map(u => '<option value="' + u.id + '">' + esc(u.username) + (u.role === 'admin' ? ' (admin)' : '') + (u.disabled ? ' — disabled' : '') + '</option>').join('');
    if (d.default_owner) $('sDefOwner').value = d.default_owner;
  } catch (_) {}
  const s = d.smtp || {};
  $('mHost').value = s.host || ''; $('mPort').value = s.port || 587; $('mEnc').value = s.encryption || 'starttls';
  $('mUser').value = s.username || ''; $('mFromAddr').value = s.from_addr || ''; $('mFromName').value = s.from_name || '';
  $('mPass').placeholder = s.has_password ? '•••••••• (unchanged)' : '';
}
$('regForm').addEventListener('submit', async e => {
  e.preventDefault(); hide('setErr'); hide('setOk');
  try {
    await api('POST', '/api/admin/site/registration', { open: $('sRegOpen').checked });
    show('setOk', $('sRegOpen').checked ? 'Registration is ON — the sign-in page now offers Create account.' : 'Registration is OFF.');
  } catch (err) { show('setErr', err.message); }
});
$('defOwnerForm').addEventListener('submit', async e => {
  e.preventDefault(); hide('setErr'); hide('setOk');
  try {
    await api('POST', '/api/admin/site/default-owner', { user_id: $('sDefOwner').value });
    show('setOk', 'Public startpage saved — signed-out visitors now see that account\u2019s page.');
  } catch (err) { show('setErr', err.message); }
});
$('profForm').addEventListener('submit', async e => {
  e.preventDefault(); hide('setErr'); hide('setOk');
  try {
    await api('POST', '/api/admin/site/profiles', { enabled: $('sProfiles').checked });
    show('setOk', $('sProfiles').checked ? 'Public profiles are ON — users can publish theirs from My account.' : 'Public profiles are OFF — every /u/ page is hidden.');
  } catch (err) { show('setErr', err.message); }
});
$('smtpForm').addEventListener('submit', async e => {
  e.preventDefault(); hide('smtpErr'); hide('smtpOk');
  try {
    await api('POST', '/api/admin/smtp', { host: $('mHost').value, port: +$('mPort').value || 587,
      username: $('mUser').value, password: $('mPass').value || null, from_addr: $('mFromAddr').value,
      from_name: $('mFromName').value, encryption: $('mEnc').value });
    $('mPass').value = ''; show('smtpOk', 'SMTP saved.'); loadSite();
  } catch (err) { show('smtpErr', err.message); }
});
$('mTestBtn').addEventListener('click', async () => {
  hide('smtpErr'); hide('smtpOk');
  try {
    await api('POST', '/api/admin/smtp/test', { to: $('mTestTo').value });
    show('smtpOk', 'Test email sent to ' + $('mTestTo').value + ' — check the inbox.');
  } catch (err) { show('smtpErr', 'Test failed: ' + err.message); }
});

// ── Reports ────────────────────────────────────────────────────
async function loadReports() {
  const d = await api('GET', '/api/admin/reports');
  try {
    const u = await api('GET', '/api/admin/users');
    const hint = $('repMailHint');
    if (u.users.some(x => x.role === 'admin' && x.email)) { hint.style.display = 'none'; }
    else {
      hint.style.display = '';
      hint.textContent = 'Heads up: report emails are OFF — no admin account has an email address yet. Add yours at /account → Email and new reports will email you.';
    }
  } catch (_) {}
  const open = d.reports.filter(r => r.status === 'open').length;
  $('repCount').style.display = open ? '' : 'none';
  $('repCount').textContent = open + ' open';
  $('repBody').innerHTML = d.reports.map(r => {
    let ic = '';
    if (r.icon && r.link_id) {
      const src = r.icon === 'auto' ? '/icon/' + r.link_id : (r.icon.startsWith('src/') ? '/' + r.icon : r.icon);
      ic = '<img src="' + esc(src) + '" alt="" style="width:22px;height:22px;border-radius:6px;vertical-align:-6px;margin-right:7px" onerror="this.remove()"> ';
    }
    return '<tr><td>' + fmt(r.created_at) + '</td><td>' + ic + '<b>' + esc(r.label) + '</b>' + (r.icon_upload ? ' <span class="sub">(uploaded icon)</span>' : '') +
      (r.url ? '<div class="sub" style="max-width:340px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title="' + esc(r.url) + '">' + esc(r.url) + '</div>' : '') +
      (r.source && r.source !== 'startpage' ? '<div class="sub">via ' + esc(r.source.replace('share:', '').replace('profile:', '')) + (r.source.startsWith('share:') ? ' (share link)' : ' (profile)') + '</div>' : '') + '</td><td>' + esc(r.owner) + '</td>' +
    '<td>' + esc(r.reporter) + '</td><td>' + esc(r.reason || '—') + '</td><td>' + esc(r.status) + '</td>' +
    '<td style="white-space:nowrap">' + (r.status === 'open'
      ? '<button class="small" data-rep="resolved" data-id="' + r.id + '">Resolve</button> <button class="small" data-rep="dismissed" data-id="' + r.id + '">Dismiss</button>'
      : '<button class="small" data-rep="open" data-id="' + r.id + '">Reopen</button>') +
      (r.icon_upload ? ' <button class="small" data-repicon="' + r.id + '">Remove icon</button>' : '') +
      ' <button class="small danger" data-repdel="' + r.id + '">Delete</button></td></tr>';
  }).join('') ||
    '<tr><td colspan="7" class="sub">No reports yet.</td></tr>';
}
$('repBody').addEventListener('click', async e => {
  const del = e.target.closest('[data-repdel]');
  if (del) {
    if (!confirm('Delete this report permanently?')) return;
    try { await api('POST', '/api/admin/reports/' + del.dataset.repdel + '/delete', {}); loadReports(); }
    catch (err) { show('repErr', err.message); }
    return;
  }
  const ric = e.target.closest('[data-repicon]');
  if (ric) {
    if (!confirm('Remove this uploaded icon from the link and delete the file? The report is marked resolved.')) return;
    try { await api('POST', '/api/admin/reports/' + ric.dataset.repicon + '/remove-icon', {}); show('repOk', 'Icon removed.'); loadReports(); }
    catch (err) { show('repErr', err.message); }
    return;
  }
  const b = e.target.closest('[data-rep]'); if (!b) return;
  try { await api('POST', '/api/admin/reports/' + b.dataset.id + '/status', { status: b.dataset.rep }); loadReports(); }
  catch (err) { show('repErr', err.message); }
});

// ── Share links ────────────────────────────────────────────────
function fillShareItems() {
  const sel = $('shareItem'); if (!sel) return;
  const cur = sel.value;
  sel.innerHTML = LINKS.filter(l => l.kind === 'url' || l.kind === 'folder')
    .map(l => '<option value="' + l.id + '">' + esc(l.label) + (l.kind === 'folder' ? ' (folder)' : '') + '</option>').join('')
    || '<option value="">(no apps yet)</option>';
  if (cur) sel.value = cur;
}
async function loadShares() {
  if (!$('shareBody')) return;
  fillShareItems();
  const d = await api('GET', '/api/admin/shares?owner=' + encodeURIComponent($('ownerSel').value));
  $('shareBody').innerHTML = d.shares.map(s =>
    '<tr><td><b>' + esc(s.label) + '</b>' + (s.kind === 'folder' ? ' <span class="sub">(folder)</span>' : '') + '</td>' +
    '<td><code>/s/' + esc(s.token) + '</code></td>' +
    '<td>' + (s.expires_at ? fmt(s.expires_at) : 'never') + '</td><td>' + s.views + '</td><td>' + fmt(s.created_at) + '</td>' +
    '<td><button class="small danger" data-shrevoke="' + s.id + '">Revoke</button></td></tr>').join('') ||
    '<tr><td colspan="6" class="sub">No share links yet.</td></tr>';
}
$('shareCreate').addEventListener('click', async () => {
  hide('shareErr'); hide('shareOk');
  const linkId = $('shareItem').value;
  if (!linkId) { show('shareErr', 'Pick an app or folder first.'); return; }
  try {
    const body = { link_id: linkId };
    const days = parseInt($('shareExpiry').value, 10);
    if (days) body.expires_days = days;
    const d = await api('POST', '/api/admin/shares', body);
    $('shareNewUrl').value = location.origin + d.path;
    $('shareNewRow').style.display = '';
    show('shareOk', 'Share link created — copy it below. Any previous link for this item stops working.');
    loadShares();
  } catch (err) { show('shareErr', err.message); }
});
$('shareCopy').addEventListener('click', async () => {
  try { await navigator.clipboard.writeText($('shareNewUrl').value); show('shareOk', 'Copied to clipboard.'); }
  catch (_) { $('shareNewUrl').select(); document.execCommand('copy'); show('shareOk', 'Copied to clipboard.'); }
});
$('shareBody').addEventListener('click', async e => {
  const b = e.target.closest('[data-shrevoke]'); if (!b) return;
  if (!confirm('Revoke this share link? It stops working immediately.')) return;
  try { await api('POST', '/api/admin/shares/' + b.dataset.shrevoke + '/revoke', {}); loadShares(); }
  catch (err) { show('shareErr', err.message); }
});
$('pathForm').addEventListener('submit', async e => {
  e.preventDefault(); hide('setErr'); hide('setOk');
  try {
    const d = await api('POST', '/api/admin/settings', { admin_path: $('sPath').value });
    show('setOk', 'Admin path changed — redirecting…');
    setTimeout(() => location.href = '/' + d.admin_path, 700);
  } catch (err) { show('setErr', err.message); }
});
$('pwForm').addEventListener('submit', async e => {
  e.preventDefault(); hide('setErr'); hide('setOk');
  try {
    await api('POST', '/api/admin/password', { current: $('pwCur').value, new: $('pwNew').value });
    show('setOk', 'Password changed.'); e.target.reset();
  } catch (err) { show('setErr', err.message); }
});
$('logout').addEventListener('click', async () => { await fetch(location.pathname + '/logout', { method: 'POST' }); location.href = '/'; });

// ── Init ───────────────────────────────────────────────────────
(async () => {
  ME = await api('GET', '/api/admin/session');
  CSRF = ME.csrf; $('who').textContent = ME.username; $('pathChip').textContent = '/' + ME.admin_path; $('sPath').value = ME.admin_path;
  ICONS = (await api('GET', '/api/admin/icons')).icons;
  $('fIcon').innerHTML = ICONS.map(i => '<option value="' + esc(i) + '">' + esc(i) + '</option>').join('');
  $('bIcon').innerHTML = ICONS.map(i => '<option value="' + esc(i) + '">' + esc(i) + '</option>').join('');
  await loadUsers(); await loadLinks(); await loadTokens(); await loadBranding(); await loadSite(); await loadReports(); kindSync();
})().catch(err => show('linkErr', err.message));

// ── Icon uploads ────────────────────────────────────────────────────
async function refreshIcons(selectPath) {
  ICONS = (await api('GET', '/api/admin/icons')).icons;
  const html = ICONS.map(i => '<option value="' + esc(i) + '">' + esc(i) + '</option>').join('');
  $('fIcon').innerHTML = html; $('bIcon').innerHTML = html;
  if (selectPath) $('fIcon').value = selectPath;
  syncIconDeleteBtn();
}
function syncIconDeleteBtn() {
  $('fIconDeleteBtn').style.display = ($('fIcon').value || '').startsWith('/uploads/') ? '' : 'none';
}
$('fIcon').addEventListener('change', syncIconDeleteBtn);
$('fIconUploadBtn').addEventListener('click', async () => {
  const files = Array.from($('fIconFile').files || []);
  if (!files.length) { $('iconUpMsg').textContent = 'Choose one or more image files first.'; return; }
  let lastPath = null, done = 0;
  const failed = [];
  for (const f of files) {
    try {
      const buf = await f.arrayBuffer();
      const bytes = new Uint8Array(buf);
      let bin = '';
      for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000));
      const d = await api('POST', '/api/admin/icons/upload', { filename: f.name, data_base64: btoa(bin) });
      lastPath = d.path; done++;
    } catch (e) { failed.push(f.name + ' (' + e.message + ')'); }
  }
  if (lastPath) { $('fIconMode').value = 'bundled'; kindSync(); await refreshIcons(lastPath); }
  $('iconUpMsg').textContent = 'Uploaded ' + done + ' image' + (done === 1 ? '' : 's') + ' to the shared library' +
    (files.length === 1 && lastPath ? ' as ' + lastPath + ' and selected it — now save the link to use it.' : '. Users can pick them from the desktop editor now.') +
    (failed.length ? ' Failed: ' + failed.join(', ') : '');
});
$('fIconDeleteBtn').addEventListener('click', async () => {
  const v = $('fIcon').value || '';
  if (!v.startsWith('/uploads/')) return;
  try {
    await api('DELETE', '/api/admin/icons/' + encodeURIComponent(v.split('/').pop()));
    await refreshIcons(null);
    $('iconUpMsg').textContent = 'Upload deleted. Any link still using it will show a placeholder until edited.';
  } catch (e) { $('iconUpMsg').textContent = 'Delete failed: ' + e.message; }
});
</script>"##;
    shell("Admin — Startpage", inner)
}
