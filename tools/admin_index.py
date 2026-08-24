# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

from __future__ import annotations

from html import escape


def admin_index_html(
    *,
    title: str,
    front_door: str,
    commit_hash: str,
    cycle_products_root: str,
    live_feed_output_root: str,
) -> str:
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{escape(title)}</title>
<style>
:root {{ --paper:#f2efe7; --ink:#19231d; --muted:#657069; --line:#b9c1b9; --panel:#fffdf7; --ok:#167447; --warn:#a45d00; --bad:#a92525; --blue:#075985; }}
* {{ box-sizing:border-box; }}
body {{ margin:0; color:var(--ink); background:linear-gradient(135deg,#e7eee8 0,#f2efe7 38%,#ede6d7 100%); font:15px/1.45 ui-sans-serif,system-ui,sans-serif; }}
main {{ max-width:1200px; margin:0 auto; padding:28px 20px 60px; }}
header {{ display:flex; justify-content:space-between; gap:24px; align-items:flex-start; flex-wrap:wrap; margin-bottom:20px; }}
h1,h2,h3,p {{ margin-top:0; }}
h1 {{ font-size:27px; margin-bottom:5px; }}
h2 {{ font-size:18px; margin-bottom:10px; }}
h3 {{ font-size:17px; margin-bottom:4px; }}
a {{ color:var(--blue); }}
code {{ background:#e3e1da; padding:1px 4px; border-radius:3px; overflow-wrap:anywhere; }}
.muted {{ color:var(--muted); }}
.controller {{ text-align:right; max-width:620px; }}
.links {{ display:flex; flex-wrap:wrap; gap:8px; margin:10px 0 22px; }}
.links a {{ display:inline-block; padding:7px 11px; border:1px solid var(--line); border-radius:4px; background:var(--panel); font-weight:650; text-decoration:none; }}
.grid {{ display:grid; grid-template-columns:repeat(auto-fit,minmax(290px,1fr)); gap:13px; }}
.card {{ background:color-mix(in srgb,var(--panel) 94%,transparent); border:1px solid var(--line); border-top:5px solid var(--line); border-radius:7px; padding:15px; box-shadow:0 7px 24px rgba(35,43,37,.06); }}
.card.production {{ border-top-color:#216e4b; }}
.card.staging {{ border-top-color:#ad6b14; }}
.card.sunset {{ border-top-color:#667b8b; }}
.title-row {{ display:flex; align-items:flex-start; justify-content:space-between; gap:10px; }}
.role {{ text-transform:uppercase; letter-spacing:.08em; font-size:12px; font-weight:800; color:var(--muted); }}
.tag {{ display:inline-block; border:1px solid var(--line); border-radius:999px; padding:2px 8px; font-size:12px; font-weight:750; }}
.facts {{ display:grid; grid-template-columns:max-content 1fr; gap:5px 10px; margin:13px 0; }}
.facts dt {{ color:var(--muted); }}
.facts dd {{ margin:0; overflow-wrap:anywhere; }}
.services {{ margin-bottom:20px; overflow-x:auto; }}
table {{ width:100%; border-collapse:collapse; background:var(--panel); }}
th,td {{ padding:7px 9px; border-bottom:1px solid var(--line); text-align:left; }}
th {{ color:var(--muted); }}
.ok {{ color:var(--ok); }} .warning {{ color:var(--warn); }} .critical {{ color:var(--bad); }}
.empty {{ padding:20px; border:1px dashed var(--line); background:rgba(255,255,255,.45); }}
@media (max-width:650px) {{ .controller {{ text-align:left; }} main {{ padding:18px 12px 40px; }} }}
</style>
</head>
<body>
<main>
  <header>
    <div><h1>{escape(title)}</h1><div class="muted">One view of production, staging, and supported sunset releases.</div></div>
    <div class="controller">
      <div>Front door: <code>{escape(front_door)}</code></div>
      <div>Controller commit: <code class="commit">{escape(commit_hash)}</code></div>
      <div class="muted">Cycle products: {escape(cycle_products_root)}<br>Live feeds: {escape(live_feed_output_root)}</div>
    </div>
  </header>
  <nav class="links" aria-label="Global monitoring">
    <a href="/pipeline-health/">Pipeline Health</a>
    <a href="/build-watch/">Build Watch</a>
    <a href="/health.json">Deployment Health JSON</a>
  </nav>
  <section class="services">
    <h2>Global Services</h2>
    <div id="services" class="empty">Loading service state...</div>
  </section>
  <section>
    <h2>Release Channels</h2>
    <div id="channels" class="grid"><div class="empty">Loading deployment state...</div></div>
  </section>
</main>
<script>
const esc = (value) => String(value ?? "").replace(/[&<>"']/g, (ch) => ({{'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}}[ch]));
function channelLinks(role, tag) {{
  if (role === "production") return {{ app:"/", packages:"/packages/current_artifacts.json", downloads:"/downloads/", live:"/live-feeds/status.html", pipeline:"/pipeline-health/#production" }};
  if (role === "staging") return {{ app:"/staging/", packages:"/staging/packages/current_artifacts.json", downloads:"/staging/downloads/", live:"/staging/live-feeds/status.html", pipeline:"/pipeline-health/#staging" }};
  const base = `/releases/${{encodeURIComponent(tag)}}`;
  return {{ app:`${{base}}/web/`, packages:`${{base}}/packages/current_artifacts.json`, downloads:`${{base}}/downloads/`, live:`${{base}}/live-feeds/status.html`, pipeline:`/pipeline-health/#release-${{encodeURIComponent(tag)}}` }};
}}
function stateClass(value) {{
  if (["passed","running","active"].includes(value)) return "ok";
  if (["pending","building","qualifying","draining"].includes(value)) return "warning";
  return value ? "critical" : "muted";
}}
function renderServices(services) {{
  const host = document.getElementById("services");
  const entries = Object.entries(services || {{}}).filter(([name]) => !name.startsWith("aerobag-live-feeds-release@"));
  if (!entries.length) {{ host.innerHTML = `<span class="muted">No shared service state is available.</span>`; return; }}
  host.className = "";
  host.innerHTML = `<table><thead><tr><th>Service</th><th>Runtime</th><th>Startup</th></tr></thead><tbody>${{entries.sort(([a],[b]) => a.localeCompare(b)).map(([name, state]) => {{
    const runtime = state?.active ?? (state?.alive === true ? "active" : state?.alive === false ? "failed" : "unknown");
    const startup = state?.enabled ?? (state?.returncode == null ? "managed by stack" : `exit ${{state.returncode}}`);
    return `<tr><td>${{esc(name)}}</td><td class="${{stateClass(runtime)}}">${{esc(runtime)}}</td><td>${{esc(startup)}}</td></tr>`;
  }}).join("")}}</tbody></table>`;
}}
function card(role, tag, record) {{
  const links = channelLinks(role, tag);
  const heading = role === "sunset" ? `Sunset ${{tag}}` : role[0].toUpperCase() + role.slice(1);
  return `<article id="${{role === "sunset" ? `release-${{esc(tag)}}` : role}}" class="card ${{role}}">
    <div class="title-row"><div><div class="role">${{esc(role)}}</div><h3>${{esc(heading)}}</h3></div><span class="tag">${{esc(tag)}}</span></div>
    <dl class="facts">
      <dt>Commit</dt><dd><code>${{esc(record?.commit || "unknown")}}</code></dd>
      <dt>Build</dt><dd class="${{stateClass(record?.build_status)}}">${{esc(record?.build_status || "unknown")}}</dd>
      <dt>Qualification</dt><dd class="${{stateClass(record?.qualification_status)}}">${{esc(record?.qualification_status || "unknown")}}</dd>
      <dt>Live feeds</dt><dd class="${{stateClass(record?.live_feed_status)}}">${{esc(record?.live_feed_status || "unknown")}}</dd>
      ${{record?.last_error ? `<dt>Last error</dt><dd class="critical">${{esc(record.last_error)}}</dd>` : ""}}
    </dl>
    <div class="links"><a href="${{links.app}}">App</a><a href="${{links.packages}}">Artifacts</a><a href="${{links.downloads}}">Downloads</a><a href="${{links.live}}">Live feeds</a><a href="${{links.pipeline}}">Pipeline</a></div>
  </article>`;
}}
async function render() {{
  const host = document.getElementById("channels");
  try {{
    const response = await fetch("/health.json", {{cache:"no-store"}});
    if (!response.ok) throw new Error(`health.json returned ${{response.status}}`);
    const health = await response.json();
    renderServices(health.services);
    const state = health.releases;
    if (!state || typeof state !== "object" || !state.production) {{
      host.innerHTML = `<div class="empty">Standalone deployment; no release-channel state is available.</div>`;
      return;
    }}
    const records = state.releases || {{}};
    const cards = [card("production", state.production, records[state.production])];
    if (state.staging) cards.push(card("staging", state.staging, records[state.staging]));
    for (const tag of state.sunset || []) cards.push(card("sunset", tag, records[tag]));
    host.innerHTML = cards.join("");
  }} catch (error) {{
    host.innerHTML = `<div class="empty critical">Could not load deployment state: ${{esc(error)}}</div>`;
  }}
}}
void render();
</script>
</body>
</html>
"""
