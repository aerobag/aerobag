// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
const $ = (selector, root = document) => root.querySelector(selector);
const $$ = (selector, root = document) => [...root.querySelectorAll(selector)];
const utc = value => value.replace('T', ' ').replace(/\.\d+(?:Z|\+00:00)$/, 'Z').replace('+00:00', 'Z');
const clock = value => utc(value).slice(11);
const rgb = value => value.slice(0, 3).join(', ');
const asset = (frame, file) => `frames/${frame.state_id}/${file}`;
const body = document.body;
body.dataset.mode = 'pair';
body.dataset.phase = 'original';
let review, phase = 'original';

function picture(frame, level, crop, name) {
  const pins = level.flagged_pixels.filter(p => p.x >= crop.x && p.x < crop.x + crop.width && p.y >= crop.y && p.y < crop.y + crop.height);
  const flags = pins.map(p => `<i class="flag" style="left:${(p.x-crop.x)/crop.width*100}%;top:${(p.y-crop.y)/crop.height*100}%;width:${100/crop.width}%;height:${100/crop.height}%"></i>`).join('');
  const image = source => `<img alt="${source === 'original' ? 'NOAA' : 'Aerobag'} res${level.res} crop ${crop.index+1}" class="${source}" width="96" height="96" src="${asset(frame, `res${level.res}-crop${crop.index}-${source}.png`)}">`;
  return `<div class="pane ${name === 'single' ? 'single-pane' : 'pair-pane'}"><div class="pane-label">${name === 'single' ? '<span class="phase-label">NOAA</span>' : name === 'original' ? 'NOAA original' : 'Aerobag compressed'}</div><div class="crop-image">${name === 'single' ? image('compressed')+image('original') : image(name)}${flags}</div></div>`;
}

function frameHTML(frame) {
  const first = frame.samples[0], last = frame.samples.at(-1), native = frame.levels[0];
  const sections = frame.levels.filter(level => level.flagged_pixels.length).map(level => {
    const crops = level.crops.map(crop => `<div class="crop">
      <div class="crop-caption"><span>Crop ${crop.index+1} · 96 × 96 pixels · x${crop.x}, y${crop.y}</span><button data-inspect="${frame.state_id}" data-res="${level.res}" data-x="${crop.x+48}" data-y="${crop.y+48}">Inspect</button></div>
      <div class="crop-pair">${picture(frame,level,crop,'original')}${picture(frame,level,crop,'compressed')}${picture(frame,level,crop,'single')}</div>
    </div>`).join('');
    const pixels = level.flagged_pixels.map(p => `<tr><td><button data-inspect="${frame.state_id}" data-res="${level.res}" data-x="${p.x+.5}" data-y="${p.y+.5}">${p.x}, ${p.y}</button></td><td>${p.lat.toFixed(4)}, ${p.lon.toFixed(4)}</td><td><span class="colors"><i class="chip" style="background:rgb(${rgb(p.original)})"></i>${rgb(p.original)}</span></td><td><span class="colors"><i class="chip" style="background:rgb(${rgb(p.compressed)})"></i>${rgb(p.compressed)}</span></td><td><b>${p.error}</b></td></tr>`).join('');
    return `<div class="level-title"><h3>res${level.res} ${level.res===0?'· native pixels':`· ${2**level.res}× sample spacing`}</h3><span>${level.flagged_pixels.length} flagged / ${level.opaque_pixels.toLocaleString()} opaque</span></div>${crops}<div class="table-scroll"><table class="pixel-table"><thead><tr><th>Pixel x, y</th><th>Lat, lon</th><th>NOAA RGB</th><th>Aerobag RGB</th><th>Max Δ</th></tr></thead><tbody>${pixels}</tbody></table></div>`;
  }).join('');
  const pins = native.flagged_pixels.map(p => `<i class="pin" style="left:${p.x/native.width*100}%;top:${p.y/native.height*100}%"></i>`).join('');
  return `<article class="frame" id="${frame.state_id}"><div class="frame-head"><div><h2>${utc(frame.observed_at_utc)}</h2><div class="frame-sub">Warning ${clock(first.sampled_at_utc)}–${clock(last.sampled_at_utc)} · production ${frame.release}</div><div class="stats"><span><b>${frame.reconstructed_count}</b> counted across resolutions</span><span><b>${native.flagged_pixels.length}</b> unique native pixels</span><span><b>${frame.reconstructed_max_error}</b> max RGB error</span></div></div><button data-full="${frame.state_id}">Open full frame</button></div><div class="frame-body"><aside class="context"><div><div class="overview"><img loading="lazy" src="${asset(frame, 'res3-original.png')}" alt="NOAA frame overview with flagged locations">${pins}</div><div class="frame-sub">7000 × 3500 source pixels</div></div><div class="metadata"><span class="verified">Reconstruction matches production metrics</span><br>${frame.recovery.includes('development')?'Development':'Production'} cache · SHA-256 verified<br>NOAA RGBA and alpha preserved before quantization<br>Deployed tiler ${frame.commit}<br>Nearest-neighbor resolution levels<br><a href="${asset(frame,'analysis.json')}">Frame evidence</a><br><a href="${asset(frame,'res0-original.png')}" target="_blank">Original PNG</a> · <a href="${asset(frame,'res0-compressed.png')}" target="_blank">Compressed PNG</a></div></aside><div class="details">${sections}<div class="level-tail">${frame.levels.filter(l=>!l.flagged_pixels.length).map(l=>`res${l.res}: no flagged pixels`).join(' · ')}</div></div></div></article>`;
}

function renderFrames() {
  const frames = review.frames.filter(f=>f.verified);
  const order = $('#order').value;
  frames.sort((a,b) => order === 'time' ? a.state_id.localeCompare(b.state_id) : (order === 'count' ? b.reconstructed_count-a.reconstructed_count : b.reconstructed_max_error-a.reconstructed_max_error) || a.state_id.localeCompare(b.state_id));
  $('#frames').innerHTML = frames.map(frameHTML).join('');
  updatePhase();
}
function updatePhase() {
  body.dataset.phase = phase;
  const source = body.dataset.mode === 'blink' ? phase : body.dataset.mode;
  $$('.phase-label').forEach(node=>node.textContent = source === 'compressed' ? 'Aerobag compressed' : 'NOAA original');
  if ($('#viewer').open) draw();
}
setInterval(()=>{ phase=phase==='original'?'compressed':'original'; updatePhase(); }, 800);
$$('input[name="mode"]').forEach(input=>input.addEventListener('change',()=>{body.dataset.mode=input.value; updatePhase();}));
$('#cropZoom').onchange = event => document.documentElement.style.setProperty('--crop-size', `${96*Number(event.target.value)}px`);
$('#marks').onchange = event => body.classList.toggle('marks',event.target.checked);
$$('input[name="background"]').forEach(input=>input.addEventListener('change',()=>{document.documentElement.style.setProperty('--radar-bg',input.value); if ($('#viewer').open) draw();}));
$('#order').onchange = renderFrames;

const canvas = $('#fullCanvas'), context = canvas.getContext('2d');
const viewer = {frame:null, images:null, cx:0, cy:0, zoom:1, fit:true, drag:null, token:0};
const sampleCanvas = document.createElement('canvas'); sampleCanvas.width=sampleCanvas.height=1;
const sampleContext = sampleCanvas.getContext('2d', {willReadFrequently:true});
const sourceName = key => key === 'compressed' ? 'Aerobag compressed' : 'NOAA original';
function panes() {const mode=$('#viewerMode').value; return mode==='pair'?['original','compressed']:[mode==='blink'?phase:mode];}
function dimensions() {const r=canvas.getBoundingClientRect(); return {width:r.width,height:r.height,paneWidth:r.width/panes().length};}
function scale() { const d=dimensions(); return viewer.fit?Math.min(d.paneWidth/viewer.images.original.width, d.height/viewer.images.original.height):viewer.zoom; }
function draw() {
  if(!viewer.images || !$('#viewer').open) return;
  const d=dimensions(), ratio=devicePixelRatio, z=scale(), list=panes();
  const w=Math.round(d.width*ratio), h=Math.round(d.height*ratio);
  if(canvas.width!==w || canvas.height!==h){canvas.width=w;canvas.height=h;}
  context.setTransform(ratio,0,0,ratio,0,0);
  context.imageSmoothingEnabled=false;
  context.fillStyle=getComputedStyle(document.documentElement).getPropertyValue('--radar-bg');
  context.fillRect(0,0,d.width,d.height);
  list.forEach((key,index)=>{
    const left=index*d.paneWidth, dx=left+d.paneWidth/2-viewer.cx*z, dy=d.height/2-viewer.cy*z;
    context.save();context.beginPath();context.rect(left,0,d.paneWidth,d.height);context.clip();
    context.drawImage(viewer.images[key],dx,dy,viewer.images[key].width*z,viewer.images[key].height*z);
    if($('#viewerMarks').checked){context.strokeStyle='#ff4baa';context.lineWidth=1;
      viewer.frame.levels[Number($('#viewerLevel').value)].flagged_pixels.forEach(p=>context.strokeRect(dx+p.x*z-.5,dy+p.y*z-.5,z+1,z+1));}
    context.fillStyle='#111c';context.fillRect(left+8,8,150,26);context.fillStyle='white';context.font='13px system-ui';context.fillText(sourceName(key),left+16,26);
    context.restore();
  });
  if(list.length===2){context.strokeStyle='#fff';context.lineWidth=1;context.beginPath();context.moveTo(d.paneWidth,0);context.lineTo(d.paneWidth,d.height);context.stroke();}
  $('#viewerInfo').textContent=`${viewer.images.original.width} × ${viewer.images.original.height} · ${(z*100).toFixed(0)}% · nearest-neighbor`;
}
async function loadLevel(res, focus=null) {
  const token=++viewer.token; viewer.images=null; $('#canvasStatus').textContent='Loading exact pixels…';
  const frame=viewer.frame;
  try {
    const loaded = await Promise.all(['original','compressed'].map(async key=>{const image=new Image();image.src=asset(frame,`res${res}-${key}.png`);await image.decode();return [key,image];}));
    if(token!==viewer.token || !$('#viewer').open) return;
    viewer.images=Object.fromEntries(loaded);
    viewer.cx=focus?.x ?? viewer.images.original.width/2;viewer.cy=focus?.y ?? viewer.images.original.height/2;
    const level=frame.levels[res];
    $('#viewerPixel').innerHTML='<option value="">—</option>'+level.flagged_pixels.map((p,i)=>`<option value="${i}">${p.x}, ${p.y} · Δ${p.error}</option>`).join('');
    $('#canvasStatus').textContent=''; draw();
    if(focus) readPixel(focus.x,focus.y);
  }catch(error){if(token===viewer.token) $('#canvasStatus').textContent=`Image load failed: ${error.message}`;}
}
function openViewer(state,res=0,focus=null) {
  viewer.frame=review.frames.find(f=>f.state_id===state);
  $('#viewerTitle').textContent=utc(viewer.frame.observed_at_utc);
  $('#viewerLevel').value=String(res);$('#viewerMode').value=body.dataset.mode;
  viewer.fit=!focus;viewer.zoom=16;$('#viewerZoom').value=focus?'16':'fit';$('#pixelReadout').textContent='—';
  $('#viewer').showModal();loadLevel(res,focus);
}
document.addEventListener('click',event=>{
  const full=event.target.closest('[data-full]'); if(full) openViewer(full.dataset.full);
  const inspect=event.target.closest('[data-inspect]');if(inspect)openViewer(inspect.dataset.inspect,Number(inspect.dataset.res),{x:Number(inspect.dataset.x),y:Number(inspect.dataset.y)});
});
$('#closeViewer').onclick=()=>$('#viewer').close();
$('#viewer').addEventListener('close',()=>{viewer.token++;viewer.images=null;context.clearRect(0,0,canvas.width,canvas.height);});
$('#viewerMode').onchange=()=>{if(viewer.fit && viewer.images){viewer.cx=viewer.images.original.width/2;viewer.cy=viewer.images.original.height/2;}draw();};
$('#viewerLevel').onchange=()=>{viewer.fit=true;$('#viewerZoom').value='fit';loadLevel(Number($('#viewerLevel').value));};
$('#viewerZoom').onchange=()=>{viewer.fit=$('#viewerZoom').value==='fit';viewer.zoom=Number($('#viewerZoom').value)||1;if(viewer.fit && viewer.images){viewer.cx=viewer.images.original.width/2;viewer.cy=viewer.images.original.height/2;}draw();};
$('#viewerMarks').onchange=draw;
$('#viewerPixel').onchange=()=>{const value=$('#viewerPixel').value;if(value==='')return;const p=viewer.frame.levels[Number($('#viewerLevel').value)].flagged_pixels[Number(value)];viewer.cx=p.x+.5;viewer.cy=p.y+.5;viewer.fit=false;viewer.zoom=16;$('#viewerZoom').value='16';draw();readPixel(p.x,p.y);};
new ResizeObserver(()=>{if(viewer.images)draw();}).observe(canvas.parentElement);
function imagePoint(event) {const rect=canvas.getBoundingClientRect(),d=dimensions(),z=scale();return {x:viewer.cx+((event.clientX-rect.left)%d.paneWidth-d.paneWidth/2)/z,y:viewer.cy+(event.clientY-rect.top-d.height/2)/z};}
function readPixel(x,y) {
  if(!viewer.images)return;x=Math.floor(x);y=Math.floor(y);const image=viewer.images.original;
  if(x<0||y<0||x>=image.width||y>=image.height)return;
  const pixels=['original','compressed'].map(key=>{sampleContext.clearRect(0,0,1,1);sampleContext.drawImage(viewer.images[key],x,y,1,1,0,0,1,1);return [...sampleContext.getImageData(0,0,1,1).data];});
  const error=Math.max(...pixels[0].slice(0,3).map((v,i)=>Math.abs(v-pixels[1][i])));
  $('#pixelReadout').textContent=`x${x}, y${y} | NOAA RGBA (${pixels[0].join(', ')}) | Aerobag RGBA (${pixels[1].join(', ')}) | max RGB error ${error}`;
}
canvas.onpointerdown=event=>{if(!viewer.images)return;canvas.setPointerCapture(event.pointerId);viewer.drag={x:event.clientX,y:event.clientY,cx:viewer.cx,cy:viewer.cy};readPixel(...Object.values(imagePoint(event)));};
canvas.onpointermove=event=>{if(!viewer.images)return;if(viewer.drag){viewer.cx=viewer.drag.cx-(event.clientX-viewer.drag.x)/scale();viewer.cy=viewer.drag.cy-(event.clientY-viewer.drag.y)/scale();draw();}else {const p=imagePoint(event);readPixel(p.x,p.y);}};
canvas.onpointerup=()=>{viewer.drag=null;};canvas.onpointercancel=()=>{viewer.drag=null;};
canvas.addEventListener('wheel',event=>{if(!viewer.images)return;event.preventDefault();const before=imagePoint(event),z=scale(),next=Math.min(32,Math.max(.04,z*(event.deltaY<0?1.25:.8)));viewer.fit=false;viewer.zoom=next;const after=imagePoint(event);viewer.cx+=before.x-after.x;viewer.cy+=before.y-after.y;$('#viewerZoom').value='';draw();},{passive:false});

(async()=>{
  const response=await fetch('review.json');if(!response.ok)throw new Error(`Evidence HTTP ${response.status}`);review=await response.json();
  $('#period').textContent=`${review.period_start.slice(0,10)} – ${review.period_end.slice(0,10)} · UTC`;
  $('#summary').innerHTML=`<div><strong>${review.total}</strong><span>exact warning frames identified</span></div><div><strong>${review.recovered}</strong><span>originals recovered &amp; verified</span></div><div><strong>${review.total-review.recovered}</strong><span>originals not recovered</span></div><div><strong>&gt; ${review.threshold} / 255</strong><span>max-channel RGB warning threshold</span></div><div><strong>Unchanged</strong><span>production palette &amp; thresholds</span></div>`;
  $('#unavailable tbody').innerHTML=review.frames.filter(f=>!f.verified).map(f=>`<tr class="${f.samples[0].poor_count===6?'missing-peak':''}"><td>${utc(f.observed_at_utc)}</td><td>${utc(f.samples[0].sampled_at_utc)}</td><td>${f.samples[0].poor_count}</td><td>${f.samples[0].max_error}</td><td><code>${f.state_id}</code><br><span class="status">No matching cached original recovered</span></td></tr>`).join('');
  renderFrames();document.documentElement.dataset.ready='true';
  if(location.hash){document.getElementById(location.hash.slice(1))?.scrollIntoView();}
})().catch(error=>{$('#summary').textContent=`Evidence load failed: ${error.message}`;console.error(error);});
