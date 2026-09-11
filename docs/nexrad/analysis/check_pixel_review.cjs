// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

const {chromium} = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const url = process.argv[2];
const root = process.argv[3];
if (!url || !root) throw new Error('usage: node check_pixel_review.cjs URL SCREENSHOT_DIR');
fs.mkdirSync(root,{recursive:true});

(async()=>{
  const browser = await chromium.launch({executablePath:'/usr/bin/google-chrome-stable',headless:true,args:['--no-sandbox','--disable-dev-shm-usage']});
  const errors = [];
  const deadline = setTimeout(()=>{console.error('Browser check exceeded 90 seconds');browser.close();process.exitCode=1;},90000);
  try {
    const page = await browser.newPage({viewport:{width:1360,height:1000}});
    page.on('pageerror',error=>errors.push(error.message));
    page.on('response',response=>{if(response.status()>=400)errors.push(`${response.status()} ${response.url()}`);});
    await page.goto(url);
    await page.waitForSelector('html[data-ready="true"]');
    const review = await page.evaluate(async()=>await (await fetch('review.json')).json());
    assert.equal(await page.locator('.frame').count(),review.recovered);
    assert.equal(await page.locator('#unavailable tbody tr').count(),review.total-review.recovered);
    const ordered = review.frames.filter(f=>f.verified).sort((a,b)=>b.reconstructed_max_error-a.reconstructed_max_error||a.state_id.localeCompare(b.state_id));
    const firstLevel = ordered[0].levels.find(l=>l.flagged_pixels.length);
    const pixel = firstLevel.flagged_pixels[0];
    await page.evaluate(async()=>{await Promise.all([...document.images].map(image=>{image.loading='eager';return image.decode();}));});
    await page.screenshot({path:path.join(root,'desktop.png')});
    await page.locator('input[name="mode"][value="blink"]').check();
    await page.waitForFunction(()=>document.body.dataset.phase==='original');
    await page.waitForFunction(()=>document.body.dataset.phase==='compressed');
    const blink = await page.locator('.single-pane .phase-label').first().textContent();
    assert.equal(blink,'Aerobag compressed');
    await page.locator('input[name="mode"][value="pair"]').check();
    await page.locator('.pixel-table button').first().click();
    await page.waitForFunction(()=>document.querySelector('#canvasStatus').textContent==='');
    await page.selectOption('#viewerMode','original');
    const sample = ()=>page.evaluate(()=>{const canvas=document.querySelector('canvas');return [...canvas.getContext('2d').getImageData(Math.floor(canvas.width/2),Math.floor(canvas.height/2),1,1).data];});
    assert.deepEqual(await sample(),pixel.original);
    await page.selectOption('#viewerMode','compressed');
    assert.deepEqual(await sample(),pixel.compressed);
    await page.selectOption('#viewerMode','pair');
    await page.screenshot({path:path.join(root,'pixel-detail.png')});
    await page.selectOption('#viewerZoom','fit');
    await page.screenshot({path:path.join(root,'full-frame.png')});
    await page.selectOption('#viewerLevel','1');
    await page.waitForFunction(width=>document.querySelector('#canvasStatus').textContent==='' && document.querySelector('#viewerInfo').textContent.startsWith(String(width)), ordered[0].levels[1].width);
    await page.click('#closeViewer');
    await page.selectOption('#order','count');
    ordered.sort((a,b)=>b.reconstructed_count-a.reconstructed_count||a.state_id.localeCompare(b.state_id));
    assert.equal(await page.locator('.frame').first().getAttribute('id'),ordered[0].state_id);
    await page.setViewportSize({width:390,height:844});
    await page.selectOption('#order','error');
    await page.evaluate(async()=>{await Promise.all([...document.images].map(image=>{image.loading='eager';return image.decode();}));});
    assert(await page.evaluate(()=>document.documentElement.scrollWidth <= innerWidth));
    await page.screenshot({path:path.join(root,'mobile.png')});
    await page.locator('.frame [data-full]').first().click();
    await page.waitForFunction(()=>document.querySelector('#canvasStatus').textContent==='');
    await page.screenshot({path:path.join(root,'mobile-viewer.png')});
    assert(await page.evaluate(()=>document.querySelector('dialog').scrollWidth <= innerWidth));
    assert.deepEqual(errors,[]);
    console.log(`PASS: ${review.recovered} recovered / ${review.total-review.recovered} missing, all image assets, blinking, exact canvas RGB in both modes, resolution change, sort, desktop/mobile layout.`);
  } finally {clearTimeout(deadline);await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
