// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

import {SheetPrefetchQueue, cacheImageBytes, sheetsAhead} from "./prefetch.mjs";

const $ = id => document.getElementById(id);
const token = document.querySelector('meta[name="review-token"]').content;
const statusText = {pending: "Not reviewed", approved: "Approved", disapproved: "Disapproved",
  stale: "Changed: review again", recheck: "Corrected: review again", inventory: "Inventory only",
  complete: "All inset regions accounted for", incomplete: "Missing / unresolved regions"};
let state, selected, selectedRegion, busy = false, mode = "all";
let pendingCursor, cursorSending = false;
let scrollFrame = 0;
const nodes = new Map();
const svgNS = "http://www.w3.org/2000/svg";
let prefetchTargets = [];
const prefetch = new SheetPrefetchQueue({
  load: async current => {
    const data = await api(current.overview_url);
    if (data.digest !== current.digest) throw new Error("Sheet revision changed; reload the inventory.");
    await cacheImageBytes(data.image);
    return data;
  },
  changed: updatePrefetch,
});

function message(text, error = false) {
  $("message").textContent = text;
  $("message").classList.toggle("error", error);
}

async function api(path, body) {
  const response = await fetch(path, body === undefined ? {} : {
    method: "POST", headers: {"Content-Type": "application/json", "X-Review-Token": token}, body: JSON.stringify(body),
  });
  const result = await response.json();
  if (!response.ok) throw new Error(result.error || `Request failed (${response.status})`);
  return result;
}

function visible() {
  return state.sheets.filter(s => mode === "all" || (mode === "rejected" ? s.has_rejection : s.needs_review));
}
const sheet = id => state.sheets.find(s => s.id === id);
const item = id => state.items.find(i => i.id === id);
const ready = image => image?.complete && image.naturalWidth > 0;

async function saveCursor() {
  if (!selected) return;
  pendingCursor = {id: selected, region_id: selectedRegion};
  if (cursorSending) return;
  cursorSending = true;
  try {
    while (pendingCursor) {
      const cursor = pendingCursor; pendingCursor = null;
      await api("/api/sheet-cursor", cursor);
    }
  } catch (error) { message(`Could not save position: ${error.message}`, true); }
  finally { cursorSending = false; }
}

function controls() {
  const current = item(selectedRegion), rows = state ? visible() : [];
  const canReview = !busy && current && rows.some(s => s.id === selected) && ready(nodes.get(selected)?.preview);
  $("approve").disabled = !canReview || current.status === "stale";
  $("reject").disabled = !canReview;
  $("skip").disabled = busy || !selected;
  $("previous").disabled = busy || !selected || rows.findIndex(s => s.id === selected) === 0;
  $("import").disabled = busy;
  $("note").disabled = busy || !current;
  $("position").textContent = selected ? `Sheet ${state.sheets.findIndex(s => s.id === selected) + 1} / ${state.sheets.length}` : "";
  $("active-region").textContent = selected ? `Region: ${sheet(selected)?.regions.find(r => r.id === selectedRegion)?.name || "none"}` : "";
  for (const [id, n] of nodes) {
    for (const button of n.card.querySelectorAll("button")) button.disabled = busy;
    n.refresh.disabled = busy || !item(n.region)?.can_refresh;
    const rendered = n.geometry?.digest === sheet(id).digest && ready(n.sourceImage);
    n.card.querySelector(".accounted").disabled = busy || !rendered;
    n.card.querySelector(".missing").disabled = busy || !rendered;
  }
}

function select(id, regionId = null, scroll = false, persist = true) {
  const current = sheet(id), node = nodes.get(id);
  selected = current?.id || null;
  selectedRegion = current?.regions.find(r => r.id === regionId)?.id || node?.region || current?.regions[0]?.id || null;
  for (const [key, n] of nodes) {
    n.card.classList.toggle("selected", key === id);
    n.jump.classList.toggle("selected", key === id);
    if (key !== id && !n.onScreen) releaseOverview(n);
  }
  if (current) {
    selectRegion(node, selectedRegion);
    if (scroll) node.card.scrollIntoView({block: "start"});
    showOverview(node);
  }
  $("note").value = item(selectedRegion)?.decision?.note || "";
  $("complete").hidden = Boolean(id);
  $("completion-text").textContent = "End of this source-sheet pass. Region approvals and sheet completeness decisions are saved separately.";
  if (!id && scroll) $("complete").scrollIntoView({block: "start"});
  controls();
  schedulePrefetch();
  if (persist) void saveCursor();
}

function selectRegion(node, id) {
  node.region = id;
  const region = sheet(node.id).regions.find(r => r.id === id), candidate = item(id);
  for (const button of node.regions.querySelectorAll("button")) button.classList.toggle("selected", button.dataset.region === id);
  for (const g of node.overview.querySelectorAll("g[data-region]")) g.classList.toggle("selected", g.dataset.region === id);
  node.detail.querySelector("h3").textContent = region?.name || "";
  node.detail.querySelector(".stored-note").textContent = candidate?.decision?.note || "";
  node.detail.querySelector(".caption").textContent = candidate?.image_caption || region?.review_note || "";
  node.preview.hidden = !candidate;
  const url = candidate && (node.detail.querySelector(".outline").checked ? candidate.image : candidate.plain_image);
  if (url && node.preview.getAttribute("src") !== url) node.preview.src = url;
  if (!url) node.preview.removeAttribute("src");
  node.preview.alt = `${region?.name}: pinned review candidate`;
  const link = node.detail.querySelector(".detail-link");
  link.hidden = !candidate; link.href = candidate?.detail_url || "#";
  const editor = node.detail.querySelector(".editor");
  editor.hidden = !region?.editor_url; editor.href = region?.editor_url || "#";
  node.refresh.hidden = !candidate;
  node.detail.querySelector(".outline-label").hidden = !candidate;
}

function svgElement(name, attributes = {}) {
  const element = document.createElementNS(svgNS, name);
  for (const [key, value] of Object.entries(attributes)) element.setAttribute(key, value);
  return element;
}

function drawOverview(node, data) {
  const current = sheet(node.id);
  const svg = svgElement("svg", {viewBox: `0 0 ${data.width} ${data.height}`, "aria-label": `${current.chart}: all source regions`});
  svg.style.aspectRatio = `${data.width}/${data.height}`;
  svg.append(svgElement("image", {href:data.image, width:data.width, height:data.height}));
  // The PNG is the readiness barrier: a blank source cannot be marked complete.
  node.sourceImage = new Image(); node.sourceImage.onload = controls;
  node.sourceImage.onerror = () => { node.overviewError.textContent = "Source image failed to load. Reload to retry."; controls(); };
  node.sourceImage.src = data.image;
  const labels = [], marker = Math.max(data.width, data.height) / 100;
  for (const projected of data.regions) {
    const region = current.regions.find(r => r.id === projected.id);
    const g = svgElement("g", {"data-region":region.id, class:region.id === node.region ? "selected" : ""});
    const title = svgElement("title"); title.textContent = `${projected.number}. ${region.name}. ${projected.exclusion}`; g.append(title);
    const d = projected.rings.map(polygon => polygon.map(ring => ring.map(([x,y], i) => `${i ? "L" : "M"}${x},${y}`).join(" ") + "Z").join(" ")).join(" ");
    const path = svgElement("path", {d, fill:"none", stroke:region.color, "vector-effect":"non-scaling-stroke", "stroke-width":2});
    if (!region.enabled) path.setAttribute("stroke-dasharray", "5 5");
    g.append(path);
    const points = projected.rings.flat(2);
    if (points.length) {
      const [x,y] = points.reduce((best, point) => point[1] < best[1] ? point : best);
      const label = svgElement("g", {transform:`translate(${Math.max(marker,Math.min(data.width-marker,x))},${Math.max(marker,Math.min(data.height-marker,y))})`});
      label.append(svgElement("circle", {r:marker, fill:"#16212b", stroke:region.color, "stroke-width":2, "vector-effect":"non-scaling-stroke"}));
      const text = svgElement("text", {"text-anchor":"middle", dy:".35em", fill:region.color, "font-size":marker*1.35});
      text.textContent = projected.number; label.append(text);
      label.onclick = () => { if (!busy) select(node.id, region.id); }; labels.push(label);
    }
    g.onclick = () => { if (!busy) select(node.id, region.id); }; svg.append(g);
    const button = [...node.regions.querySelectorAll("button")].find(b => b.dataset.region === region.id);
    button.querySelector(".exclusion").textContent = projected.exclusion;
  }
  svg.append(...labels); node.overview.replaceChildren(svg);
  node.drawnDigest = data.digest;
  node.overviewError.textContent = ""; controls();
}

function showOverview(node) {
  const current = sheet(node.id);
  if (!current || node.drawnDigest === current.digest) return;
  if (current.source_error) { node.overviewError.textContent = current.source_error; return; }
  const entry = prefetch.get(current.overview_url);
  if (entry?.status === "ready") {
    node.geometry = entry.value;
    drawOverview(node, entry.value);
  } else {
    node.overviewError.textContent = entry?.status === "error" ? entry.error.message : "Loading whole source sheet...";
  }
}

function releaseOverview(node) {
  node.overview.replaceChildren();
  if (node.sourceImage) node.sourceImage.removeAttribute("src");
  node.sourceImage = null; node.drawnDigest = null;
}

function schedulePrefetch() {
  if (!state) return;
  const background = sheetsAhead(visible(), selected, $("prefetch").value);
  const requests = [];
  const add = (current, priority) => {
    if (current && !current.source_error) requests.push({key:current.overview_url, value:current, priority});
  };
  add(sheet(selected),0);
  for (const [id, n] of nodes) if (n.onScreen) add(sheet(id),1);
  for (const current of background) add(current,2);
  prefetchTargets = [...new Set(requests.map(r => r.key))];
  prefetch.setRequests(requests);
}

function updatePrefetch() {
  if (!state) return;
  for (const [id, node] of nodes) if (node.onScreen || id === selected) showOverview(node);
  const entries = prefetchTargets.map(key => prefetch.get(key));
  const cached = entries.filter(e => e?.status === "ready").length;
  const failed = entries.filter(e => e?.status === "error").length;
  const loading = entries.filter(e => e?.status === "loading").length;
  const status = $("prefetch-status");
  status.textContent = `Buffer: ${cached}/${entries.length} sheets cached${loading ? `; ${loading} loading` : ""}${failed ? `; ${failed} failed` : ""}`;
  status.dataset.cached = cached; status.dataset.total = entries.length; status.dataset.failed = failed;
  $("prefetch-retry").hidden = failed === 0;
  controls();
}

const observer = new IntersectionObserver(entries => {
  for (const entry of entries) {
    const node = nodes.get(entry.target.dataset.id);
    if (!node) continue;
    node.onScreen = entry.isIntersecting;
    if (node.onScreen) showOverview(node);
    else if (node.id !== selected) releaseOverview(node);
  }
  schedulePrefetch();
}, {root:$("cards"), rootMargin:"100px"});

function createCard(current) {
  const card = document.createElement("article"); card.dataset.id = current.id;
  card.innerHTML = '<div class="card-head"><h2></h2><span class="status"></span></div><p class="caption">Whole current source sheet, including outside the main cutline. Numbers identify every defined region; unoutlined features still need your inspection.</p><p class="overview-error"></p><div class="sheet-overview"></div><div class="sheet-decision"><strong>Sheet completeness (not a cutline approval)</strong><input class="sheet-note" maxlength="4000" aria-label="Source sheet note" placeholder="Missing inset, duplicate, or other issue to follow up"><div class="controls"><button class="accounted">All inset regions accounted for</button><button class="missing">Flag missing / unresolved regions</button></div></div><div class="region-layout"><div class="regions" aria-label="All regions on source sheet"></div><section class="region-detail"><h3></h3><p class="caption"></p><img class="chart-image" loading="lazy"><p class="stored-note"></p><div class="links"><a class="detail-link" target="_blank">Comparison / control patches</a><a class="editor" target="_blank">Edit this region</a><button class="refresh">Refresh after edit</button><label class="outline-label"><input type="checkbox" class="outline" checked> Show cutline</label></div></section></div>';
  const jump = document.createElement("button"); jump.className = "jump";
  jump.innerHTML = '<span></span><div class="review-summary"></div>';
  const node = {id:current.id, card, jump, region:null, geometry:null,
    onScreen:false, drawnDigest:null,
    overview:card.querySelector(".sheet-overview"), overviewError:card.querySelector(".overview-error"),
    regions:card.querySelector(".regions"), detail:card.querySelector(".region-detail"),
    preview:card.querySelector(".chart-image"), refresh:card.querySelector(".refresh")};
  node.preview.onload = controls; node.preview.onerror = controls;
  node.preview.onclick = () => node.detail.classList.toggle("expanded");
  node.detail.querySelector(".outline").onchange = () => { selectRegion(node, node.region); controls(); };
  node.refresh.onclick = () => refreshItem(node);
  const workflow = document.createElement('div'); workflow.className = 'sheet-work';
  workflow.innerHTML = '<p class="work-note"></p><details class="add-region"><summary>Add region</summary><div class="links"></div><p>FAA detail entries below already have georeferences. Edit their cutlines instead of adding manual duplicates.</p></details><button class="refresh-sheet">Reload regions and render new candidates</button>';
  card.querySelector('.sheet-decision').before(workflow);
  workflow.querySelector('.refresh-sheet').onclick = () => refreshSheet(node);
  jump.onclick = () => { if (!busy) select(current.id, null, true); };
  card.onclick = event => { if (!busy && selected !== node.id && !event.target.closest("button,a,input,svg")) select(node.id); };
  card.querySelector(".accounted").onclick = () => decideSheet(node, "complete");
  card.querySelector(".missing").onclick = () => decideSheet(node, "incomplete");
  $("cards").insertBefore(card, $("complete")); $("jump-list").append(jump);
  observer.observe(card); return node;
}

function renderReviewSummary(element, summary) {
  element.replaceChildren(...summary.map(line => {
    const label = document.createElement("small");
    label.className = line.tone; label.textContent = line.label;
    return label;
  }));
}

function render() {
  const shown = new Set(visible().map(s => s.id)), c = state.counts, sc = state.sheet_counts;
  for (const [id, n] of nodes) if (!sheet(id)) {
    observer.unobserve(n.card); n.card.remove(); n.jump.remove(); nodes.delete(id);
  }
  $("counts").textContent = `${state.sheets.length} sheets / ${sc.complete} accounted for / ${c.approved} regions approved / ${c.disapproved} disapproved`;
  for (const current of state.sheets) {
    if (!nodes.has(current.id)) nodes.set(current.id, createCard(current));
    const n = nodes.get(current.id);
    n.card.hidden = n.jump.hidden = !shown.has(current.id);
    n.card.querySelector("h2").textContent = `${current.family} / ${current.chart}`;
    const label = n.card.querySelector(".status");
    renderReviewSummary(label, current.review_summary);
    n.jump.querySelector("span").textContent = `${current.family} / ${current.chart}`;
    // Use the same server-owned summary in the index and card header.
    renderReviewSummary(n.jump.querySelector(".review-summary"), current.review_summary);
    const note = n.card.querySelector(".sheet-note");
    if (document.activeElement !== note) note.value = current.decision?.note || "";
    n.card.querySelector('.work-note').textContent = current.work_note;
    n.card.querySelector('.add-region .links').replaceChildren(...current.add_region_actions.map(action => {
      const link = document.createElement('a'); link.textContent = action.label;
      link.href = action.url; link.target = '_blank'; return link;
    }));
    n.card.querySelector('.add-region').hidden = current.add_region_actions.length === 0;
    n.regions.replaceChildren(...current.regions.map(region => {
      const button = document.createElement("button"); button.dataset.region = region.id;
      button.style.setProperty("--region-color", region.color);
      button.innerHTML = '<strong></strong><small class="region-status"></small><small class="exclusion"></small>';
      button.querySelector("strong").textContent = `${region.number}. ${region.name}`;
      button.querySelector(".region-status").textContent = statusText[region.status];
      button.querySelector(".region-status").className = `region-status ${region.status}`;
      button.querySelector(".exclusion").textContent = n.geometry?.digest === current.digest
        ? n.geometry.regions.find(r => r.id === region.id)?.exclusion || "" : "Parent exclusion: inspecting geometry...";
      button.onclick = () => { if (!busy) select(current.id, region.id); }; return button;
    }));
    if (!current.regions.some(r => r.id === n.region)) n.region = current.regions[0]?.id;
    selectRegion(n, n.region);
    if (!n.card.hidden && n.geometry && n.geometry.digest !== current.digest) {
      n.geometry = null; releaseOverview(n);
    }
  }
  controls();
}

async function decide(verdict) {
  if (busy || $(verdict === "approved" ? "approve" : "reject").disabled) return;
  const candidate = item(selectedRegion), rows = visible(), next = rows[rows.findIndex(s => s.id === selected)+1]?.id;
  busy = true; controls(); message("Saving region decision...");
  try {
    state = await api("/api/decision", {id:candidate.id, digest:candidate.digest, verdict, note:$("note").value});
    render(); select(next || null, null, true);
    message(`${candidate.chart} / ${candidate.name}: ${statusText[verdict]}. Sheet completeness was not changed.`);
  } catch (error) { message(error.message, true); }
  finally { busy = false; controls(); }
}

async function decideSheet(node, verdict) {
  if (busy) return;
  const current = sheet(node.id), rows = visible(), next = rows[rows.findIndex(s => s.id === node.id)+1]?.id;
  busy = true; controls();
  try {
    state = await api("/api/sheet-decision", {id:current.id, digest:current.digest, verdict, note:node.card.querySelector(".sheet-note").value});
    render();
    select(visible().some(s => s.id === node.id) ? node.id : next || null, node.region, true);
    message("Sheet decision saved. Individual region approvals were not changed.");
  } catch (error) { message(error.message, true); }
  finally { busy = false; controls(); }
}

function move(offset) {
  if (busy) return;
  const rows = visible(), index = selected ? rows.findIndex(s => s.id === selected) : rows.length;
  select(rows[index + offset]?.id || null, null, true); message("Moved without changing any decision.");
}
function filter(value) {
  mode = value; $("filter").value = $("mobile-filter").value = value;
  render(); select(visible()[0]?.id || null, null, true);
}
async function refreshItem(node) {
  if (busy) return;
  const candidate = item(node.region); busy = true; controls(); message("Rendering current metadata...");
  try {
    state = await api("/api/refresh", {id:candidate.id, digest:candidate.digest});
    render(); select(node.id, candidate.id); message("Fresh candidate ready. Inspect it before approving.");
  } catch (error) { message(error.message, true); }
  finally { busy = false; controls(); }
}
async function refreshSheet(node) {
  if (busy) return;
  busy = true; controls(); message('Loading new regions and rendering review candidates...');
  try {
    state = await api('/api/state');
    const current = sheet(node.id);
    state = await api('/api/refresh-sheet', {id:current.id, digest:current.digest});
    render(); select(node.id, node.region);
    message('Regions refreshed. No approvals or sheet decisions were changed.');
  } catch (error) { message(error.message, true); }
  finally { busy = false; controls(); }
}
$("approve").onclick = () => decide("approved"); $("reject").onclick = () => decide("disapproved");
$("skip").onclick = () => move(1); $("previous").onclick = () => move(-1);
$("filter").onchange = event => filter(event.target.value); $("mobile-filter").onchange = event => filter(event.target.value);
$("show-rejected").onclick = () => filter("rejected"); $("show-all").onclick = () => filter("all");
$("prefetch").onchange = schedulePrefetch;
$("prefetch-retry").onclick = () => prefetch.retryFailed();
$("import").onclick = async () => {
  busy = true; controls();
  try { state = await api("/api/import", {}); render(); select(selected, selectedRegion); message("Latest reports and metadata loaded; decisions retained."); }
  catch (error) { message(error.message, true); }
  finally { busy = false; controls(); }
};
$("cards").addEventListener("scroll", () => {
  if (busy || scrollFrame || !selected) return;
  scrollFrame = requestAnimationFrame(() => {
    scrollFrame = 0;
    const anchor = $("cards").getBoundingClientRect().top + 40;
    const current = visible().find(s => {
      const bounds = nodes.get(s.id).card.getBoundingClientRect();
      return bounds.top <= anchor && bounds.bottom > anchor;
    });
    if (current && current.id !== selected) select(current.id);
  });
});
document.addEventListener("keydown", event => {
  if (event.repeat || event.ctrlKey || event.metaKey || event.altKey || event.target.closest("input,textarea,select")) return;
  const actions = {n: () => move(1), d: () => decide("disapproved"), p: () => move(-1), s: () => move(1)};
  if (actions[event.key.toLowerCase()]) { event.preventDefault(); actions[event.key.toLowerCase()](); }
});
try {
  state = await api("/api/state"); render();
  select(visible().find(s => s.id === state.sheet_cursor)?.id || visible()[0]?.id || null, state.cursor, true, false);
} catch (error) { message(error.message, true); }
