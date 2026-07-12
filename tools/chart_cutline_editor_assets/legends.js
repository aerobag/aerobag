"use strict";

const SVG_NS = "http://www.w3.org/2000/svg";

const elements = {
  familySelect: document.querySelector("#familySelect"),
  chartSelect: document.querySelector("#chartSelect"),
  previousChart: document.querySelector("#previousChart"),
  nextChart: document.querySelector("#nextChart"),
  undo: document.querySelector("#undo"),
  redo: document.querySelector("#redo"),
  drawRegion: document.querySelector("#drawRegion"),
  deleteRegion: document.querySelector("#deleteRegion"),
  moveEarlier: document.querySelector("#moveEarlier"),
  moveLater: document.querySelector("#moveLater"),
  reloadLayout: document.querySelector("#reloadLayout"),
  saveLayout: document.querySelector("#saveLayout"),
  saveState: document.querySelector("#saveState"),
  chartTitle: document.querySelector("#chartTitle"),
  chartFacts: document.querySelector("#chartFacts"),
  regionCount: document.querySelector("#regionCount"),
  overviewStage: document.querySelector("#overviewStage"),
  overviewImage: document.querySelector("#overviewImage"),
  overviewSvg: document.querySelector("#overviewSvg"),
  regionShapes: document.querySelector("#regionShapes"),
  regionHandles: document.querySelector("#regionHandles"),
  regionTitle: document.querySelector("#regionTitle"),
  regionX: document.querySelector("#regionX"),
  regionY: document.querySelector("#regionY"),
  regionWidth: document.querySelector("#regionWidth"),
  regionHeight: document.querySelector("#regionHeight"),
  maxOutputWidth: document.querySelector("#maxOutputWidth"),
  regionList: document.querySelector("#regionList"),
  previewFacts: document.querySelector("#previewFacts"),
  previewViewport: document.querySelector("#previewViewport"),
  previewImage: document.querySelector("#previewImage"),
  message: document.querySelector("#message"),
};

const state = {
  families: [],
  family: null,
  charts: [],
  chart: null,
  regions: [],
  revision: null,
  maxOutputWidth: 1210,
  selectedIndex: -1,
  dirty: false,
  undo: [],
  redo: [],
  drawMode: false,
  drag: null,
  messageTimer: null,
};

async function api(url, options) {
  const response = await fetch(url, options);
  const contentType = response.headers.get("Content-Type") || "";
  const body = contentType.includes("application/json") ? await response.json() : null;
  if (!response.ok) {
    throw new Error(body && body.error ? body.error : response.statusText);
  }
  return body;
}

async function initialize() {
  bindControls();
  try {
    const result = await api("/api/families");
    state.families = result.families;
    for (const family of state.families) {
      const option = document.createElement("option");
      option.value = family.id;
      option.textContent = family.label;
      elements.familySelect.append(option);
    }
    const requested = new URLSearchParams(window.location.search).get("family");
    const remembered = window.localStorage.getItem("aerobag-legend-family");
    const initial = [requested, remembered, "TAC", state.families[0].id]
      .find((id) => state.families.some((family) => family.id === id));
    await loadFamily(initial);
  } catch (error) {
    showMessage(error.message, true, 0);
  }
}

function bindControls() {
  elements.familySelect.addEventListener("change", async () => {
    if (!canLeaveDirtyChart()) {
      elements.familySelect.value = state.family.id;
      return;
    }
    await loadFamily(elements.familySelect.value);
  });
  elements.chartSelect.addEventListener("change", async () => {
    if (!canLeaveDirtyChart()) {
      elements.chartSelect.value = state.chart.name;
      return;
    }
    await loadChart(elements.chartSelect.value);
  });
  elements.previousChart.addEventListener("click", () => moveChart(-1));
  elements.nextChart.addEventListener("click", () => moveChart(1));
  elements.undo.addEventListener("click", undo);
  elements.redo.addEventListener("click", redo);
  elements.drawRegion.addEventListener("click", toggleDrawMode);
  elements.deleteRegion.addEventListener("click", deleteRegion);
  elements.moveEarlier.addEventListener("click", () => moveRegion(-1));
  elements.moveLater.addEventListener("click", () => moveRegion(1));
  elements.reloadLayout.addEventListener("click", reloadLayout);
  elements.saveLayout.addEventListener("click", saveLayout);
  [elements.regionX, elements.regionY, elements.regionWidth, elements.regionHeight]
    .forEach((input) => input.addEventListener("change", updateRegionFromInputs));
  elements.maxOutputWidth.addEventListener("change", updateMaxOutputWidth);
  elements.overviewSvg.addEventListener("pointerdown", beginPointerAction);
  window.addEventListener("pointermove", continuePointerAction);
  window.addEventListener("pointerup", endPointerAction);
  window.addEventListener("pointercancel", endPointerAction);
  window.addEventListener("keydown", handleKeyDown);
  window.addEventListener("beforeunload", (event) => {
    if (state.dirty) {
      event.preventDefault();
      event.returnValue = "";
    }
  });
}

async function loadFamily(familyId) {
  setBusy(true);
  try {
    const result = await api("/api/charts?family=" + encodeURIComponent(familyId));
    state.family = state.families.find((family) => family.id === familyId);
    state.charts = result.charts;
    elements.familySelect.value = familyId;
    elements.chartSelect.replaceChildren();
    for (const chart of state.charts) {
      const option = document.createElement("option");
      option.value = chart.name;
      option.textContent = chart.name;
      elements.chartSelect.append(option);
    }
    window.localStorage.setItem("aerobag-legend-family", familyId);
    const parameters = new URLSearchParams(window.location.search);
    const requested = parameters.get("family") === familyId ? parameters.get("chart") : null;
    const remembered = window.localStorage.getItem("aerobag-legend-chart-" + familyId);
    const preferred = {
      SEC: "Seattle SEC",
      TAC: "Seattle TAC",
      ENR_L: "ENR_L01",
      ENR_H: "ENR_H01",
    }[familyId];
    const initial = [requested, remembered, preferred, state.charts[0] && state.charts[0].name]
      .find((name) => state.charts.some((chart) => chart.name === name));
    if (!initial) {
      throw new Error("No charts are available for " + state.family.label);
    }
    await loadChart(initial);
  } catch (error) {
    showMessage(error.message, true, 0);
  } finally {
    setBusy(false);
  }
}

async function loadChart(name) {
  setBusy(true);
  try {
    const familyQuery = "family=" + encodeURIComponent(state.family.id);
    const chart = await api("/api/chart?" + familyQuery + "&name=" + encodeURIComponent(name));
    const layout = await api("/api/legend?" + familyQuery + "&name=" + encodeURIComponent(name));
    state.chart = chart;
    state.regions = layout.regions.map(copyRegion);
    state.revision = layout.revision;
    state.maxOutputWidth = layout.max_output_width;
    state.selectedIndex = state.regions.length ? 0 : -1;
    state.dirty = false;
    state.undo = [];
    state.redo = [];
    state.drawMode = false;
    elements.chartSelect.value = name;
    elements.maxOutputWidth.value = String(state.maxOutputWidth);
    elements.overviewImage.src = chart.overview_url;
    elements.overviewStage.style.aspectRatio = chart.width + " / " + chart.height;
    elements.overviewSvg.setAttribute("viewBox", "0 0 " + chart.width + " " + chart.height);
    elements.chartTitle.textContent = chart.name;
    elements.chartFacts.textContent = chart.width + " x " + chart.height + " source pixels";
    window.localStorage.setItem("aerobag-legend-chart-" + state.family.id, name);
    const url = new URL(window.location.href);
    url.searchParams.set("family", state.family.id);
    url.searchParams.set("chart", name);
    window.history.replaceState(null, "", url);
    render();
    updatePreview();
  } catch (error) {
    showMessage(error.message, true, 0);
  } finally {
    setBusy(false);
  }
}

function render() {
  renderOverview();
  renderRegionList();
  updateInputs();
  updateUiState();
}

function renderOverview() {
  elements.regionShapes.replaceChildren();
  elements.regionHandles.replaceChildren();
  if (!state.chart) {
    return;
  }
  const scale = state.chart.width / Math.max(elements.overviewStage.clientWidth, 1);
  const handleRadius = Math.max(6 * scale, 15);
  const badgeRadius = Math.max(10 * scale, 24);
  state.regions.forEach((region, index) => {
    const rect = document.createElementNS(SVG_NS, "rect");
    rect.classList.add("legendRegion");
    if (index === state.selectedIndex) {
      rect.classList.add("selected");
    }
    rect.dataset.index = String(index);
    setRectAttributes(rect, region);
    elements.regionShapes.append(rect);

    const badge = document.createElementNS(SVG_NS, "circle");
    badge.classList.add("regionNumber");
    badge.setAttribute("cx", String(region.x + badgeRadius));
    badge.setAttribute("cy", String(region.y + badgeRadius));
    badge.setAttribute("r", String(badgeRadius));
    elements.regionShapes.append(badge);
    const label = document.createElementNS(SVG_NS, "text");
    label.classList.add("regionNumberText");
    label.setAttribute("x", String(region.x + badgeRadius));
    label.setAttribute("y", String(region.y + badgeRadius));
    label.setAttribute("font-size", String(badgeRadius * 1.1));
    label.textContent = String(index + 1);
    elements.regionShapes.append(label);

    if (index === state.selectedIndex) {
      for (const [corner, point] of Object.entries(regionCorners(region))) {
        const handle = document.createElementNS(SVG_NS, "circle");
        handle.classList.add("vertexHandle", "selected");
        handle.dataset.index = String(index);
        handle.dataset.corner = corner;
        handle.setAttribute("cx", String(point[0]));
        handle.setAttribute("cy", String(point[1]));
        handle.setAttribute("r", String(handleRadius));
        elements.regionHandles.append(handle);
      }
    }
  });
  elements.regionCount.textContent = state.regions.length + " regions";
}

function renderRegionList() {
  elements.regionList.replaceChildren();
  state.regions.forEach((region, index) => {
    const item = document.createElement("li");
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = (index + 1) + ": " + region.width + " x " + region.height;
    button.title = "Region " + (index + 1) + " at " + region.x + ", " + region.y;
    button.classList.toggle("selected", index === state.selectedIndex);
    button.addEventListener("click", () => selectRegion(index, true));
    item.append(button);
    elements.regionList.append(item);
  });
}

function beginPointerAction(event) {
  if (!state.chart) {
    return;
  }
  const point = constrainedPoint(event.clientX, event.clientY);
  if (state.drawMode) {
    event.preventDefault();
    pushUndo();
    state.regions.push({ x: point[0], y: point[1], width: 1, height: 1 });
    state.selectedIndex = state.regions.length - 1;
    state.drag = { kind: "draw", pointerId: event.pointerId, start: point, moved: false };
    render();
    return;
  }
  const handle = event.target.closest(".vertexHandle");
  if (handle) {
    event.preventDefault();
    const index = Number(handle.dataset.index);
    selectRegion(index, false);
    pushUndo();
    state.drag = {
      kind: "resize",
      pointerId: event.pointerId,
      corner: handle.dataset.corner,
      original: copyRegion(state.regions[index]),
      moved: false,
    };
    return;
  }
  const rect = event.target.closest(".legendRegion");
  if (rect) {
    event.preventDefault();
    const index = Number(rect.dataset.index);
    selectRegion(index, false);
    pushUndo();
    state.drag = {
      kind: "move",
      pointerId: event.pointerId,
      start: point,
      original: copyRegion(state.regions[index]),
      moved: false,
    };
  }
}

function continuePointerAction(event) {
  if (!state.drag || state.drag.pointerId !== event.pointerId) {
    return;
  }
  const point = constrainedPoint(event.clientX, event.clientY);
  if (state.drag.kind === "draw") {
    state.regions[state.selectedIndex] = rectangleFromPoints(state.drag.start, point);
  } else if (state.drag.kind === "resize") {
    state.regions[state.selectedIndex] = resizedRegion(state.drag.original, state.drag.corner, point);
  } else {
    const dx = point[0] - state.drag.start[0];
    const dy = point[1] - state.drag.start[1];
    const original = state.drag.original;
    state.regions[state.selectedIndex] = {
      x: clamp(Math.round(original.x + dx), 0, state.chart.width - original.width),
      y: clamp(Math.round(original.y + dy), 0, state.chart.height - original.height),
      width: original.width,
      height: original.height,
    };
  }
  state.drag.moved = true;
  markDirty();
  renderOverview();
  updateInputs();
}

function endPointerAction(event) {
  if (!state.drag || state.drag.pointerId !== event.pointerId) {
    return;
  }
  const drag = state.drag;
  state.drag = null;
  if (!drag.moved) {
    if (drag.kind === "draw") {
      state.regions.splice(state.selectedIndex, 1);
      state.selectedIndex = state.regions.length ? state.regions.length - 1 : -1;
    }
    state.undo.pop();
  } else if (drag.kind === "draw") {
    state.drawMode = false;
  }
  render();
  updatePreview();
}

function rectangleFromPoints(left, right) {
  const x = Math.round(Math.min(left[0], right[0]));
  const y = Math.round(Math.min(left[1], right[1]));
  return {
    x,
    y,
    width: Math.max(1, Math.round(Math.max(left[0], right[0])) - x),
    height: Math.max(1, Math.round(Math.max(left[1], right[1])) - y),
  };
}

function resizedRegion(original, corner, point) {
  const corners = regionCorners(original);
  const opposite = { nw: "se", ne: "sw", se: "nw", sw: "ne" }[corner];
  return rectangleFromPoints(corners[opposite], point);
}

function regionCorners(region) {
  return {
    nw: [region.x, region.y],
    ne: [region.x + region.width, region.y],
    se: [region.x + region.width, region.y + region.height],
    sw: [region.x, region.y + region.height],
  };
}

function setRectAttributes(rect, region) {
  rect.setAttribute("x", String(region.x));
  rect.setAttribute("y", String(region.y));
  rect.setAttribute("width", String(region.width));
  rect.setAttribute("height", String(region.height));
}

function constrainedPoint(clientX, clientY) {
  const point = elements.overviewSvg.createSVGPoint();
  point.x = clientX;
  point.y = clientY;
  const transformed = point.matrixTransform(elements.overviewSvg.getScreenCTM().inverse());
  return [
    clamp(transformed.x, 0, state.chart.width),
    clamp(transformed.y, 0, state.chart.height),
  ];
}

function toggleDrawMode() {
  state.drawMode = !state.drawMode;
  updateUiState();
}

function selectRegion(index, refreshPreview) {
  state.selectedIndex = index;
  render();
  if (refreshPreview) {
    updatePreview();
  }
}

function deleteRegion() {
  if (state.selectedIndex < 0) {
    return;
  }
  pushUndo();
  state.regions.splice(state.selectedIndex, 1);
  state.selectedIndex = Math.min(state.selectedIndex, state.regions.length - 1);
  markDirty();
  render();
  updatePreview();
}

function moveRegion(direction) {
  const index = state.selectedIndex;
  const target = index + direction;
  if (index < 0 || target < 0 || target >= state.regions.length) {
    return;
  }
  pushUndo();
  const [region] = state.regions.splice(index, 1);
  state.regions.splice(target, 0, region);
  state.selectedIndex = target;
  markDirty();
  render();
}

function updateRegionFromInputs() {
  if (state.selectedIndex < 0) {
    return;
  }
  const values = [elements.regionX, elements.regionY, elements.regionWidth, elements.regionHeight]
    .map((input) => Math.round(Number(input.value)));
  if (!values.every(Number.isFinite)) {
    updateInputs();
    return;
  }
  const [rawX, rawY, rawWidth, rawHeight] = values;
  const width = clamp(rawWidth, 1, state.chart.width);
  const height = clamp(rawHeight, 1, state.chart.height);
  pushUndo();
  state.regions[state.selectedIndex] = {
    x: clamp(rawX, 0, state.chart.width - width),
    y: clamp(rawY, 0, state.chart.height - height),
    width,
    height,
  };
  markDirty();
  render();
  updatePreview();
}

function updateMaxOutputWidth() {
  const value = Math.round(Number(elements.maxOutputWidth.value));
  if (!Number.isFinite(value)) {
    elements.maxOutputWidth.value = String(state.maxOutputWidth);
    return;
  }
  pushUndo();
  state.maxOutputWidth = clamp(value, 320, 4096);
  elements.maxOutputWidth.value = String(state.maxOutputWidth);
  markDirty();
}

function updateInputs() {
  const region = state.regions[state.selectedIndex];
  const inputs = [elements.regionX, elements.regionY, elements.regionWidth, elements.regionHeight];
  inputs.forEach((input) => { input.disabled = !region; });
  if (!region) {
    elements.regionTitle.textContent = "No region selected";
    inputs.forEach((input) => { input.value = ""; });
    return;
  }
  elements.regionTitle.textContent = "Region " + (state.selectedIndex + 1) + " of " + state.regions.length;
  elements.regionX.value = String(region.x);
  elements.regionY.value = String(region.y);
  elements.regionWidth.value = String(region.width);
  elements.regionHeight.value = String(region.height);
}

function updatePreview() {
  const region = state.regions[state.selectedIndex];
  if (!state.chart || !region) {
    elements.previewImage.removeAttribute("src");
    elements.previewImage.style.display = "none";
    elements.previewFacts.textContent = "";
    return;
  }
  elements.previewImage.style.display = "block";
  elements.previewImage.src = "/api/crop?family=" + encodeURIComponent(state.family.id)
    + "&name=" + encodeURIComponent(state.chart.name)
    + "&x=" + region.x + "&y=" + region.y
    + "&width=" + region.width + "&height=" + region.height
    + "&revision=" + Date.now();
  elements.previewFacts.textContent = region.width + " x " + region.height + " source pixels at 1:1";
  elements.previewViewport.scrollTo(0, 0);
}

function pushUndo() {
  state.undo.push({
    regions: state.regions.map(copyRegion),
    maxOutputWidth: state.maxOutputWidth,
    selectedIndex: state.selectedIndex,
  });
  if (state.undo.length > 100) {
    state.undo.shift();
  }
  state.redo = [];
}

function restoreSnapshot(snapshot) {
  state.regions = snapshot.regions.map(copyRegion);
  state.maxOutputWidth = snapshot.maxOutputWidth;
  state.selectedIndex = Math.min(snapshot.selectedIndex, state.regions.length - 1);
  elements.maxOutputWidth.value = String(state.maxOutputWidth);
  markDirty();
  render();
  updatePreview();
}

function currentSnapshot() {
  return {
    regions: state.regions.map(copyRegion),
    maxOutputWidth: state.maxOutputWidth,
    selectedIndex: state.selectedIndex,
  };
}

function undo() {
  if (!state.undo.length) {
    return;
  }
  state.redo.push(currentSnapshot());
  restoreSnapshot(state.undo.pop());
}

function redo() {
  if (!state.redo.length) {
    return;
  }
  state.undo.push(currentSnapshot());
  restoreSnapshot(state.redo.pop());
}

async function saveLayout() {
  if (!state.chart || !state.dirty) {
    return;
  }
  setBusy(true);
  try {
    const result = await api("/api/legend/save", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        family: state.family.id,
        name: state.chart.name,
        regions: state.regions,
        max_output_width: state.maxOutputWidth,
        revision: state.revision,
      }),
    });
    state.regions = result.regions.map(copyRegion);
    state.revision = result.revision;
    state.dirty = false;
    state.undo = [];
    state.redo = [];
    render();
    showMessage("Saved " + state.chart.name + ".legend.json", false);
  } catch (error) {
    showMessage(error.message, true, 0);
  } finally {
    setBusy(false);
  }
}

async function reloadLayout() {
  if (!state.chart || !canLeaveDirtyChart()) {
    return;
  }
  await loadChart(state.chart.name);
}

async function moveChart(direction) {
  if (!state.chart || !canLeaveDirtyChart()) {
    return;
  }
  const index = state.charts.findIndex((chart) => chart.name === state.chart.name);
  const next = (index + direction + state.charts.length) % state.charts.length;
  await loadChart(state.charts[next].name);
}

function canLeaveDirtyChart() {
  return !state.dirty || window.confirm("Discard unsaved legend-region edits?");
}

function markDirty() {
  state.dirty = true;
  updateUiState();
}

function updateUiState() {
  elements.saveState.textContent = state.dirty ? "Unsaved" : "Saved";
  elements.saveState.classList.toggle("dirty", state.dirty);
  elements.saveLayout.disabled = !state.dirty;
  elements.undo.disabled = state.undo.length === 0;
  elements.redo.disabled = state.redo.length === 0;
  const selected = state.selectedIndex >= 0;
  elements.deleteRegion.disabled = !selected;
  elements.moveEarlier.disabled = !selected || state.selectedIndex === 0;
  elements.moveLater.disabled = !selected || state.selectedIndex === state.regions.length - 1;
  elements.drawRegion.classList.toggle("activeTool", state.drawMode);
  elements.overviewSvg.classList.toggle("drawMode", state.drawMode);
}

function setBusy(busy) {
  elements.familySelect.disabled = busy;
  elements.chartSelect.disabled = busy;
  elements.reloadLayout.disabled = busy;
  elements.saveLayout.disabled = busy || !state.dirty;
}

function handleKeyDown(event) {
  if (!state.chart || event.target.matches("input, select, button")) {
    return;
  }
  const key = event.key.toLowerCase();
  if (!event.ctrlKey && !event.metaKey && !event.altKey && (key === "n" || key === "p")) {
    event.preventDefault();
    if (state.regions.length) {
      const direction = key === "n" ? 1 : -1;
      selectRegion((state.selectedIndex + direction + state.regions.length) % state.regions.length, true);
    }
    return;
  }
  if ((event.ctrlKey || event.metaKey) && key === "s") {
    event.preventDefault();
    saveLayout();
  } else if ((event.ctrlKey || event.metaKey) && key === "z") {
    event.preventDefault();
    event.shiftKey ? redo() : undo();
  } else if (event.key === "Escape" && state.drawMode) {
    state.drawMode = false;
    updateUiState();
  }
}

function copyRegion(region) {
  return { x: region.x, y: region.y, width: region.width, height: region.height };
}

function clamp(value, minimum, maximum) {
  return Math.max(minimum, Math.min(maximum, value));
}

function showMessage(text, error, duration) {
  window.clearTimeout(state.messageTimer);
  elements.message.textContent = text;
  elements.message.classList.toggle("error", Boolean(error));
  const timeout = duration === undefined ? 5000 : duration;
  if (timeout > 0) {
    state.messageTimer = window.setTimeout(() => {
      elements.message.textContent = "";
      elements.message.classList.remove("error");
    }, timeout);
  }
}

initialize();
