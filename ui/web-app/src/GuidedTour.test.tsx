// @vitest-environment jsdom
// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { GuidedTourOverlay, guidedTourTargets, guidedTourPosition } from "./GuidedTour";
import type { UiGuidedTour } from "./generated/sessionPageWire";

const tour: UiGuidedTour = { generation: 2, step_id: "chart",chapter:"Quick start",title:"The chart",body:"Choose a base map.",
  placement:"auto",presentation:"callout",restart_label:"Start over",page:"map",surface:"none",subject:"",row_uid:null,option_uid:null,targets:["base-map"],position:2,total:10,back_enabled:true,next_label:"Next",close_label:"Close tour",
  shortcuts:[{key:"enter",action:"next"},{key:"n",action:"next"},{key:"b",action:"back"}],
  map_point:null,viewport:{lat:47,lon:-122,zoom:7,centered:false,track_up:false} };
let root: Root, container: HTMLDivElement;
beforeEach(()=>{
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT",true);
  vi.stubGlobal("requestAnimationFrame",vi.fn(()=>1));vi.stubGlobal("cancelAnimationFrame",vi.fn());
  container=document.createElement('div');document.body.appendChild(container);root=createRoot(container);
});
afterEach(async()=>{await act(async()=>root.unmount());container.remove();document.body.replaceChildren();vi.restoreAllMocks();vi.unstubAllGlobals();});

it("renders core's section progress and continuation label, including the first complete-tour title card",async()=>{
  let measure!: FrameRequestCallback;
  vi.mocked(requestAnimationFrame).mockImplementation(callback=>{ measure=callback; return 1; });
  const action=vi.fn();
  await act(async()=>root.render(<GuidedTourOverlay tour={{...tour,step_id:"home",title:"Quick start complete",
    position:10,targets:[],next_label:"Continue to complete tour"}} busy={false} error={null} onAction={action}/>));
  await act(async()=>measure(0));
  expect(document.querySelector('.guidedTourChapter')?.textContent).toContain('10 / 10');
  const next=document.querySelector<HTMLButtonElement>('[data-testid=guided-tour-next]')!;
  expect(next.textContent).toBe('Continue to complete tour');
  await act(async()=>next.click());
  expect(action).toHaveBeenCalledExactlyOnceWith('next');
  await act(async()=>root.render(<GuidedTourOverlay tour={{...tour,generation:3,step_id:"planning-title",chapter:"Flight Planning",
    title:"Flight Planning",body:"",presentation:"title_card",targets:[],position:1,total:38}} busy={false} error={null} onAction={action}/>));
  expect(document.querySelector('.guidedTourChapter')?.textContent).toContain('1 / 38');
});

it("captures app keyboard shortcuts, traps focus, and dispatches only tour actions",async()=>{
  const action=vi.fn(), appShortcut=vi.fn();
  const original=document.createElement('button');document.body.appendChild(original);original.focus();
  await act(async()=>root.render(<GuidedTourOverlay tour={tour} busy={false} error={null} onAction={action}/>));
  window.addEventListener('keydown',appShortcut);
  const panel=document.querySelector('[data-testid=guided-tour-panel]')!;
  await act(async()=>panel.dispatchEvent(new KeyboardEvent('keydown',{key:')',bubbles:true,cancelable:true})));
  expect(appShortcut).not.toHaveBeenCalled();
  await act(async()=>panel.dispatchEvent(new KeyboardEvent('keydown',{key:'Tab',bubbles:true,cancelable:true})));
  expect(document.activeElement).toBe(document.querySelector('[data-testid=guided-tour-restart]'));
  await act(async()=>document.activeElement!.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true})));
  expect(action).toHaveBeenCalledWith('close');
  await act(async()=>root.render(null));expect(document.activeElement).toBe(original);
  window.removeEventListener('keydown',appShortcut);
});

it("keeps Close available while a route is computing and disables repeated navigation",async()=>{
  const action=vi.fn();
  await act(async()=>root.render(<GuidedTourOverlay tour={tour} busy error={null} onAction={action}/>));
  for (const id of ['next','back']) expect((document.querySelector(`[data-testid=guided-tour-${id}]`) as HTMLButtonElement).disabled).toBe(true);
  await act(async()=>(document.querySelector('[data-testid=guided-tour-close]') as HTMLButtonElement).click());
  expect(action).toHaveBeenCalledExactlyOnceWith('close');
});

it("uses tour shortcuts regardless of button focus and ignores repeats and modified keys",async()=>{
  let measure!: FrameRequestCallback;
  vi.mocked(requestAnimationFrame).mockImplementation(callback=>{ measure=callback; return 1; });
  const action=vi.fn(), appShortcut=vi.fn();
  await act(async()=>root.render(<GuidedTourOverlay tour={{...tour,targets:[]}} busy={false} error={null} onAction={action}/>));
  await act(async()=>measure(0));
  const close=document.querySelector<HTMLButtonElement>('[data-testid=guided-tour-close]')!;
  close.focus();
  window.addEventListener('keydown',appShortcut);
  const press=async(key:string,extra:KeyboardEventInit={})=>{
    const event=new KeyboardEvent('keydown',{key,bubbles:true,cancelable:true,...extra});
    await act(async()=>close.dispatchEvent(event));
    expect(event.defaultPrevented).toBe(true);
  };
  for (const key of ['Enter','n','N','b','B']) await press(key);
  for (const extra of [{repeat:true},{ctrlKey:true},{altKey:true},{metaKey:true},{isComposing:true}]) await press('Enter',extra);
  expect(action.mock.calls.map(([name])=>name)).toEqual(['next','next','next','back','back']);
  expect(appShortcut).not.toHaveBeenCalled();
  window.removeEventListener('keydown',appShortcut);
});

it("keyboard navigation respects loading and Back availability after rerender",async()=>{
  let measure!: FrameRequestCallback;
  vi.mocked(requestAnimationFrame).mockImplementation(callback=>{ measure=callback; return 1; });
  const action=vi.fn();
  let generation=tour.generation;
  const render=async(busy:boolean,back:boolean,targets:string[])=>{
    await act(async()=>root.render(<GuidedTourOverlay tour={{...tour,generation:++generation,back_enabled:back,targets}} busy={busy} error={null} onAction={action}/>));
    await act(async()=>measure(0));
  };
  const press=async(key:string)=>{await act(async()=>window.dispatchEvent(new KeyboardEvent('keydown',{key,bubbles:true,cancelable:true})));};
  await render(true,true,[]);
  await press('Enter'); await press('n'); await press('b');
  expect(action).not.toHaveBeenCalled();
  await render(false,false,[]);
  await press('b');
  expect(action).not.toHaveBeenCalled();
  await press('Enter');
  expect(action).toHaveBeenCalledExactlyOnceWith('next');
  await render(false,true,['missing-scene-target']);
  await press('n');
  expect(action).toHaveBeenCalledTimes(1);
  await press('b');
  expect(action).toHaveBeenLastCalledWith('back');
});

it("clips callout geometry to the visible viewport instead of outlining offscreen tray contents",()=>{
  const el=document.createElement('div');el.dataset.testid='chart-family-button';document.body.appendChild(el);
  el.checkVisibility=()=>true;
  vi.spyOn(el,'getBoundingClientRect').mockReturnValue(new DOMRect(20,-200,100,3000));
  const [bounds]=guidedTourTargets(tour);
  expect(bounds.top).toBe(0);expect(bounds.bottom).toBe(innerHeight);expect(bounds.left).toBe(20);
});

it("targets the core-selected plate thumbnail, not the entire folder or the first plate",()=>{
  vi.stubGlobal('CSS',{escape:(value:string)=>value});
  const ids=['plate-airport-button','plate-folder-button','plate-folder-tile:other','plate-folder-tile:diagram'];
  const bounds=ids.map((id,index)=>{
    const el=document.createElement('button');el.dataset.testid=id;document.body.appendChild(el);
    el.checkVisibility=()=>true;
    const rect=new DOMRect(20+index*100,100,70,80);
    vi.spyOn(el,'getBoundingClientRect').mockReturnValue(rect);
    return rect;
  });
  const scene={...tour,option_uid:'diagram',targets:['plate-airport-menu','plate-folder','plate-selected-thumbnail']};
  expect(guidedTourTargets(scene)).toEqual([bounds[0],bounds[1],bounds[3]]);
  expect(guidedTourTargets({...scene,option_uid:'missing'})).toEqual([bounds[0],bounds[1]]);
});

it("honors explicit placement and renders a title card without narration", async()=>{
  expect(guidedTourPosition({...tour,placement:"top_right"},[],380,240,1200,900)).toEqual({left:800,top:20});
  expect(guidedTourPosition({...tour,placement:"bottom_right"},[],380,240,1200,900)).toEqual({left:800,top:560});
  expect(guidedTourPosition({...tour,presentation:"title_card",placement:"center"},[],380,280,1200,900)).toEqual({left:410,top:310});
  const action=vi.fn();
  await act(async()=>root.render(<GuidedTourOverlay tour={{...tour,presentation:"title_card",placement:"center",body:"",title:"Flight Planning",targets:[]}} busy={false} error={null} onAction={action}/>));
  expect(document.querySelector('.guidedTourPanel.isTitleCard h2')?.textContent).toBe('Flight Planning');
  expect(document.querySelector('.guidedTourPanel p')).toBeNull();
  await act(async()=>(document.querySelector('[data-testid=guided-tour-restart]') as HTMLButtonElement).click());
  expect(action).toHaveBeenCalledWith('restart');
});

it("targets the rendered map marker and follows its displayed position",()=>{
  const svg=document.createElementNS("http://www.w3.org/2000/svg","svg");
  const marker=document.createElementNS("http://www.w3.org/2000/svg","g");
  marker.setAttribute("data-tour-anchor","map-spot-marker");svg.appendChild(marker);document.body.appendChild(svg);
  marker.checkVisibility=()=>true;
  let displayed=new DOMRect(200,300,32,43);
  vi.spyOn(marker,"getBoundingClientRect").mockImplementation(()=>displayed);
  const scene={...tour,targets:["map-spot-marker"]};
  expect(guidedTourTargets(scene)[0]).toEqual(displayed);
  displayed=new DOMRect(280,330,32,43);
  expect(guidedTourTargets(scene)[0]).toEqual(displayed);
});
