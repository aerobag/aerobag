// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

// A disposable browser journey: no approvals or rejections touch real metadata.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { connectToBrowser, launchChrome, stopProcess, waitFor } from "../ui/web-app/scripts/chrome-cdp.mjs";

const repo = fileURLToPath(new URL("../", import.meta.url));
const artifacts = process.argv[2] || await mkdtemp(resolve(tmpdir(), "chart-review-browser-"));
const profile = await mkdtemp(resolve(tmpdir(), "chart-review-chrome-"));
const python = spawn("/usr/bin/python3", ["-u", "-c", `
import json, signal, shutil
from tools.test_chart_visual_references import ChartFixture
from tools.chart_visual_review import ReviewStore, make_server
fixture=ChartFixture(); fixture.setUp()
shutil.copy(fixture.sources/'Test SEC.tif', fixture.sources/'Other SEC.tif')
shutil.copy(fixture.cutline, fixture.metadata/'SEC/Other SEC.geojson')
layout={'schema_version':1,'source':'Test SEC.tif','source_width':1000,'source_height':800,
        'regions':[{'x':120,'y':120,'width':40,'height':40}]}
(fixture.metadata/'SEC/Test SEC.inset.json').write_text(json.dumps(layout))
fixture.check()
store=ReviewStore(fixture.output.parent, fixture.root/'review', fixture.metadata, 'browser test', {'SEC':fixture.sources})
server=make_server(store, ('127.0.0.1',0))
original_get=server.RequestHandlerClass.do_GET
image_gate={'blocked':False,'blocked_requests':0}
def get_with_image_gate(handler):
 if handler.path.startswith('/test/image-gate/'):
  image_gate['blocked']=handler.path.endswith('/block')
  return handler.send(image_gate)
 if image_gate['blocked'] and handler.path.startswith('/sheets/'):
  image_gate['blocked_requests']+=1
  return handler.send({'error':'uncached image request after prefetch'},status=503)
 return original_get(handler)
server.RequestHandlerClass.do_GET=get_with_image_gate
def stop(*args): raise SystemExit(0)
signal.signal(signal.SIGTERM, stop)
print(json.dumps({'port':server.server_port}),flush=True)
try: server.serve_forever()
finally: server.server_close(); fixture.doCleanups()
`], {cwd:repo, stdio:["ignore","pipe","pipe"]});
let output = "", stderr = "", chrome, browser, page;
python.stdout.on("data", data => {output += data;});
python.stderr.on("data", data => {stderr += data;});
try {
  const port = await waitFor(() => {
    if (python.exitCode !== null) throw new Error(`fixture exited: ${stderr}`);
    if (!output.includes("\n")) return false;
    return JSON.parse(output.split("\n")[0]).port;
  }, 5000, "fixture server did not start");
  const origin = `http://127.0.0.1:${port}`;
  chrome = await launchChrome({userDataDir:profile, width:1300, height:1000});
  browser = await connectToBrowser(chrome.endpoint);
  page = await browser.createPage();
  await page.send("Page.enable");
  await page.send("Runtime.enable");
  await page.navigate(origin);
  const ready = () => waitFor(() => page.evaluate('document.querySelector("#approve") && !document.querySelector("#approve").disabled'), 3000, "visible candidate did not become approvable");
  const click = async selector => {
    await waitFor(() => page.evaluate(`(() => { const e=document.querySelector(${JSON.stringify(selector)}); return e && !e.disabled && e.getClientRects().length > 0; })()`),
      3000, `Control did not become visible and enabled: ${selector}`);
    const point = await page.evaluate(`(() => {
      const element = document.querySelector(${JSON.stringify(selector)});
      if (!element || element.disabled) throw new Error('Unavailable control');
      element.scrollIntoView({block:'center'});
      const r = element.getBoundingClientRect(), x = r.x+r.width/2, y = r.y+r.height/2;
      if (r.width <= 0 || r.height <= 0 || !element.contains(document.elementFromPoint(x,y))) throw new Error('Control is obscured');
      return {x,y};
    })()`);
    await page.send("Input.dispatchMouseEvent", {type:"mousePressed",...point,button:"left",clickCount:1});
    await page.send("Input.dispatchMouseEvent", {type:"mouseReleased",...point,button:"left",clickCount:1});
  };
  const serverState = () => fetch(`${origin}/api/state`).then(r => r.json());
  const indexText = chart => page.evaluate(`([...document.querySelectorAll('.jump')].find(b => b.querySelector('span').textContent === ${JSON.stringify(chart)}))?.innerText || ''`);
  await ready();
  await waitFor(() => page.evaluate('document.querySelector("article.selected .accounted") && !document.querySelector("article.selected .accounted").disabled'),3000,"whole sheet did not render");
  await writeFile(resolve(artifacts,"first-candidate.png"),Buffer.from((await page.send("Page.captureScreenshot",{format:"png"})).data,"base64"));
  const first = await page.evaluate('document.querySelector("article.selected").dataset.id');
  await waitFor(() => page.evaluate('document.querySelector("#prefetch-status").dataset.cached === "2"'),
    3000,"next sheet was not prefetched while reviewing the first");
  assert.equal(await page.evaluate('document.querySelectorAll("article .sheet-overview svg").length'),1,
    "prefetch must not render/decode every offscreen sheet");
  // Make any new whole-sheet image download fail. Next must still paint using
  // the bytes fetched in advance, rather than merely prefetching its geometry.
  await fetch(`${origin}/test/image-gate/block`);
  await click("#skip");
  await waitFor(() => page.evaluate('document.querySelector("article.selected .accounted") && !document.querySelector("article.selected .accounted").disabled'),
    3000,"prefetched sheet could not paint without downloading its image again");
  const gate = await fetch(`${origin}/test/image-gate/allow`).then(r => r.json());
  assert.equal(gate.blocked_requests,0,"HTTP cache was bypassed after prefetch");
  assert.equal((await serverState()).counts.approved,0,"prefetch/navigation must not approve anything");
  await click("#previous");
  await ready();
  await click("article.selected .accounted");
  await waitFor(async () => (await serverState()).sheet_counts.complete === 1,3000,"sheet completeness not persisted");
  assert.equal((await serverState()).counts.approved,0,"sheet completeness must not approve any region");
  await click("#approve");
  await waitFor(async () => (await serverState()).counts.approved === 1,3000,"approval was not persisted");
  await ready();
  assert.notEqual(await page.evaluate('document.querySelector("article.selected").dataset.id'),first);
  assert.match(await indexText("SEC / Other SEC"), /Regions: Approved \(1\/1\)/,
    "a successful approval must immediately update the source-sheet index");
  assert.match(await indexText("SEC / Other SEC"), /Inset inventory: All accounted for/);
  await waitFor(() => page.evaluate('document.querySelectorAll("article.selected svg g[data-region]").length === 3'),3000,"not all regions are overlaid");
  const second = (await serverState()).sheets.find(s => s.id !== first);
  assert.equal(second.regions.length,3,"main, navigable and reference must share a source card");
  const inset = second.regions.find(r => r.kind === "inset"), reference = second.regions.find(r => r.kind === "reference");
  await click(`article.selected .regions button[data-region="${reference.id}"]`);
  assert.ok(await page.evaluate('document.querySelector("#approve").disabled'),"reference extraction has no approvable candidate");
  assert.match(await page.evaluate(`document.querySelector('article.selected .regions button[data-region="${reference.id}"] .exclusion').textContent`),/100% overlaps/);
  await click(`article.selected .regions button[data-region="${inset.id}"]`);
  await ready();
  await click("article.selected .sheet-note");
  await page.send("Input.insertText",{text:"Unoutlined inset in southeast corner"});
  await click("article.selected .missing");
  await waitFor(async () => (await serverState()).sheet_counts.incomplete === 1,3000,"sheet flag not persisted");
  await click("#note");
  await page.send("Input.insertText",{text:"North edge needs trimming"});
  await click("#reject");
  await waitFor(async () => (await serverState()).counts.disapproved === 1,3000,"rejection was not persisted");
  await waitFor(() => page.evaluate('!document.querySelector("#complete").hidden'),3000,"pass did not complete");
  assert.match(await indexText("SEC / Test SEC"), /1 disapproved/);
  await page.navigate(origin);
  await ready();
  assert.equal(await page.evaluate('document.querySelector("article.selected").dataset.id'),second.id,"reload must preserve source cursor");
  assert.equal(await page.evaluate('document.querySelector("#note").value'),"North edge needs trimming");
  assert.equal(await page.evaluate('document.querySelector("article.selected .sheet-note").value'),"Unoutlined inset in southeast corner");
  await writeFile(resolve(artifacts,"rejection-after-reload.png"),Buffer.from((await page.send("Page.captureScreenshot",{format:"png"})).data,"base64"));
  await click("#approve");
  await waitFor(async () => (await serverState()).counts.approved === 2,3000,"second-pass approval failed");
  await waitFor(async () => /Regions: 1\/2 approved; 1 not reviewed/.test(await indexText("SEC / Test SEC")),
    3000,"partial region approval did not update the index, or counted inventory-only extracts");
  assert.match(await indexText("SEC / Test SEC"), /Inset inventory: Missing \/ unresolved regions/);
  assert.deepEqual(await fetch(`${origin}/api/rejections`).then(r => r.json()),[]);
  assert.equal((await serverState()).sheet_counts.incomplete,1,"region approval cannot clear a missing-inset flag");
  // Exercise the narrow layout with real rendered controls, not DOM .click().
  await page.send("Emulation.setDeviceMetricsOverride",{width:480,height:1000,deviceScaleFactor:1,mobile:false});
  await click("#show-all");
  await ready();
  assert.ok(await page.evaluate('document.documentElement.scrollWidth <= innerWidth'),"horizontal overflow");
  await writeFile(resolve(artifacts,"narrow.png"),Buffer.from((await page.send("Page.captureScreenshot",{format:"png"})).data,"base64"));
  assert.equal(page.diagnostics.filter(entry => entry.method === "Runtime.exceptionThrown").length,0,JSON.stringify(page.diagnostics));
  console.log(`PASS: bounded byte-cache prefetch paints without network images; grouping, overlays, independent decisions, reload and narrow layout. Screenshots: ${artifacts}`);
} catch (error) {
  if (page) {
    console.error(await page.evaluate('document.body.innerText'));
    console.error(page.diagnostics);
    await writeFile(resolve(artifacts,"failure.png"),Buffer.from((await page.send("Page.captureScreenshot",{format:"png"})).data,"base64"));
  }
  throw error;
} finally {
  browser?.close();
  await stopProcess(chrome?.process);
  await stopProcess(python);
  await rm(profile,{recursive:true,force:true});
}
