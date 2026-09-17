#!/usr/bin/env node
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

// Fixture-free production-widget check, not a full app/live-feed journey.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFile, mkdtemp, rm, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { tmpdir } from "node:os";
import { requireWebDependency, webWorkspaceDirectory } from "./web-workspace-require.mjs";
import { launchChrome, connectToBrowser, stopProcess, waitFor } from "./chrome-cdp.mjs";

const root = resolve(process.env.AEROBAG_REPO_ROOT ?? new URL("../../..", import.meta.url).pathname);
const web = resolve(root, "ui/web-app");
const { build } = requireWebDependency("esbuild");
const theme = JSON.parse(await readFile(resolve(root, "ui/shared-fixtures/ui-theme.json"), "utf8"));
const variables = Object.entries(theme.controls).map(([key, value]) => [`--theme-${key.replaceAll("_", "-")}`, value]);
for (const key of ["notam_badge_bg", "notam_badge_fg", "notam_badge_stroke"]) variables.push([`--theme-plate-${key.replaceAll("_", "-")}`, theme.plate_folder[key]]);
const badge = {
  label: "N", count: 20, action_id: "subject_notams:airway:V23", accessibility_label: "V23: 20 NOTAMs",
  detail: { title: "Airway V23 NOTAMs", advisory_text: "Review affected segments; check official sources.", empty_text: "None",
    notams: Array.from({length:20}, (_, i) => ({id:String(i),label:`Notice ${i+1}`,text:`V23 MALAY TO MCKEN MEA 5400 NORTHBOUND. Notice ${i+1}.`})) },
};
const bundle = await build({write:false, bundle:true, format:"iife", jsx:"automatic", nodePaths:[resolve(webWorkspaceDirectory(),"node_modules")],
  stdin:{resolveDir:web, loader:"tsx", contents:`
    import React, {useState} from "react"; import {createRoot} from "react-dom/client";
    import {NotamBadgedControl} from "./src/NotamUi";
    function Fixture() { const [badge,setBadge] = useState(${JSON.stringify(badge)});
      window.cancelNotice = () => setBadge(null);
      return <div className="appShell" style={${JSON.stringify(Object.fromEntries(variables))}}>
        <div className="planTable" style={{width:600}}>
          <NotamBadgedControl active overlayBadge badge={badge}><button className="planWaypointCell planWaypointButton planStructuredWaypointCell" onClick={()=>{window.rowClicks=(window.rowClicks||0)+1}}><span className="planStructuredLabel isFullWidth">V23</span></button></NotamBadgedControl>
          {[1,2,3,4,5].map(i=><div className="planCell" key={i}>12:34</div>)}
        </div>
      </div>;
    } createRoot(document.getElementById("root")).render(<Fixture/>);
  `}});
const css = await readFile(resolve(web,"src/styles.css"));
const server = createServer((req,res) => {
  res.setHeader("Content-Type", req.url === "/app.js" ? "text/javascript" : req.url === "/app.css" ? "text/css" : "text/html");
  res.end(req.url === "/app.js" ? bundle.outputFiles[0].contents : req.url === "/app.css" ? css : '<!doctype html><meta name="viewport" content="width=device-width,initial-scale=1"><link rel="stylesheet" href="/app.css"><div id="root"></div><script src="/app.js"></script>');
});
await new Promise(done => server.listen(0,"127.0.0.1",done));
const profile = await mkdtemp(resolve(tmpdir(),"notam-widget-"));
let chrome, browser;
try {
  chrome = await launchChrome({userDataDir:profile}); browser = await connectToBrowser(chrome.endpoint);
  const page = await browser.createPage(); await page.send("Page.enable"); await page.send("Runtime.enable");
  for (const width of [1280,390]) {
    await page.send("Emulation.setDeviceMetricsOverride", {width,height:800,deviceScaleFactor:1,mobile:false});
    await page.navigate(`http://127.0.0.1:${server.address().port}`); await page.waitForLoad();
    await waitFor(() => page.evaluate('!!document.querySelector("[data-action-id]")'),5000,"badge missing");
    const rowGeometry = await page.evaluate('(()=>{const e=document.querySelector(".notamBadgedControl"), r=e.getBoundingClientRect(), b=e.querySelector(".planWaypointButton").getBoundingClientRect(), n=e.querySelector("[data-action-id]").getBoundingClientRect(), l=e.querySelector(".planStructuredLabel").getBoundingClientRect();return {width:r.width,buttonWidth:b.width,labelRight:l.right,badgeLeft:n.left}})()');
    assert(Math.abs(rowGeometry.width-rowGeometry.buttonWidth)<1 && rowGeometry.labelRight<=rowGeometry.badgeLeft, JSON.stringify(rowGeometry));
    const box = await page.evaluate('(()=>{const r=document.querySelector("[data-action-id]").getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2}})()');
    await page.send("Input.dispatchMouseEvent", {type:"mousePressed",...box,button:"left",clickCount:1});
    await page.send("Input.dispatchMouseEvent", {type:"mouseReleased",...box,button:"left",clickCount:1});
    await waitFor(() => page.evaluate('!!document.querySelector(".notamReaderOverlay")'),5000,"reader did not open");
    assert.equal(await page.evaluate('window.rowClicks || 0'),0);
    const geometry = await page.evaluate('(()=>{const e=document.querySelector(".procedureNotamDetailModal"),r=e.getBoundingClientRect();return {left:r.left,right:r.right,top:r.top,bottom:r.bottom,scrollable:e.scrollHeight>e.clientHeight}})()');
    assert(geometry.left>=0 && geometry.right<=width && geometry.top>=0 && geometry.bottom<=800 && geometry.scrollable, JSON.stringify(geometry));
    await page.send("Input.dispatchMouseEvent", {type:"mouseWheel", x:width/2,y:400,deltaX:0,deltaY:10000});
    await waitFor(() => page.evaluate('document.querySelector(".procedureNotamDetailModal").scrollTop>0'),5000,"reader cannot scroll");
    const shot = await page.send("Page.captureScreenshot", {format:"png"});
    await writeFile(resolve(tmpdir(),`notam-reader-${width}.png`),Buffer.from(shot.data,"base64"));
    await page.evaluate('window.cancelNotice()');
    await waitFor(() => page.evaluate('!document.querySelector(".notamReaderOverlay") && !document.querySelector("[data-action-id]")'),5000,"cancelled notice remains visible");
  }
  console.log("NOTAM widget smoke passed: physical click, scroll, cancellation; desktop and mobile.");
} finally {
  await browser?.close(); await stopProcess(chrome?.process); server.close();
  await rm(profile,{recursive:true,force:true,maxRetries:5,retryDelay:100});
}
