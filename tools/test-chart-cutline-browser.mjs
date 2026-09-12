// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

// Real editor interactions on synthetic charts; never writes curated metadata.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { connectToBrowser, launchChrome, stopProcess, waitFor } from "../ui/web-app/scripts/chrome-cdp.mjs";

const repo = fileURLToPath(new URL("../", import.meta.url));
const artifacts = await mkdtemp(resolve(tmpdir(), "chart-cutline-browser-"));
const profile = await mkdtemp(resolve(tmpdir(), "chart-cutline-chrome-"));
const python = spawn("/usr/bin/python3", ["-u", "-c", `
import json, signal
from http.server import ThreadingHTTPServer
from tools.test_chart_cutlines import CutlineProjectionTest
from tools.chart_cutline_editor import EditorState, EditorCatalog, EditorRequestHandler
fixtures=[]; families={}
for family, dense in [('ENR_H', True), ('SEC', False), ('TAC', False)]:
    fixture=CutlineProjectionTest(); fixture.setUp(); fixtures.append(fixture)
    if dense: fixture.dense_input()
    else:
        fixture.document['features'][0]['geometry']['coordinates']=[fixture.dense_oracle(fixture.points, per_edge=1)]
        fixture.write(fixture.document)
    if family != 'SEC':
        destination=fixture.root/family
        fixture.cutline_dir.rename(destination)
        fixture.cutline_dir=destination
        fixture.path=destination/'Curved.geojson'
    families[family]=EditorState(fixture.root, fixture.cutline_dir, fixture.root/'cache', 600)
    families[family].save_extract('Curved','inset',[{'x':100,'y':200,'width':300,'height':400}],1210,None)
catalog=EditorCatalog(families)
handler=type('TestHandler',(EditorRequestHandler,),{'state':catalog})
server=ThreadingHTTPServer(('127.0.0.1',0),handler)
def stop(*args): raise SystemExit(0)
signal.signal(signal.SIGTERM,stop)
print(json.dumps({'port':server.server_port}),flush=True)
try: server.serve_forever()
finally:
    server.server_close()
    for fixture in fixtures: fixture.doCleanups()
`], {cwd:repo, stdio:["ignore","pipe","pipe"]});
let output = "", stderr = "", chrome, browser, page;
python.stdout.on("data", data => {output += data;});
python.stderr.on("data", data => {stderr += data;});
try {
  const port = await waitFor(() => {
    if (python.exitCode !== null) throw new Error(`fixture exited: ${stderr}`);
    return output.includes("\n") && JSON.parse(output.split("\n")[0]).port;
  }, 5000, "fixture did not start");
  const origin = `http://127.0.0.1:${port}`;
  chrome = await launchChrome({userDataDir:profile, width:1500, height:1000});
  browser = await connectToBrowser(chrome.endpoint);
  page = await browser.createPage();
  await page.send("Page.enable");
  await page.send("Runtime.enable");
  const click = async selector => {
    const point = await page.evaluate(`(() => {
      const e=document.querySelector(${JSON.stringify(selector)});
      if (!e || e.disabled) throw new Error('Unavailable control');
      const r=e.getBoundingClientRect(), x=r.x+r.width/2, y=r.y+r.height/2;
      if (r.width<=0 || r.height<=0 || !e.contains(document.elementFromPoint(x,y))) throw new Error('Obscured control');
      return {x,y};
    })()`);
    await page.send("Input.dispatchMouseEvent",{type:"mousePressed",...point,button:"left",clickCount:1});
    await page.send("Input.dispatchMouseEvent",{type:"mouseReleased",...point,button:"left",clickCount:1});
  };
  const ready = () => waitFor(() => page.evaluate(`document.querySelector('#pointX') &&
    !document.querySelector('#pointX').disabled && document.querySelector('#vertexCount').textContent==='4 points' &&
    document.querySelector('#overviewImage').complete && document.querySelector('#overviewImage').naturalWidth>0`),
  5000, "four-handle editor did not load");
  const outline = () => page.evaluate("document.querySelector('#overviewPolygon').getAttribute('points')");
  for (const family of ["ENR_H", "SEC"]) {
    const url = `${origin}/?family=${family}&chart=Curved`;
    await page.navigate(url);
    await ready();
    const before = await outline();
    if (family === "SEC") assert.ok(before.trim().split(/\s+/).length>40, "curved edge lost its samples");
    await click("#pointX");
    await page.send("Input.dispatchKeyEvent",{type:"keyDown",key:"a",code:"KeyA",modifiers:2,windowsVirtualKeyCode:65});
    await page.send("Input.dispatchKeyEvent",{type:"keyUp",key:"a",code:"KeyA",modifiers:2,windowsVirtualKeyCode:65});
    await page.send("Input.insertText",{text:"115"});
    await click("#pointY");
    await waitFor(() => page.evaluate("!document.querySelector('#saveChart').disabled"),3000,"preview never enabled Save");
    const edited = await outline();
    assert.notEqual(edited,before,"editing did not update the drawn boundary");
    await click("#saveChart");
    await waitFor(() => page.evaluate("document.querySelector('#saveState').textContent==='Saved' && !document.querySelector('#reloadChart').disabled"),3000,"save did not finish");
    await click("#reloadChart");
    await ready();
    assert.equal(await page.evaluate("document.querySelector('#pointX').value"),"115.0");
    const after = (await outline()).trim().split(/[\s,]+/).map(Number);
    const expected = edited.trim().split(/[\s,]+/).map(Number);
    assert.equal(after.length,expected.length);
    assert.ok(after.every((v,i)=>Math.abs(v-expected[i])<1e-6),"reload changed the previewed boundary");
    await writeFile(resolve(artifacts,`${family}.png`),Buffer.from((await page.send("Page.captureScreenshot",{format:"png"})).data,"base64"));
  }
  for (const family of ['ENR_H','TAC']) {
    await page.navigate(`${origin}/extracts?type=navigable-inset&family=${family}&chart=Curved&all=1&mode=boundary`);
    await waitFor(() => page.evaluate(`document.querySelector('#drawRegion') && !document.querySelector('#drawRegion').disabled && document.querySelector('#chartTitle').textContent.includes('Curved') && document.querySelector('#overviewImage').naturalWidth>0`),5000,'manual inset editor did not retain requested family/chart');
    assert.equal(await page.evaluate("document.querySelector('#familySelect').value"),family);
    assert.equal(await page.evaluate("document.querySelector('#familySelect').disabled"),false);
    assert.equal(await page.evaluate("document.querySelector('#insetTargetFamily').options[0].value"),family);
    await click('#drawRegion');
    const box = await page.evaluate("(() => {const r=document.querySelector('#overviewImage').getBoundingClientRect();return {x:r.x,y:r.y,w:r.width,h:r.height};})()");
    for (const [u,v] of [[.25,.25],[.55,.25],[.55,.55],[.25,.55]]) {
      const point={x:box.x+u*box.w,y:box.y+v*box.h};
      await page.send('Input.dispatchMouseEvent',{type:'mousePressed',...point,button:'left',clickCount:1});
      await page.send('Input.dispatchMouseEvent',{type:'mouseReleased',...point,button:'left',clickCount:1});
    }
    await click('#finishOutline');
    await click('#saveLayout');
    await waitFor(() => page.evaluate("document.querySelector('#saveState').textContent==='Saved'"),3000,'new inset draft not saved');
    const layout=await fetch(`${origin}/api/navigable-insets?family=${family}&name=Curved`).then(r=>r.json());
    assert.equal(layout.regions.length,1);
    assert.equal(layout.regions[0].target_family,family);
    assert.equal(layout.regions[0].projection_wkt,layout.new_inset_projection_wkt,
      'a new draft must persist its explicit projection');
    assert.equal(layout.regions[0].enabled,false,'drawing a boundary must not silently publish an uncalibrated region');
    await page.navigate(`${origin}/extracts?type=navigable-inset&family=${family}&chart=Curved&all=1&mode=boundary`);
    await waitFor(() => page.evaluate("document.querySelector('#regionList').children.length===1"),3000,'saved draft did not reload');
    assert.equal(await page.evaluate("document.querySelector('#insetProjection').textContent"),layout.regions[0].projection_wkt,
      'the displayed projection must survive save/reload');
    await writeFile(resolve(artifacts,`${family}-new-inset.png`),Buffer.from((await page.send('Page.captureScreenshot',{format:'png'})).data,'base64'));
  }
  const referenceUrl = `${origin}/extracts?type=inset&family=TAC&chart=Curved`;
  const referenceReady = () => waitFor(() => page.evaluate("document.querySelector('#startGeoreferencing') && !document.querySelector('#startGeoreferencing').disabled && document.querySelector('#startGeoreferencing').getClientRects().length>0 && document.querySelector('#regionCount').textContent==='1 region' && document.querySelector('#extractType').value==='inset'"),3000,'reference conversion control not ready');
  const referenceBefore = await fetch(`${origin}/api/extract?family=TAC&type=inset&name=Curved`).then(r=>r.json());
  await page.navigate(referenceUrl);
  await referenceReady();
  await click('#startGeoreferencing');
  await waitFor(() => page.evaluate("document.querySelector('#extractType').value==='navigable-inset' && document.querySelector('#insetId').value==='Reference 1' && !document.querySelector('#controlPointSection').hidden && document.querySelector('#overviewImage').naturalWidth>0"),5000,'conversion did not open new draft in georef mode');
  assert.equal(await page.evaluate("document.querySelector('#insetEnabled').checked"),false);
  const maps = await fetch(`${origin}/api/navigable-insets?family=TAC&name=Curved`).then(r=>r.json());
  assert.equal(maps.regions.length,2,'existing draft must survive creating another from a reference');
  assert.deepEqual(maps.regions[1].boundary,[[100,200],[400,200],[400,600],[100,600]]);
  assert.equal(maps.regions[1].projection_wkt,maps.new_inset_projection_wkt);
  assert.deepEqual(await fetch(`${origin}/api/extract?family=TAC&type=inset&name=Curved`).then(r=>r.json()),referenceBefore);
  await writeFile(resolve(artifacts,'reference-to-map-draft.png'),Buffer.from((await page.send('Page.captureScreenshot',{format:'png'})).data,'base64'));
  await page.navigate(referenceUrl);
  await referenceReady();
  await click('#startGeoreferencing');
  await waitFor(() => page.evaluate("document.querySelector('#insetId').value==='Reference 1' && !document.querySelector('#controlPointSection').hidden"),3000,'repeat should reopen same draft');
  assert.equal((await fetch(`${origin}/api/navigable-insets?family=TAC&name=Curved`).then(r=>r.json())).regions.length,2);
  assert.equal(page.diagnostics.filter(e=>e.method==="Runtime.exceptionThrown").length,0,JSON.stringify(page.diagnostics));
  console.log(`PASS: sparse/curved cutlines; non-SEC insets; reference-to-map calibration without redrawing or deleting references. Screenshots: ${artifacts}`);
} catch(error) {
  console.error('Editor server exit:', python.exitCode, python.signalCode, '\n', stderr);
  if (page) {
    console.error(await page.evaluate("document.body.innerText"));
    console.error(page.diagnostics);
  }
  throw error;
} finally {
  browser?.close();
  await stopProcess(chrome?.process);
  await stopProcess(python);
  await rm(profile,{recursive:true,force:true});
}
