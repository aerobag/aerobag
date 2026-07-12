"use strict";

const SVG_NS = "http://www.w3.org/2000/svg";
const CROP_SIZE = 768;

const elements = {
  chartSelect: document.querySelector("#chartSelect"),
  previousChart: document.querySelector("#previousChart"),
  nextChart: document.querySelector("#nextChart"),
  undo: document.querySelector("#undo"),
  redo: document.querySelector("#redo"),
  addVertex: document.querySelector("#addVertex"),
  deleteVertex: document.querySelector("#deleteVertex"),
  snapVertex: document.querySelector("#snapVertex"),
  reloadChart: document.querySelector("#reloadChart"),
  saveChart: document.querySelector("#saveChart"),
  saveState: document.querySelector("#saveState"),
  chartTitle: document.querySelector("#chartTitle"),
  chartFacts: document.querySelector("#chartFacts"),
  vertexCount: document.querySelector("#vertexCount"),
  overviewStage: document.querySelector("#overviewStage"),
  overviewImage: document.querySelector("#overviewImage"),
  overviewSvg: document.querySelector("#overviewSvg"),
  overviewPolygon: document.querySelector("#overviewPolygon"),
  overviewHandles: document.querySelector("#overviewHandles"),
  vertexTitle: document.querySelector("#vertexTitle"),
  pointX: document.querySelector("#pointX"),
  pointY: document.querySelector("#pointY"),
  loupeViewport: document.querySelector("#loupeViewport"),
  loupeStage: document.querySelector("#loupeStage"),
  loupeImage: document.querySelector("#loupeImage"),
  loupeSvg: document.querySelector("#loupeSvg"),
  loupeLine: document.querySelector("#loupeLine"),
  loupeHandle: document.querySelector("#loupeHandle"),
  crosshairH: document.querySelector("#crosshairH"),
  crosshairV: document.querySelector("#crosshairV"),
  cropFacts: document.querySelector("#cropFacts"),
  message: document.querySelector("#message"),
  zoomButtons: Array.from(document.querySelectorAll(".zoomButton")),
};

const state = {
  charts: [],
  chart: null,
  points: [],
  revision: "",
  selectedIndex: 0,
  dirty: false,
  undo: [],
  redo: [],
  drag: null,
  zoom: 1,
  overviewBounds: null,
  crop: { x: 0, y: 0, width: CROP_SIZE, height: CROP_SIZE },
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
    const result = await api("/api/charts");
    state.charts = result.charts;
    elements.chartSelect.replaceChildren();
    for (const chart of state.charts) {
      const option = document.createElement("option");
      option.value = chart.name;
      option.textContent = chart.name;
      elements.chartSelect.append(option);
    }
    const remembered = window.localStorage.getItem("aerobag-cutline-chart");
    const initial = state.charts.some((chart) => chart.name === remembered)
      ? remembered
      : state.charts[0].name;
    await loadChart(initial);
  } catch (error) {
    showMessage(error.message, true, 0);
  }
}

function bindControls() {
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
  elements.addVertex.addEventListener("click", addVertex);
  elements.deleteVertex.addEventListener("click", deleteVertex);
  elements.snapVertex.addEventListener("click", snapVertex);
  elements.reloadChart.addEventListener("click", reloadChart);
  elements.saveChart.addEventListener("click", saveChart);
  elements.pointX.addEventListener("change", updatePointFromInputs);
  elements.pointY.addEventListener("change", updatePointFromInputs);
  elements.zoomButtons.forEach((button) => {
    button.addEventListener("click", () => setZoom(Number(button.dataset.zoom)));
  });
  elements.overviewSvg.addEventListener("pointerdown", beginOverviewDrag);
  elements.loupeSvg.addEventListener("pointerdown", beginLoupeDrag);
  window.addEventListener("pointermove", continueDrag);
  window.addEventListener("pointerup", endDrag);
  window.addEventListener("pointercancel", endDrag);
  window.addEventListener("beforeunload", (event) => {
    if (state.dirty) {
      event.preventDefault();
      event.returnValue = "";
    }
  });
  window.addEventListener("keydown", handleKeyDown);
}

async function loadChart(name) {
  setBusy(true);
  try {
    const chart = await api("/api/chart?name=" + encodeURIComponent(name));
    state.chart = chart;
    state.points = chart.points.map((point) => [point[0], point[1]]);
    state.revision = chart.revision;
    state.selectedIndex = 0;
    state.dirty = false;
    state.undo = [];
    state.redo = [];
    elements.chartSelect.value = name;
    window.localStorage.setItem("aerobag-cutline-chart", name);
    elements.overviewImage.src = chart.overview_url;
    configureOverviewBounds();
    elements.chartTitle.textContent = chart.name;
    elements.chartFacts.textContent = chart.width + " x " + chart.height + " source pixels";
    renderOverview();
    await centerLoupe();
    updateUiState();
  } catch (error) {
    showMessage(error.message, true, 0);
  } finally {
    setBusy(false);
  }
}

function canLeaveDirtyChart() {
  return !state.dirty || window.confirm("Discard unsaved cutline edits?");
}

async function moveChart(direction) {
  if (!state.chart || !canLeaveDirtyChart()) {
    return;
  }
  const index = state.charts.findIndex((chart) => chart.name === state.chart.name);
  const next = (index + direction + state.charts.length) % state.charts.length;
  await loadChart(state.charts[next].name);
}

function renderOverview() {
  if (!state.chart) {
    return;
  }
  elements.overviewPolygon.setAttribute("points", pointsAttribute(state.points));
  elements.overviewHandles.replaceChildren();
  const scale = state.overviewBounds.width / Math.max(elements.overviewStage.clientWidth, 1);
  const radius = Math.max(6 * scale, 18);
  state.points.forEach((point, index) => {
    const circle = document.createElementNS(SVG_NS, "circle");
    circle.classList.add("vertexHandle");
    if (index === state.selectedIndex) {
      circle.classList.add("selected");
    }
    circle.dataset.index = String(index);
    circle.setAttribute("cx", String(point[0]));
    circle.setAttribute("cy", String(point[1]));
    circle.setAttribute("r", String(radius));
    elements.overviewHandles.append(circle);
  });
  elements.vertexCount.textContent = state.points.length + " points";
  renderLoupeOverlay();
  updateCoordinateInputs();
  updateUiState();
}

function renderLoupeOverlay() {
  if (!state.chart || !state.points.length) {
    return;
  }
  const selected = state.points[state.selectedIndex];
  const local = [selected[0] - state.crop.x, selected[1] - state.crop.y];
  const previous = state.points[(state.selectedIndex - 1 + state.points.length) % state.points.length];
  const next = state.points[(state.selectedIndex + 1) % state.points.length];
  const localPrevious = [previous[0] - state.crop.x, previous[1] - state.crop.y];
  const localNext = [next[0] - state.crop.x, next[1] - state.crop.y];
  elements.loupeLine.setAttribute("points", pointsAttribute([localPrevious, local, localNext]));
  elements.loupeHandle.setAttribute("cx", String(local[0]));
  elements.loupeHandle.setAttribute("cy", String(local[1]));
  elements.loupeHandle.setAttribute("r", String(7 / state.zoom));
  elements.crosshairH.setAttribute("x1", "0");
  elements.crosshairH.setAttribute("x2", String(state.crop.width));
  elements.crosshairH.setAttribute("y1", String(local[1]));
  elements.crosshairH.setAttribute("y2", String(local[1]));
  elements.crosshairV.setAttribute("x1", String(local[0]));
  elements.crosshairV.setAttribute("x2", String(local[0]));
  elements.crosshairV.setAttribute("y1", "0");
  elements.crosshairV.setAttribute("y2", String(state.crop.height));
  elements.vertexTitle.textContent = "Point " + (state.selectedIndex + 1) + " of " + state.points.length;
}

async function centerLoupe() {
  if (!state.chart || !state.points.length) {
    return;
  }
  const point = state.points[state.selectedIndex];
  const width = CROP_SIZE;
  const height = CROP_SIZE;
  const x = Math.floor(point[0] - width / 2);
  const y = Math.floor(point[1] - height / 2);
  const sourceX = Math.max(0, x);
  const sourceY = Math.max(0, y);
  const sourceRight = Math.min(state.chart.width, x + width);
  const sourceBottom = Math.min(state.chart.height, y + height);
  const source = {
    x: sourceX,
    y: sourceY,
    width: Math.max(0, sourceRight - sourceX),
    height: Math.max(0, sourceBottom - sourceY),
  };
  state.crop = { x, y, width, height, source };
  elements.loupeSvg.setAttribute("viewBox", "0 0 " + width + " " + height);
  if (source.width > 0 && source.height > 0) {
    elements.loupeImage.style.display = "block";
    elements.loupeImage.src = "/api/crop?name=" + encodeURIComponent(state.chart.name)
      + "&x=" + source.x + "&y=" + source.y
      + "&width=" + source.width + "&height=" + source.height;
  } else {
    elements.loupeImage.removeAttribute("src");
    elements.loupeImage.style.display = "none";
  }
  elements.cropFacts.textContent = width + " x " + height + " canvas at " + x + ", " + y;
  applyLoupeScale();
  renderLoupeOverlay();
  if (source.width > 0 && source.height > 0) {
    await waitForImage(elements.loupeImage);
  }
  requestAnimationFrame(scrollLoupeToPoint);
}

function applyLoupeScale() {
  const displayWidth = state.crop.width * state.zoom;
  const displayHeight = state.crop.height * state.zoom;
  elements.loupeStage.style.width = displayWidth + "px";
  elements.loupeStage.style.height = displayHeight + "px";
  elements.loupeStage.classList.toggle("zoomed", state.zoom > 1);
  positionLoupeImage();
  elements.zoomButtons.forEach((button) => {
    button.classList.toggle("active", Number(button.dataset.zoom) === state.zoom);
  });
}

function positionLoupeImage() {
  const source = state.crop.source;
  if (!source) {
    return;
  }
  elements.loupeImage.style.left = (source.x - state.crop.x) * state.zoom + "px";
  elements.loupeImage.style.top = (source.y - state.crop.y) * state.zoom + "px";
  elements.loupeImage.style.width = source.width * state.zoom + "px";
  elements.loupeImage.style.height = source.height * state.zoom + "px";
  elements.loupeImage.style.right = "auto";
  elements.loupeImage.style.bottom = "auto";
}

function configureOverviewBounds() {
  const padding = Math.max(state.chart.width, state.chart.height) * 0.02;
  const xs = state.points.map((point) => point[0]);
  const ys = state.points.map((point) => point[1]);
  const left = Math.min(0, ...xs) - padding;
  const top = Math.min(0, ...ys) - padding;
  const right = Math.max(state.chart.width, ...xs) + padding;
  const bottom = Math.max(state.chart.height, ...ys) + padding;
  const width = right - left;
  const height = bottom - top;
  state.overviewBounds = { left, top, width, height };
  elements.overviewStage.style.aspectRatio = width + " / " + height;
  elements.overviewSvg.setAttribute("viewBox", left + " " + top + " " + width + " " + height);
  elements.overviewImage.style.left = ((-left / width) * 100) + "%";
  elements.overviewImage.style.top = ((-top / height) * 100) + "%";
  elements.overviewImage.style.width = ((state.chart.width / width) * 100) + "%";
  elements.overviewImage.style.height = ((state.chart.height / height) * 100) + "%";
  elements.overviewImage.style.right = "auto";
  elements.overviewImage.style.bottom = "auto";
}

function setZoom(zoom) {
  state.zoom = zoom;
  applyLoupeScale();
  renderLoupeOverlay();
  requestAnimationFrame(scrollLoupeToPoint);
}

function scrollLoupeToPoint() {
  if (!state.points.length) {
    return;
  }
  const point = state.points[state.selectedIndex];
  const localX = (point[0] - state.crop.x) * state.zoom;
  const localY = (point[1] - state.crop.y) * state.zoom;
  elements.loupeViewport.scrollLeft = Math.max(0, localX - elements.loupeViewport.clientWidth / 2);
  elements.loupeViewport.scrollTop = Math.max(0, localY - elements.loupeViewport.clientHeight / 2);
}

function beginOverviewDrag(event) {
  const handle = event.target.closest(".vertexHandle");
  if (!handle) {
    return;
  }
  const index = Number(handle.dataset.index);
  const selectionChanged = index !== state.selectedIndex;
  selectVertex(index, false);
  startDrag("overview", event, selectionChanged);
}

function beginLoupeDrag(event) {
  if (event.target !== elements.loupeHandle) {
    return;
  }
  startDrag("loupe", event, false);
}

function startDrag(surface, event, selectionChanged) {
  event.preventDefault();
  pushUndo();
  state.drag = { surface, pointerId: event.pointerId, moved: false, selectionChanged };
}

function continueDrag(event) {
  if (!state.drag || state.drag.pointerId !== event.pointerId) {
    return;
  }
  const svg = state.drag.surface === "overview" ? elements.overviewSvg : elements.loupeSvg;
  const local = svgPoint(svg, event.clientX, event.clientY);
  const point = state.drag.surface === "overview"
    ? [local.x, local.y]
    : [local.x + state.crop.x, local.y + state.crop.y];
  state.points[state.selectedIndex] = point;
  state.drag.moved = true;
  markDirty();
  renderOverview();
}

async function endDrag(event) {
  if (!state.drag || state.drag.pointerId !== event.pointerId) {
    return;
  }
  const moved = state.drag.moved;
  const selectionChanged = state.drag.selectionChanged;
  state.drag = null;
  if (!moved) {
    state.undo.pop();
    updateUiState();
    if (selectionChanged) {
      await centerLoupe();
    }
    return;
  }
  if (selectionChanged || !pointInsideCrop(state.points[state.selectedIndex], 70)) {
    await centerLoupe();
  }
}

function pointInsideCrop(point, margin) {
  return point[0] >= state.crop.x + margin
    && point[1] >= state.crop.y + margin
    && point[0] <= state.crop.x + state.crop.width - margin
    && point[1] <= state.crop.y + state.crop.height - margin;
}

async function selectVertex(index, recenter) {
  state.selectedIndex = index;
  renderOverview();
  if (recenter) {
    await centerLoupe();
  }
}

function addVertex() {
  if (!state.points.length) {
    return;
  }
  pushUndo();
  const index = state.selectedIndex;
  const nextIndex = (index + 1) % state.points.length;
  const point = state.points[index];
  const next = state.points[nextIndex];
  state.points.splice(index + 1, 0, [(point[0] + next[0]) / 2, (point[1] + next[1]) / 2]);
  state.selectedIndex = index + 1;
  markDirty();
  renderOverview();
  centerLoupe();
}

function deleteVertex() {
  if (state.points.length <= 3) {
    showMessage("A cutline needs at least three points", true);
    return;
  }
  pushUndo();
  state.points.splice(state.selectedIndex, 1);
  state.selectedIndex = Math.min(state.selectedIndex, state.points.length - 1);
  markDirty();
  renderOverview();
  centerLoupe();
}

async function snapVertex() {
  if (!state.chart || !state.points.length) {
    return;
  }
  elements.snapVertex.disabled = true;
  try {
    const point = state.points[state.selectedIndex];
    const result = await api("/api/snap", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name: state.chart.name, point, radius: 256 }),
    });
    pushUndo();
    state.points[state.selectedIndex] = result.point;
    markDirty();
    renderOverview();
    await centerLoupe();
    showMessage(
      "Snapped " + result.distance.toFixed(1) + " px; confidence " + Math.round(result.confidence * 100) + "%",
      false,
    );
  } catch (error) {
    showMessage(error.message, true);
  } finally {
    elements.snapVertex.disabled = false;
  }
}

function updatePointFromInputs() {
  const x = Number(elements.pointX.value);
  const y = Number(elements.pointY.value);
  if (!Number.isFinite(x) || !Number.isFinite(y)) {
    updateCoordinateInputs();
    return;
  }
  pushUndo();
  state.points[state.selectedIndex] = [x, y];
  markDirty();
  renderOverview();
  centerLoupe();
}

function updateCoordinateInputs() {
  if (!state.points.length) {
    return;
  }
  const point = state.points[state.selectedIndex];
  elements.pointX.value = point[0].toFixed(1);
  elements.pointY.value = point[1].toFixed(1);
}

function pushUndo() {
  state.undo.push(clonePoints(state.points));
  if (state.undo.length > 100) {
    state.undo.shift();
  }
  state.redo = [];
}

function undo() {
  if (!state.undo.length) {
    return;
  }
  state.redo.push(clonePoints(state.points));
  state.points = state.undo.pop();
  state.selectedIndex = Math.min(state.selectedIndex, state.points.length - 1);
  markDirty();
  renderOverview();
  centerLoupe();
}

function redo() {
  if (!state.redo.length) {
    return;
  }
  state.undo.push(clonePoints(state.points));
  state.points = state.redo.pop();
  state.selectedIndex = Math.min(state.selectedIndex, state.points.length - 1);
  markDirty();
  renderOverview();
  centerLoupe();
}

async function saveChart() {
  if (!state.chart || !state.dirty) {
    return;
  }
  setBusy(true);
  try {
    const result = await api("/api/save", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: state.chart.name,
        points: state.points,
        revision: state.revision,
      }),
    });
    state.revision = result.revision;
    state.dirty = false;
    state.undo = [];
    state.redo = [];
    updateUiState();
    showMessage("Saved " + state.chart.cutline_file, false);
  } catch (error) {
    showMessage(error.message, true, 0);
  } finally {
    setBusy(false);
  }
}

async function reloadChart() {
  if (!state.chart || !canLeaveDirtyChart()) {
    return;
  }
  await loadChart(state.chart.name);
}

function markDirty() {
  state.dirty = true;
  updateUiState();
}

function updateUiState() {
  elements.saveState.textContent = state.dirty ? "Unsaved" : "Saved";
  elements.saveState.classList.toggle("dirty", state.dirty);
  elements.saveChart.disabled = !state.dirty;
  elements.undo.disabled = state.undo.length === 0;
  elements.redo.disabled = state.redo.length === 0;
  elements.deleteVertex.disabled = state.points.length <= 3;
  elements.previousChart.disabled = state.charts.length < 2;
  elements.nextChart.disabled = state.charts.length < 2;
}

function setBusy(busy) {
  elements.chartSelect.disabled = busy;
  elements.saveChart.disabled = busy || !state.dirty;
  elements.reloadChart.disabled = busy;
}

function handleKeyDown(event) {
  if (!state.chart || event.target.matches("input, select, button")) {
    return;
  }
  const key = event.key.toLowerCase();
  if (!event.ctrlKey && !event.metaKey && !event.altKey && (key === "n" || key === "p")) {
    event.preventDefault();
    const direction = key === "n" ? 1 : -1;
    const index = (
      state.selectedIndex + direction + state.points.length
    ) % state.points.length;
    selectVertex(index, true);
    return;
  }
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
    event.preventDefault();
    saveChart();
    return;
  }
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "z") {
    event.preventDefault();
    event.shiftKey ? redo() : undo();
    return;
  }
  const movement = {
    ArrowLeft: [-1, 0],
    ArrowRight: [1, 0],
    ArrowUp: [0, -1],
    ArrowDown: [0, 1],
  }[event.key];
  if (movement) {
    event.preventDefault();
    pushUndo();
    const multiplier = event.shiftKey ? 10 : 1;
    const point = state.points[state.selectedIndex];
    state.points[state.selectedIndex] = [
      point[0] + movement[0] * multiplier,
      point[1] + movement[1] * multiplier,
    ];
    markDirty();
    renderOverview();
  }
}

function svgPoint(svg, clientX, clientY) {
  const point = svg.createSVGPoint();
  point.x = clientX;
  point.y = clientY;
  return point.matrixTransform(svg.getScreenCTM().inverse());
}

function pointsAttribute(points) {
  return points.map((point) => point[0] + "," + point[1]).join(" ");
}

function clonePoints(points) {
  return points.map((point) => [point[0], point[1]]);
}

function clamp(value, minimum, maximum) {
  return Math.max(minimum, Math.min(maximum, value));
}

function waitForImage(image) {
  if (image.complete && image.naturalWidth) {
    return Promise.resolve();
  }
  return new Promise((resolve, reject) => {
    image.addEventListener("load", resolve, { once: true });
    image.addEventListener("error", () => reject(new Error("Failed to load chart crop")), { once: true });
  });
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
