// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

"use strict";

const SVG_NS = "http://www.w3.org/2000/svg";
const LOUPE_SIZE = 768;

const elements = {
  extractType: document.querySelector("#extractType"),
  familySelect: document.querySelector("#familySelect"),
  chartSelect: document.querySelector("#chartSelect"),
  chartCutlineLink: document.querySelector("#chartCutlineLink"),
  previousChart: document.querySelector("#previousChart"),
  nextChart: document.querySelector("#nextChart"),
  candidateScope: document.querySelector("#candidateScope"),
  showAllCharts: document.querySelector("#showAllCharts"),
  undo: document.querySelector("#undo"),
  redo: document.querySelector("#redo"),
  drawRegion: document.querySelector("#drawRegion"),
  finishOutline: document.querySelector("#finishOutline"),
  deleteRegion: document.querySelector("#deleteRegion"),
  moveEarlier: document.querySelector("#moveEarlier"),
  moveLater: document.querySelector("#moveLater"),
  reloadLayout: document.querySelector("#reloadLayout"),
  saveLayout: document.querySelector("#saveLayout"),
  saveState: document.querySelector("#saveState"),
  chartTitle: document.querySelector("#chartTitle"),
  chartFacts: document.querySelector("#chartFacts"),
  regionCount: document.querySelector("#regionCount"),
  overviewViewport: document.querySelector("#overviewViewport"),
  overviewStage: document.querySelector("#overviewStage"),
  overviewImage: document.querySelector("#overviewImage"),
  overviewSvg: document.querySelector("#overviewSvg"),
  regionShapes: document.querySelector("#regionShapes"),
  regionHandles: document.querySelector("#regionHandles"),
  regionTitle: document.querySelector("#regionTitle"),
  regionInputs: document.querySelector("#regionInputs"),
  regionX: document.querySelector("#regionX"),
  regionY: document.querySelector("#regionY"),
  regionWidth: document.querySelector("#regionWidth"),
  regionHeight: document.querySelector("#regionHeight"),
  outputWidthLabel: document.querySelector("#outputWidthLabel"),
  maxOutputWidth: document.querySelector("#maxOutputWidth"),
  navigableControls: document.querySelector("#navigableControls"),
  insetEditMode: document.querySelector("#insetEditMode"),
  insetEditModeButtons: Array.from(document.querySelectorAll(".insetEditModeButton")),
  insetIdentity: document.querySelector("#insetIdentity"),
  boundaryGuide: document.querySelector("#boundaryGuide"),
  boundaryPointList: document.querySelector("#boundaryPointList"),
  insetId: document.querySelector("#insetId"),
  insetEnabled: document.querySelector("#insetEnabled"),
  insetTargetFamily: document.querySelector("#insetTargetFamily"),
  boundaryControls: document.querySelector("#boundaryControls"),
  boundaryPointTitle: document.querySelector("#boundaryPointTitle"),
  boundaryPointX: document.querySelector("#boundaryPointX"),
  boundaryPointY: document.querySelector("#boundaryPointY"),
  addBoundaryPoint: document.querySelector("#addBoundaryPoint"),
  deleteBoundaryPoint: document.querySelector("#deleteBoundaryPoint"),
  snapBoundaryPoint: document.querySelector("#snapBoundaryPoint"),
  fitSummary: document.querySelector("#fitSummary"),
  controlPointSection: document.querySelector("#controlPointSection"),
  georefGuide: document.querySelector("#georefGuide"),
  controlPointList: document.querySelector("#controlPointList"),
  newControlKind: document.querySelector("#newControlKind"),
  regionList: document.querySelector("#regionList"),
  previewFacts: document.querySelector("#previewFacts"),
  previewViewport: document.querySelector("#previewViewport"),
  previewStage: document.querySelector("#previewStage"),
  previewImage: document.querySelector("#previewImage"),
  previewSvg: document.querySelector("#previewSvg"),
  previewBoundary: document.querySelector("#previewBoundary"),
  previewCrosshairH: document.querySelector("#previewCrosshairH"),
  previewCrosshairV: document.querySelector("#previewCrosshairV"),
  previewBoundaryHandle: document.querySelector("#previewBoundaryHandle"),
  previewControlPoints: document.querySelector("#previewControlPoints"),
  previewZoom: document.querySelector("#previewZoom"),
  previewZoomButtons: Array.from(document.querySelectorAll(".previewZoomButton")),
  message: document.querySelector("#message"),
};

const state = {
  extractType: "legend",
  families: [],
  family: null,
  charts: [],
  chart: null,
  regions: [],
  revision: null,
  maxOutputWidth: 1210,
  selectedIndex: -1,
  selectedVertex: 0,
  dirty: false,
  undo: [],
  redo: [],
  drawMode: false,
  draftBoundary: null,
  insetEditMode: "boundary",
  controlPointPickStage: null,
  controlPointEditIndex: null,
  previewAnchor: null,
  overviewBounds: null,
  previewZoom: 1,
  previewCrop: null,
  drag: null,
  messageTimer: null,
  showAllNavigableCharts: false,
  busy: false,
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
    const parameters = new URLSearchParams(window.location.search);
    const requestedType = parameters.get("type");
    const rememberedType = window.localStorage.getItem("aerobag-extract-type");
    state.extractType = [requestedType, rememberedType, "legend"]
      .find((value) => ["legend", "inset", "navigable-inset"].includes(value));
    elements.extractType.value = state.extractType;
    state.showAllNavigableCharts = window.localStorage.getItem(
      "aerobag-show-all-navigable-charts",
    ) === "true";
    elements.showAllCharts.checked = state.showAllNavigableCharts;
    const result = await api("/api/families");
    state.families = result.families;
    for (const family of state.families) {
      const option = document.createElement("option");
      option.value = family.id;
      option.textContent = family.label;
      elements.familySelect.append(option);
    }
    const requested = new URLSearchParams(window.location.search).get("family");
    const remembered = window.localStorage.getItem("aerobag-extract-family");
    const initial = state.extractType === "navigable-inset"
      ? "SEC"
      : [requested, remembered, "TAC", state.families[0].id]
        .find((id) => state.families.some((family) => family.id === id));
    await loadFamily(initial);
  } catch (error) {
    showMessage(error.message, true, 0);
  }
}

function bindControls() {
  elements.extractType.addEventListener("change", async () => {
    if (!canLeaveDirtyChart()) {
      elements.extractType.value = state.extractType;
      return;
    }
    state.extractType = elements.extractType.value;
    window.localStorage.setItem("aerobag-extract-type", state.extractType);
    await loadFamily(state.extractType === "navigable-inset" ? "SEC" : state.family.id);
  });
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
  elements.showAllCharts.addEventListener("change", async () => {
    if (!canLeaveDirtyChart()) {
      elements.showAllCharts.checked = state.showAllNavigableCharts;
      return;
    }
    state.showAllNavigableCharts = elements.showAllCharts.checked;
    window.localStorage.setItem(
      "aerobag-show-all-navigable-charts",
      String(state.showAllNavigableCharts),
    );
    const selected = populateChartSelect(state.chart?.name);
    if (selected && selected !== state.chart?.name) {
      await loadChart(selected);
    }
  });
  elements.undo.addEventListener("click", undo);
  elements.redo.addEventListener("click", redo);
  elements.drawRegion.addEventListener("click", toggleDrawMode);
  elements.finishOutline.addEventListener("click", finishOutline);
  elements.deleteRegion.addEventListener("click", deleteRegion);
  elements.moveEarlier.addEventListener("click", () => moveRegion(-1));
  elements.moveLater.addEventListener("click", () => moveRegion(1));
  elements.reloadLayout.addEventListener("click", reloadLayout);
  elements.saveLayout.addEventListener("click", saveLayout);
  [elements.regionX, elements.regionY, elements.regionWidth, elements.regionHeight]
    .forEach((input) => input.addEventListener("change", updateRegionFromInputs));
  elements.maxOutputWidth.addEventListener("change", updateMaxOutputWidth);
  elements.insetId.addEventListener("change", updateInsetIdentity);
  elements.insetEnabled.addEventListener("change", updateInsetIdentity);
  elements.insetTargetFamily.addEventListener("change", updateInsetIdentity);
  elements.boundaryPointX.addEventListener("change", updateBoundaryPointFromInputs);
  elements.boundaryPointY.addEventListener("change", updateBoundaryPointFromInputs);
  elements.addBoundaryPoint.addEventListener("click", addBoundaryPoint);
  elements.deleteBoundaryPoint.addEventListener("click", deleteBoundaryPoint);
  elements.snapBoundaryPoint.addEventListener("click", snapBoundaryPoint);
  elements.insetEditModeButtons.forEach((button) => {
    button.addEventListener("click", () => setInsetEditMode(button.dataset.mode));
  });
  elements.previewZoomButtons.forEach((button) => {
    button.addEventListener("click", () => setPreviewZoom(Number(button.dataset.zoom)));
  });
  elements.overviewSvg.addEventListener("pointerdown", beginPointerAction);
  elements.overviewImage.addEventListener("error", () => {
    if (elements.overviewImage.hasAttribute("src")) {
      elements.chartFacts.textContent = "Chart overview failed to load. Reload to retry.";
      showMessage("Could not load the chart overview; see the editor server log for details.", true, 0);
    }
  });
  elements.previewSvg.addEventListener("pointerdown", beginPreviewAction);
  window.addEventListener("pointermove", continuePointerAction);
  window.addEventListener("pointerup", endPointerAction);
  window.addEventListener("pointercancel", endPointerAction);
  window.addEventListener("keydown", handleKeyDown);
  window.addEventListener("resize", sizeOverviewStage);
  window.addEventListener("beforeunload", (event) => {
    if (state.dirty || state.draftBoundary?.length) {
      event.preventDefault();
      event.returnValue = "";
    }
  });
}

async function loadFamily(familyId) {
  setBusy(true);
  try {
    const result = await api(
      "/api/charts?family=" + encodeURIComponent(familyId) + "&purpose=extract",
    );
    state.family = state.families.find((family) => family.id === familyId);
    state.charts = result.charts;
    elements.familySelect.value = familyId;
    window.localStorage.setItem("aerobag-extract-family", familyId);
    const parameters = new URLSearchParams(window.location.search);
    const requested = parameters.get("family") === familyId ? parameters.get("chart") : null;
    const remembered = window.localStorage.getItem(
      "aerobag-extract-chart-" + state.extractType + "-" + familyId,
    );
    const preferred = {
      SEC: "Seattle SEC",
      TAC: "Seattle TAC",
      FLY: "Seattle FLY",
      ENR_L: "ENR_L01",
      ENR_H: "ENR_H01",
    }[familyId];
    const availableCharts = visibleCharts();
    const initial = [requested, remembered, preferred, availableCharts[0]?.name]
      .find((name) => availableCharts.some((chart) => chart.name === name));
    if (!initial) {
      throw new Error(state.extractType === "navigable-inset"
        ? "No reviewed navigable-inset candidates are available"
        : "No charts are available for " + state.family.label);
    }
    populateChartSelect(initial);
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
    const layoutUrl = state.extractType === "navigable-inset"
      ? "/api/navigable-insets?" + familyQuery + "&name=" + encodeURIComponent(name)
      : "/api/extract?" + familyQuery
        + "&type=" + encodeURIComponent(state.extractType)
        + "&name=" + encodeURIComponent(name);
    const layout = await api(layoutUrl);
    state.chart = chart;
    state.regions = layout.regions.map(copyRegion);
    state.revision = layout.revision;
    state.maxOutputWidth = layout.max_output_width || state.maxOutputWidth;
    state.selectedIndex = state.regions.length ? 0 : -1;
    state.selectedVertex = 0;
    state.dirty = false;
    state.undo = [];
    state.redo = [];
    state.drawMode = false;
    state.draftBoundary = null;
    const requestedMode = new URLSearchParams(window.location.search).get("mode");
    state.insetEditMode = state.extractType === "navigable-inset"
      && state.selectedIndex >= 0
      && ["boundary", "georef"].includes(requestedMode)
      ? requestedMode
      : "boundary";
    state.controlPointPickStage = null;
    state.controlPointEditIndex = null;
    const selectedRegion = state.regions[0];
    state.previewAnchor = state.insetEditMode === "georef" && selectedRegion
      ? [
          selectedRegion.x + selectedRegion.width / 2,
          selectedRegion.y + selectedRegion.height / 2,
        ]
      : selectedRegion?.boundary?.[0] || null;
    elements.chartSelect.value = name;
    elements.chartCutlineLink.href = "/?" + new URLSearchParams({
      family: state.family.id,
      chart: name,
    });
    elements.maxOutputWidth.value = String(state.maxOutputWidth);
    elements.chartTitle.textContent = chart.name;
    elements.chartFacts.textContent = chart.width + " x " + chart.height + " source pixels";
    configureOverview();
    window.localStorage.setItem(
      "aerobag-extract-chart-" + state.extractType + "-" + state.family.id,
      name,
    );
    const url = new URL(window.location.href);
    url.searchParams.set("family", state.family.id);
    url.searchParams.set("chart", name);
    url.searchParams.set("type", state.extractType);
    if (state.extractType === "navigable-inset") {
      url.searchParams.set("mode", state.insetEditMode);
    } else {
      url.searchParams.delete("mode");
    }
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

function configureOverview() {
  if (!state.chart) {
    return;
  }
  const region = state.regions[state.selectedIndex];
  const georef = state.extractType === "navigable-inset"
    && state.insetEditMode === "georef" && region;
  if (georef) {
    // Calibration marks may lie outside the display cutline, e.g. on its neatline.
    const points = [...region.boundary, ...region.control_points.map((point) => point.pixel)];
    const margin = 64;
    const left = Math.max(0, Math.floor(Math.min(...points.map((p) => p[0]))) - margin);
    const top = Math.max(0, Math.floor(Math.min(...points.map((p) => p[1]))) - margin);
    const right = Math.min(state.chart.width, Math.ceil(Math.max(...points.map((p) => p[0]))) + margin);
    const bottom = Math.min(state.chart.height, Math.ceil(Math.max(...points.map((p) => p[1]))) + margin);
    state.overviewBounds = {
      x: left,
      y: top,
      width: right - left,
      height: bottom - top,
    };
    const bounds = state.overviewBounds;
    elements.overviewImage.src = "/api/crop-overview?family=" + encodeURIComponent(state.family.id)
      + "&name=" + encodeURIComponent(state.chart.name)
      + "&x=" + bounds.x + "&y=" + bounds.y
      + "&width=" + bounds.width + "&height=" + bounds.height
      + "&revision=" + Date.now();
    elements.overviewSvg.setAttribute(
      "viewBox",
      bounds.x + " " + bounds.y + " " + bounds.width + " " + bounds.height,
    );
    elements.overviewStage.classList.add("insetOverview");
    elements.chartFacts.textContent = region.id + " inset, "
      + region.width + " x " + region.height + " source pixels"
      + candidateDescription();
  } else {
    state.overviewBounds = {
      x: 0,
      y: 0,
      width: state.chart.width,
      height: state.chart.height,
    };
    elements.overviewImage.src = state.chart.overview_url;
    elements.overviewSvg.setAttribute(
      "viewBox",
      "0 0 " + state.chart.width + " " + state.chart.height,
    );
    elements.overviewStage.classList.remove("insetOverview");
    elements.chartFacts.textContent = state.chart.width + " x " + state.chart.height
      + " source pixels" + candidateDescription();
  }
  requestAnimationFrame(sizeOverviewStage);
}

function sizeOverviewStage() {
  if (!state.chart || !state.overviewBounds) {
    return;
  }
  if (state.extractType !== "navigable-inset" || state.insetEditMode !== "georef") {
    elements.overviewStage.style.width = "100%";
    elements.overviewStage.style.height = "";
    elements.overviewStage.style.aspectRatio = state.chart.width + " / " + state.chart.height;
    return;
  }
  const availableWidth = Math.max(1, elements.overviewViewport.clientWidth - 28);
  const availableHeight = Math.max(1, elements.overviewViewport.clientHeight - 28);
  const ratio = state.overviewBounds.width / state.overviewBounds.height;
  const width = Math.min(availableWidth, availableHeight * ratio);
  const height = width / ratio;
  elements.overviewStage.style.width = Math.floor(width) + "px";
  elements.overviewStage.style.height = Math.floor(height) + "px";
  elements.overviewStage.style.aspectRatio = state.overviewBounds.width
    + " / " + state.overviewBounds.height;
}

function setInsetEditMode(mode) {
  if (!['boundary', 'georef'].includes(mode)
      || state.extractType !== "navigable-inset"
      || state.draftBoundary !== null
      || (mode === "georef" && state.selectedIndex < 0)
      || mode === state.insetEditMode) {
    return;
  }
  state.insetEditMode = mode;
  state.drawMode = false;
  state.controlPointPickStage = null;
  state.controlPointEditIndex = null;
  const region = state.regions[state.selectedIndex];
  state.previewAnchor = mode === "georef"
    ? [region.x + region.width / 2, region.y + region.height / 2]
    : region?.boundary[state.selectedVertex] || null;
  const url = new URL(window.location.href);
  url.searchParams.set("mode", mode);
  window.history.replaceState(null, "", url);
  configureOverview();
  render();
  updatePreview();
}

function renderOverview() {
  elements.regionShapes.replaceChildren();
  elements.regionHandles.replaceChildren();
  if (!state.chart) {
    return;
  }
  const scale = state.overviewBounds.width / Math.max(elements.overviewStage.clientWidth, 1);
  const handleRadius = Math.max(6 * scale, 15);
  const badgeRadius = Math.max(10 * scale, 24);
  state.regions.forEach((region, index) => {
    const navigable = state.extractType === "navigable-inset";
    const georef = navigable && state.insetEditMode === "georef";
    const shape = document.createElementNS(SVG_NS, navigable ? "polygon" : "rect");
    shape.classList.add("extractRegion");
    if (navigable) {
      shape.classList.add("navigableBoundary");
      shape.classList.toggle("lockedBoundary", georef);
    }
    if (index === state.selectedIndex) {
      shape.classList.add("selected");
    }
    shape.dataset.index = String(index);
    if (navigable) {
      shape.setAttribute("points", pointsAttribute(region.boundary));
    } else {
      setRectAttributes(shape, region);
    }
    elements.regionShapes.append(shape);

    if (!georef && !navigable) {
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
    }

    if (index === state.selectedIndex && !georef && state.draftBoundary === null) {
      const handles = navigable
        ? region.boundary.map((point, vertex) => [String(vertex), point])
        : Object.entries(regionCorners(region));
      for (const [key, point] of handles) {
        const handle = document.createElementNS(SVG_NS, "circle");
        handle.classList.add("vertexHandle");
        if (navigable && Number(key) === state.selectedVertex) {
          handle.classList.add("selected");
        }
        handle.dataset.index = String(index);
        if (navigable) {
          handle.dataset.vertex = key;
        } else {
          handle.dataset.corner = key;
        }
        handle.setAttribute("cx", String(point[0]));
        handle.setAttribute("cy", String(point[1]));
        handle.setAttribute("r", String(handleRadius));
        elements.regionHandles.append(handle);
        if (navigable) {
          appendBoundaryPointLabel(point, Number(key) + 1, handleRadius);
        }
      }
    }
    if (index === state.selectedIndex && navigable && georef) {
      renderControlPointMarkers(
        elements.regionHandles,
        region.control_points,
        (point) => point.pixel,
        Math.max(7 * scale, 18),
        true,
      );
    }
  });
  if (state.draftBoundary !== null) {
    const outline = document.createElementNS(SVG_NS, "polyline");
    outline.classList.add("draftBoundary");
    outline.setAttribute("points", pointsAttribute(state.draftBoundary));
    elements.regionShapes.append(outline);
    state.draftBoundary.forEach((point, index) => {
      const handle = document.createElementNS(SVG_NS, "circle");
      handle.classList.add("vertexHandle");
      handle.dataset.draftVertex = String(index);
      handle.setAttribute("cx", String(point[0]));
      handle.setAttribute("cy", String(point[1]));
      handle.setAttribute("r", String(handleRadius));
      elements.regionHandles.append(handle);
      appendBoundaryPointLabel(point, index + 1, handleRadius);
    });
  }
  elements.regionCount.textContent = state.regions.length
    + (state.extractType === "navigable-inset"
      ? (state.regions.length === 1 ? " inset" : " insets")
      : (state.regions.length === 1 ? " region" : " regions"));
}

function appendBoundaryPointLabel(point, number, radius) {
  const label = document.createElementNS(SVG_NS, "text");
  label.classList.add("boundaryPointLabel");
  label.setAttribute("x", String(point[0]));
  label.setAttribute("y", String(point[1] - radius * 1.7));
  label.setAttribute("font-size", String(radius * 1.8));
  label.textContent = String(number);
  elements.regionHandles.append(label);
}

function renderRegionList() {
  elements.regionList.replaceChildren();
  state.regions.forEach((region, index) => {
    const item = document.createElement("li");
    const button = document.createElement("button");
    button.type = "button";
    button.disabled = state.draftBoundary !== null;
    const prefix = state.extractType === "navigable-inset" ? region.id + " - " : (index + 1) + ": ";
    const suffix = state.extractType === "navigable-inset"
      ? " (" + region.target_family + ", " + region.control_points.length + " controls)"
      : "";
    button.textContent = prefix + region.width + " x " + region.height + suffix;
    button.title = "Region " + (index + 1) + " at " + region.x + ", " + region.y;
    button.classList.toggle("selected", index === state.selectedIndex);
    button.addEventListener("click", () => selectRegion(index, true));
    item.append(button);
    elements.regionList.append(item);
  });
}

function beginPointerAction(event) {
  if (!state.chart || state.busy || event.button !== 0) {
    return;
  }
  const point = constrainedPoint(event.clientX, event.clientY);
  if (state.draftBoundary !== null) {
    event.preventDefault();
    const draftVertex = event.target.closest("[data-draft-vertex]");
    if (draftVertex) {
      if (draftVertex.dataset.draftVertex === "0") {
        finishOutline();
      }
      return;
    }
    state.draftBoundary.push(point);
    render();
    return;
  }
  if (state.extractType === "navigable-inset" && state.insetEditMode === "georef") {
    event.preventDefault();
    const marker = event.target.closest(".controlPointMarker.interactive");
    if (marker) {
      beginControlPointReposition(Number(marker.dataset.controlPointIndex));
      return;
    }
    state.previewAnchor = point;
    state.controlPointPickStage = "loupe";
    updatePreview();
    updateUiState();
    return;
  }
  if (state.drawMode) {
    event.preventDefault();
    pushUndo();
    const region = { x: point[0], y: point[1], width: 1, height: 1 };
    state.regions.push(region);
    state.selectedIndex = state.regions.length - 1;
    state.selectedVertex = 0;
    state.previewAnchor = region.boundary?.[0] || null;
    state.drag = { kind: "draw", pointerId: event.pointerId, start: point, moved: false };
    render();
    return;
  }
  const handle = event.target.closest(".vertexHandle");
  if (handle) {
    event.preventDefault();
    const index = Number(handle.dataset.index);
    state.selectedIndex = index;
    if (state.extractType === "navigable-inset") {
      state.selectedVertex = Number(handle.dataset.vertex);
      state.previewAnchor = state.regions[index].boundary[state.selectedVertex];
    }
    render();
    if (state.extractType === "navigable-inset") {
      updatePreview();
    }
    pushUndo();
    state.drag = {
      kind: state.extractType === "navigable-inset" ? "vertex" : "resize",
      pointerId: event.pointerId,
      startClient: [event.clientX, event.clientY],
      corner: handle.dataset.corner,
      original: copyRegion(state.regions[index]),
      moved: false,
    };
    return;
  }
  const rect = event.target.closest(".extractRegion");
  if (rect) {
    event.preventDefault();
    const index = Number(rect.dataset.index);
    selectRegion(index, true);
    if (state.extractType === "navigable-inset") {
      selectBoundaryPoint(CutlinePoints.nearest(state.regions[index].boundary, point));
      return;
    }
    pushUndo();
    state.drag = {
      kind: "move",
      pointerId: event.pointerId,
      start: point,
      original: copyRegion(state.regions[index]),
      moved: false,
    };
    return;
  }
  if (state.extractType === "navigable-inset") {
    if (state.selectedIndex >= 0) {
      event.preventDefault();
      selectBoundaryPoint(CutlinePoints.nearest(
        state.regions[state.selectedIndex].boundary, point,
      ));
    } else {
      showMessage("Click New inset, then click its corners on the overview.", false);
    }
  }
}

function continuePointerAction(event) {
  if (!state.drag || state.drag.pointerId !== event.pointerId) {
    return;
  }
  if (state.drag.startClient && !state.drag.moved
      && Math.hypot(
        event.clientX - state.drag.startClient[0],
        event.clientY - state.drag.startClient[1],
      ) < 3) {
    return;
  }
  const point = state.drag.kind === "preview-vertex"
    ? previewSourcePoint(event.clientX, event.clientY)
    : constrainedPoint(event.clientX, event.clientY);
  if (state.drag.kind === "draw") {
    const rectangle = rectangleFromPoints(state.drag.start, point);
    state.regions[state.selectedIndex] = withRegionRectangle(
      state.regions[state.selectedIndex], rectangle,
    );
  } else if (state.drag.kind === "vertex" || state.drag.kind === "preview-vertex") {
    const region = state.regions[state.selectedIndex];
    region.boundary[state.selectedVertex] = point;
    state.previewAnchor = point;
    state.regions[state.selectedIndex] = withRegionBoundary(region, region.boundary);
  } else if (state.drag.kind === "resize") {
    state.regions[state.selectedIndex] = withRegionRectangle(
      state.regions[state.selectedIndex],
      resizedRegion(state.drag.original, state.drag.corner, point),
    );
  } else {
    const dx = point[0] - state.drag.start[0];
    const dy = point[1] - state.drag.start[1];
    const original = state.drag.original;
    state.regions[state.selectedIndex] = withRegionRectangle(state.regions[state.selectedIndex], {
      x: clamp(Math.round(original.x + dx), 0, state.chart.width - original.width),
      y: clamp(Math.round(original.y + dy), 0, state.chart.height - original.height),
      width: original.width,
      height: original.height,
    });
  }
  state.drag.moved = true;
  markDirty();
  renderOverview();
  updateInputs();
  if (state.drag.kind === "preview-vertex") {
    renderPreviewGeometry(state.regions[state.selectedIndex]);
  }
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

function pointsAttribute(points) {
  return points.map((point) => point[0] + "," + point[1]).join(" ");
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
  if (state.extractType === "navigable-inset") {
    showMessage("", false, 0);
    if (state.draftBoundary !== null) {
      state.draftBoundary = null;
    } else {
      setInsetEditMode("boundary");
      state.draftBoundary = [];
      state.controlPointPickStage = null;
      state.controlPointEditIndex = null;
    }
    render();
    updatePreview();
    return;
  }
  state.drawMode = !state.drawMode;
  if (state.drawMode) {
    state.controlPointPickStage = null;
    state.controlPointEditIndex = null;
  }
  updateUiState();
}

function finishOutline() {
  if (state.draftBoundary === null || !CutlinePoints.hasArea(state.draftBoundary)) {
    showMessage("An outline needs at least three corners enclosing an area.", false);
    return;
  }
  pushUndo();
  state.regions.push(withRegionBoundary({
    id: nextInsetId(),
    enabled: false,
    target_family: "TAC",
    control_points: [],
    diagnostics: incompleteDiagnostics(),
  }, state.draftBoundary));
  state.draftBoundary = null;
  state.selectedVertex = 0;
  state.dirty = true;
  selectRegion(state.regions.length - 1, true);
}

function selectBoundaryPoint(index) {
  state.selectedVertex = index;
  state.previewAnchor = state.regions[state.selectedIndex].boundary[index];
  render();
  updatePreview();
}

function nextInsetId() {
  const used = new Set(state.regions.map((region) => region.id.toLowerCase()));
  for (let number = 1; ; number += 1) {
    const candidate = "Inset " + number;
    if (!used.has(candidate.toLowerCase())) {
      return candidate;
    }
  }
}

function withRegionRectangle(region, rectangle) {
  return { ...region, ...rectangle };
}

function withRegionBoundary(region, boundary) {
  const xs = boundary.map((point) => point[0]);
  const ys = boundary.map((point) => point[1]);
  const x = Math.floor(Math.min(...xs));
  const y = Math.floor(Math.min(...ys));
  return {
    ...region,
    boundary: boundary.map((point) => [point[0], point[1]]),
    x,
    y,
    width: Math.max(1, Math.ceil(Math.max(...xs)) - x),
    height: Math.max(1, Math.ceil(Math.max(...ys)) - y),
  };
}

function selectRegion(index, refreshPreview) {
  if (index !== state.selectedIndex) {
    state.selectedVertex = 0;
  }
  state.selectedIndex = index;
  const region = state.regions[index];
  state.previewAnchor = state.insetEditMode === "georef" && region
    ? [region.x + region.width / 2, region.y + region.height / 2]
    : region?.boundary?.[state.selectedVertex] || null;
  state.controlPointPickStage = null;
  state.controlPointEditIndex = null;
  configureOverview();
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
  state.selectedVertex = 0;
  state.previewAnchor = state.regions[state.selectedIndex]?.boundary?.[0] || null;
  configureOverview();
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
  if (state.selectedIndex < 0 || state.extractType === "navigable-inset") {
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
  state.regions[state.selectedIndex] = withRegionRectangle(state.regions[state.selectedIndex], {
    x: clamp(rawX, 0, state.chart.width - width),
    y: clamp(rawY, 0, state.chart.height - height),
    width,
    height,
  });
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

function updateInsetIdentity() {
  const region = state.regions[state.selectedIndex];
  if (!region || state.extractType !== "navigable-inset") {
    return;
  }
  const identifier = elements.insetId.value.trim();
  if (!identifier) {
    updateInputs();
    return;
  }
  pushUndo();
  region.id = identifier;
  region.enabled = elements.insetEnabled.checked;
  region.target_family = elements.insetTargetFamily.value;
  markDirty();
  render();
}

function updateBoundaryPointFromInputs() {
  const region = state.regions[state.selectedIndex];
  if (!region || !region.boundary[state.selectedVertex]) {
    return;
  }
  const x = Number(elements.boundaryPointX.value);
  const y = Number(elements.boundaryPointY.value);
  if (!Number.isFinite(x) || !Number.isFinite(y)) {
    updateInputs();
    return;
  }
  pushUndo();
  region.boundary[state.selectedVertex] = [
    clamp(x, 0, state.chart.width),
    clamp(y, 0, state.chart.height),
  ];
  state.previewAnchor = region.boundary[state.selectedVertex];
  state.regions[state.selectedIndex] = withRegionBoundary(region, region.boundary);
  markDirty();
  render();
  updatePreview();
}

function addBoundaryPoint() {
  const region = state.regions[state.selectedIndex];
  if (!region || region.boundary.length < 2) {
    return;
  }
  pushUndo();
  const inserted = CutlinePoints.insertAfter(region.boundary, state.selectedVertex);
  state.regions[state.selectedIndex] = withRegionBoundary(region, inserted.points);
  markDirty();
  selectBoundaryPoint(inserted.selectedIndex);
}

function deleteBoundaryPoint() {
  const region = state.regions[state.selectedIndex];
  if (!region) {
    return;
  }
  const removed = CutlinePoints.remove(region.boundary, state.selectedVertex);
  if (!removed) {
    showMessage("An inset boundary needs at least three points", true);
    return;
  }
  pushUndo();
  state.regions[state.selectedIndex] = withRegionBoundary(region, removed.points);
  markDirty();
  selectBoundaryPoint(removed.selectedIndex);
}

async function snapBoundaryPoint() {
  const region = state.regions[state.selectedIndex];
  if (!region || !region.boundary[state.selectedVertex]) {
    return;
  }
  elements.snapBoundaryPoint.disabled = true;
  try {
    const result = await api("/api/snap", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        family: state.family.id,
        name: state.chart.name,
        point: region.boundary[state.selectedVertex],
        radius: 256,
      }),
    });
    pushUndo();
    region.boundary[state.selectedVertex] = result.point;
    state.previewAnchor = region.boundary[state.selectedVertex];
    state.regions[state.selectedIndex] = withRegionBoundary(region, region.boundary);
    markDirty();
    render();
    updatePreview();
    showMessage(
      "Snapped " + result.distance.toFixed(1) + " px; confidence "
        + Math.round(result.confidence * 100) + "%",
      false,
    );
  } catch (error) {
    showMessage(error.message, true);
  } finally {
    elements.snapBoundaryPoint.disabled = false;
  }
}

function beginPreviewAction(event) {
  if (state.insetEditMode === "boundary"
      && !state.controlPointPickStage
      && event.target === elements.previewBoundaryHandle) {
    event.preventDefault();
    pushUndo();
    state.drag = {
      kind: "preview-vertex", pointerId: event.pointerId, moved: false,
      startClient: [event.clientX, event.clientY],
    };
    return;
  }
  const region = state.regions[state.selectedIndex];
  if (state.controlPointPickStage !== "loupe"
      || !region || state.extractType !== "navigable-inset") {
    return;
  }
  event.preventDefault();
  const sourcePoint = previewSourcePoint(event.clientX, event.clientY);
  pushUndo();
  const pixel = [
    Math.round(sourcePoint[0] * 1000) / 1000,
    Math.round(sourcePoint[1] * 1000) / 1000,
  ];
  const editIndex = state.controlPointEditIndex;
  if (editIndex === null) {
    region.control_points.push({ kind: elements.newControlKind.value, pixel, latitude: null, longitude: null });
  } else {
    region.control_points[editIndex].pixel = pixel;
  }
  region.diagnostics = incompleteDiagnostics();
  state.controlPointPickStage = null;
  state.controlPointEditIndex = null;
  state.previewAnchor = sourcePoint;
  markDirty();
  configureOverview();
  render();
  updatePreview();
  if (editIndex === null) {
    const row = elements.controlPointList.lastElementChild;
    if (row) {
      row.querySelector('[data-field="latitude"]:enabled, [data-field="longitude"]:enabled').focus();
    }
  } else {
    showMessage("Repositioned control point " + (editIndex + 1), false);
  }
}

function renderControlPointMarkers(parent, points, coordinates, radius, interactive = false) {
  (points || []).forEach((point, index) => {
    const [x, y] = coordinates(point);
    const marker = document.createElementNS(SVG_NS, "circle");
    marker.classList.add("controlPointMarker");
    marker.classList.toggle("interactive", interactive);
    marker.classList.toggle("selected", index === state.controlPointEditIndex);
    marker.dataset.controlPointIndex = String(index);
    marker.setAttribute("cx", String(x));
    marker.setAttribute("cy", String(y));
    marker.setAttribute("r", String(radius));
    parent.append(marker);
    const label = document.createElementNS(SVG_NS, "text");
    label.classList.add("controlPointLabel");
    label.setAttribute("x", String(x));
    label.setAttribute("y", String(y));
    label.setAttribute("font-size", String(radius * 1.25));
    label.setAttribute("dominant-baseline", "central");
    label.textContent = String(index + 1);
    parent.append(label);
  });
}

function renderPreviewControlPoints(region) {
  elements.previewControlPoints.replaceChildren();
  if (state.extractType !== "navigable-inset") {
    return;
  }
  renderControlPointMarkers(
    elements.previewControlPoints,
    region.control_points,
    (point) => [
      point.pixel[0] - state.previewCrop.x,
      point.pixel[1] - state.previewCrop.y,
    ],
    12 / state.previewZoom,
  );
}

// Retain rejected input through loupe/selection redraws, and never save its old value silently.
const invalidControlInputs = new WeakMap();
const controlKinds = { intersection: "Lat + lon", latitude: "Latitude only", longitude: "Longitude only" };

function renderControlPointList(region) {
  elements.controlPointList.replaceChildren();
  if (!region) {
    return;
  }
  region.control_points.forEach((point, index) => {
    const row = document.createElement("div");
    row.className = "controlPointRow";
    const number = document.createElement("button");
    number.type = "button";
    number.className = "controlPointNumber";
    number.classList.toggle("selected", index === state.controlPointEditIndex);
    number.textContent = String(index + 1);
    number.title = "Reposition control point " + (index + 1);
    number.setAttribute("aria-label", "Reposition control point " + (index + 1));
    number.addEventListener("click", () => beginControlPointReposition(index));
    row.append(number);
    const kindLabel = document.createElement("label");
    kindLabel.textContent = "Control type";
    const kind = document.createElement("select");
    kind.setAttribute("aria-label", "Control point " + (index + 1) + " type");
    Object.entries(controlKinds).forEach(([value, label]) => kind.add(new Option(label, value)));
    kind.value = point.kind;
    kind.addEventListener("change", () => {
      pushUndo();
      point.kind = kind.value;
      if (point.kind === "latitude") point.longitude = null;
      if (point.kind === "longitude") point.latitude = null;
      const rejected = invalidControlInputs.get(point);
      if (rejected && point.kind === "latitude") delete rejected.longitude;
      if (rejected && point.kind === "longitude") delete rejected.latitude;
      region.diagnostics = incompleteDiagnostics();
      markDirty();
      renderControlPointList(region);
      elements.fitSummary.textContent = region.diagnostics.summary;
    });
    kindLabel.append(kind);
    row.append(kindLabel);
    for (const [field, label, value] of [
      ["x", "Source X", point.pixel[0]],
      ["y", "Source Y", point.pixel[1]],
      ["latitude", "Latitude", point.latitude],
      ["longitude", "Longitude", point.longitude],
    ]) {
      const wrapper = document.createElement("label");
      wrapper.textContent = label;
      const input = document.createElement("input");
      const angular = field === "latitude" || field === "longitude";
      input.type = angular ? "text" : "number";
      if (!angular) input.step = "any";
      input.disabled = angular && point.kind !== "intersection" && point.kind !== field;
      input.placeholder = input.disabled ? "not observed" : angular ? "D or D M" : "";
      input.title = angular ? "Decimal degrees or degrees minutes; west/south use a minus sign." : label;
      const rejected = invalidControlInputs.get(point)?.[field];
      input.value = rejected?.text ?? (value === null ? "" : String(value));
      input.setCustomValidity(rejected?.error ?? "");
      input.dataset.field = field;
      input.addEventListener("change", () => updateControlPoint(index, field, input));
      wrapper.append(input);
      row.append(wrapper);
    }
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "deleteControlPoint";
    remove.title = "Delete control point";
    remove.setAttribute("aria-label", "Delete control point " + (index + 1));
    remove.textContent = "x";
    remove.addEventListener("click", () => deleteControlPoint(index));
    row.append(remove);
    elements.controlPointList.append(row);
  });
}

function beginControlPointReposition(index) {
  const region = state.regions[state.selectedIndex];
  const point = region?.control_points[index];
  if (!point || state.extractType !== "navigable-inset" || state.insetEditMode !== "georef") {
    return;
  }
  state.controlPointEditIndex = index;
  state.controlPointPickStage = "loupe";
  state.previewAnchor = [point.pixel[0], point.pixel[1]];
  renderOverview();
  renderControlPointList(region);
  updateUiState();
  updatePreview();
}

function updateControlPoint(index, field, input) {
  const region = state.regions[state.selectedIndex];
  if (!region || !region.control_points[index]) {
    return;
  }
  const point = region.control_points[index];
  const rejected = invalidControlInputs.get(point) || {};
  let value;
  try {
    if (field === "latitude" || field === "longitude") {
      value = CoordinateInput.parse(input.value, field);
    } else {
      value = Number(input.value);
      const maximum = field === "x" ? state.chart.width : state.chart.height;
      if (!input.value.trim() || !Number.isFinite(value) || value < 0 || value > maximum) {
        throw new Error("Source coordinate must be between 0 and " + maximum + ".");
      }
    }
  } catch (error) {
    rejected[field] = { text: input.value, error: error.message };
    invalidControlInputs.set(point, rejected);
    input.setCustomValidity(error.message);
    showMessage(error.message, true, 0);
    markDirty();
    return;
  }
  delete rejected[field];
  invalidControlInputs.set(point, rejected);
  input.setCustomValidity("");
  input.value = value === null ? "" : String(value);
  pushUndo();
  if (field === "x") {
    region.control_points[index].pixel[0] = value;
  } else if (field === "y") {
    region.control_points[index].pixel[1] = value;
  } else {
    region.control_points[index][field] = value;
  }
  if (state.controlPointEditIndex === index && (field === "x" || field === "y")) {
    state.previewAnchor = [...region.control_points[index].pixel];
  }
  region.diagnostics = incompleteDiagnostics();
  markDirty();
  if (field === "x" || field === "y") {
    configureOverview();
  }
  renderOverview();
  elements.fitSummary.textContent = region.diagnostics.summary;
  updatePreview();
}

function deleteControlPoint(index) {
  const region = state.regions[state.selectedIndex];
  if (!region || !region.control_points[index]) {
    return;
  }
  pushUndo();
  region.control_points.splice(index, 1);
  state.controlPointPickStage = null;
  state.controlPointEditIndex = null;
  state.previewAnchor = [region.x + region.width / 2, region.y + region.height / 2];
  region.diagnostics = incompleteDiagnostics();
  markDirty();
  configureOverview();
  render();
  updatePreview();
}

function incompleteDiagnostics() {
  return {
    ready: false,
    summary: "Save to compute fit diagnostics",
  };
}

function updateInputs() {
  const region = state.regions[state.selectedIndex];
  elements.boundaryPointList.replaceChildren();
  const inputs = [elements.regionX, elements.regionY, elements.regionWidth, elements.regionHeight];
  inputs.forEach((input) => { input.disabled = !region; });
  if (!region) {
    elements.regionTitle.textContent = "No region selected";
    inputs.forEach((input) => { input.value = ""; });
    elements.insetId.value = "";
    elements.insetEnabled.checked = false;
    elements.insetTargetFamily.value = "";
    elements.boundaryPointX.value = "";
    elements.boundaryPointY.value = "";
    renderControlPointList(null);
    return;
  }
  elements.regionTitle.textContent = state.extractType === "navigable-inset"
    ? region.id
    : "Region " + (state.selectedIndex + 1) + " of " + state.regions.length;
  elements.regionX.value = String(region.x);
  elements.regionY.value = String(region.y);
  elements.regionWidth.value = String(region.width);
  elements.regionHeight.value = String(region.height);
  if (state.extractType === "navigable-inset") {
    const point = region.boundary[state.selectedVertex];
    elements.insetId.value = region.id;
    elements.insetEnabled.checked = region.enabled;
    elements.insetTargetFamily.value = region.target_family;
    elements.boundaryPointTitle.textContent = "Boundary point "
      + (state.selectedVertex + 1) + " of " + region.boundary.length;
    elements.boundaryPointX.value = point[0].toFixed(1);
    elements.boundaryPointY.value = point[1].toFixed(1);
    region.boundary.forEach((_, index) => {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = String(index + 1);
      button.title = "Select point " + (index + 1) + " and center the loupe";
      button.classList.toggle("selected", index === state.selectedVertex);
      button.setAttribute("aria-pressed", String(index === state.selectedVertex));
      button.addEventListener("click", () => selectBoundaryPoint(index));
      elements.boundaryPointList.append(button);
    });
    elements.fitSummary.textContent = region.diagnostics?.summary || "Save to compute fit diagnostics";
    renderControlPointList(region);
  }
}

function updatePreview() {
  const region = state.regions[state.selectedIndex];
  if (!state.chart || !region || state.draftBoundary !== null) {
    elements.previewImage.removeAttribute("src");
    elements.previewImage.style.display = "none";
    elements.previewStage.style.width = "0";
    elements.previewStage.style.height = "0";
    elements.previewControlPoints.replaceChildren();
    elements.previewBoundary.setAttribute("points", "");
    elements.previewBoundaryHandle.style.display = "none";
    elements.previewFacts.textContent = "";
    return;
  }
  if (state.extractType === "navigable-inset") {
    elements.previewSvg.hidden = false;
    updateLoupePreview(region);
    return;
  }
  elements.previewSvg.hidden = true;
  state.previewCrop = { x: region.x, y: region.y, width: region.width, height: region.height };
  elements.previewImage.style.display = "block";
  elements.previewImage.style.left = "0";
  elements.previewImage.style.top = "0";
  elements.previewImage.style.width = region.width + "px";
  elements.previewImage.style.height = region.height + "px";
  elements.previewImage.style.right = "auto";
  elements.previewImage.style.bottom = "auto";
  elements.previewStage.style.width = region.width + "px";
  elements.previewStage.style.height = region.height + "px";
  elements.previewSvg.setAttribute("viewBox", "0 0 " + region.width + " " + region.height);
  elements.previewImage.src = "/api/crop?family=" + encodeURIComponent(state.family.id)
    + "&name=" + encodeURIComponent(state.chart.name)
    + "&x=" + region.x + "&y=" + region.y
    + "&width=" + region.width + "&height=" + region.height
    + "&revision=" + Date.now();
  elements.previewFacts.textContent = region.width + " x " + region.height + " source pixels at 1:1";
  renderPreviewControlPoints(region);
  elements.previewViewport.scrollTo(0, 0);
}

function updateLoupePreview(region) {
  const point = state.previewAnchor || region.boundary[state.selectedVertex];
  const x = Math.floor(point[0] - LOUPE_SIZE / 2);
  const y = Math.floor(point[1] - LOUPE_SIZE / 2);
  const source = {
    x: Math.max(0, x),
    y: Math.max(0, y),
  };
  source.width = Math.max(0, Math.min(state.chart.width, x + LOUPE_SIZE) - source.x);
  source.height = Math.max(0, Math.min(state.chart.height, y + LOUPE_SIZE) - source.y);
  state.previewCrop = { x, y, width: LOUPE_SIZE, height: LOUPE_SIZE, source };
  elements.previewImage.style.display = source.width && source.height ? "block" : "none";
  elements.previewSvg.setAttribute("viewBox", "0 0 " + LOUPE_SIZE + " " + LOUPE_SIZE);
  if (source.width && source.height) {
    elements.previewImage.src = "/api/crop?family=" + encodeURIComponent(state.family.id)
      + "&name=" + encodeURIComponent(state.chart.name)
      + "&x=" + source.x + "&y=" + source.y
      + "&width=" + source.width + "&height=" + source.height
      + "&revision=" + Date.now();
  }
  elements.previewFacts.textContent = LOUPE_SIZE + " x " + LOUPE_SIZE
    + " loupe centered at " + point[0].toFixed(1) + ", " + point[1].toFixed(1);
  applyPreviewScale();
  renderPreviewGeometry(region);
  requestAnimationFrame(scrollPreviewToSelectedPoint);
}

function renderPreviewGeometry(region) {
  if (!state.previewCrop || state.extractType !== "navigable-inset") {
    return;
  }
  const selected = state.previewAnchor || region.boundary[state.selectedVertex];
  const local = [selected[0] - state.previewCrop.x, selected[1] - state.previewCrop.y];
  if (state.insetEditMode === "georef") {
    elements.previewBoundary.setAttribute("points", "");
    elements.previewBoundaryHandle.style.display = "none";
    renderPreviewCrosshair(local);
    renderPreviewControlPoints(region);
    return;
  }
  const previous = region.boundary[
    (state.selectedVertex - 1 + region.boundary.length) % region.boundary.length
  ];
  const next = region.boundary[(state.selectedVertex + 1) % region.boundary.length];
  elements.previewBoundary.setAttribute("points", pointsAttribute([
    [previous[0] - state.previewCrop.x, previous[1] - state.previewCrop.y],
    local,
    [next[0] - state.previewCrop.x, next[1] - state.previewCrop.y],
  ]));
  elements.previewBoundaryHandle.style.display = "block";
  elements.previewBoundaryHandle.setAttribute("cx", String(local[0]));
  elements.previewBoundaryHandle.setAttribute("cy", String(local[1]));
  elements.previewBoundaryHandle.setAttribute("r", String(7 / state.previewZoom));
  renderPreviewCrosshair(local);
  renderPreviewControlPoints(region);
}

function renderPreviewCrosshair(local) {
  elements.previewCrosshairH.setAttribute("x1", "0");
  elements.previewCrosshairH.setAttribute("x2", String(LOUPE_SIZE));
  elements.previewCrosshairH.setAttribute("y1", String(local[1]));
  elements.previewCrosshairH.setAttribute("y2", String(local[1]));
  elements.previewCrosshairV.setAttribute("x1", String(local[0]));
  elements.previewCrosshairV.setAttribute("x2", String(local[0]));
  elements.previewCrosshairV.setAttribute("y1", "0");
  elements.previewCrosshairV.setAttribute("y2", String(LOUPE_SIZE));
}

function applyPreviewScale() {
  const crop = state.previewCrop;
  const zoom = state.previewZoom;
  elements.previewStage.style.width = crop.width * zoom + "px";
  elements.previewStage.style.height = crop.height * zoom + "px";
  elements.previewStage.classList.toggle("zoomed", zoom > 1);
  const source = crop.source;
  elements.previewImage.style.left = (source.x - crop.x) * zoom + "px";
  elements.previewImage.style.top = (source.y - crop.y) * zoom + "px";
  elements.previewImage.style.width = source.width * zoom + "px";
  elements.previewImage.style.height = source.height * zoom + "px";
  elements.previewImage.style.right = "auto";
  elements.previewImage.style.bottom = "auto";
  elements.previewZoomButtons.forEach((button) => {
    button.classList.toggle("active", Number(button.dataset.zoom) === zoom);
  });
}

function setPreviewZoom(zoom) {
  state.previewZoom = zoom;
  if (state.extractType === "navigable-inset" && state.previewCrop) {
    applyPreviewScale();
    renderPreviewGeometry(state.regions[state.selectedIndex]);
    requestAnimationFrame(scrollPreviewToSelectedPoint);
  }
}

function scrollPreviewToSelectedPoint() {
  const region = state.regions[state.selectedIndex];
  if (!region || !state.previewCrop) {
    return;
  }
  const point = state.previewAnchor || region.boundary[state.selectedVertex];
  const x = (point[0] - state.previewCrop.x) * state.previewZoom;
  const y = (point[1] - state.previewCrop.y) * state.previewZoom;
  elements.previewViewport.scrollLeft = Math.max(0, x - elements.previewViewport.clientWidth / 2);
  elements.previewViewport.scrollTop = Math.max(0, y - elements.previewViewport.clientHeight / 2);
}

function previewSourcePoint(clientX, clientY) {
  const point = elements.previewSvg.createSVGPoint();
  point.x = clientX;
  point.y = clientY;
  const local = point.matrixTransform(elements.previewSvg.getScreenCTM().inverse());
  return [
    clamp(local.x + state.previewCrop.x, 0, state.chart.width),
    clamp(local.y + state.previewCrop.y, 0, state.chart.height),
  ];
}

function pushUndo() {
  state.undo.push({
    regions: state.regions.map(copyRegion),
    maxOutputWidth: state.maxOutputWidth,
    selectedIndex: state.selectedIndex,
    selectedVertex: state.selectedVertex,
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
  const boundary = state.regions[state.selectedIndex]?.boundary || [];
  state.selectedVertex = Math.min(snapshot.selectedVertex || 0, Math.max(0, boundary.length - 1));
  state.previewAnchor = boundary[state.selectedVertex] || null;
  state.controlPointPickStage = null;
  state.controlPointEditIndex = null;
  state.drawMode = false;
  if (state.selectedIndex < 0 && state.insetEditMode === "georef") {
    state.insetEditMode = "boundary";
    const url = new URL(window.location.href);
    url.searchParams.set("mode", "boundary");
    window.history.replaceState(null, "", url);
  }
  elements.maxOutputWidth.value = String(state.maxOutputWidth);
  markDirty();
  configureOverview();
  render();
  updatePreview();
}

function currentSnapshot() {
  return {
    regions: state.regions.map(copyRegion),
    maxOutputWidth: state.maxOutputWidth,
    selectedIndex: state.selectedIndex,
    selectedVertex: state.selectedVertex,
  };
}

function undo() {
  if (state.draftBoundary !== null) {
    state.draftBoundary.pop();
    render();
    return;
  }
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
  if (!state.chart || !state.dirty || state.draftBoundary !== null) {
    return;
  }
  for (const region of state.regions) {
    if (region.control_points?.some((point) => Object.keys(invalidControlInputs.get(point) || {}).length)) {
      showMessage("Correct invalid coordinates in " + region.id + " before saving.", true, 0);
      return;
    }
  }
  setBusy(true);
  try {
    const navigable = state.extractType === "navigable-inset";
    const result = await api(navigable ? "/api/navigable-insets/save" : "/api/extract/save", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        family: state.family.id,
        type: navigable ? undefined : state.extractType,
        name: state.chart.name,
        regions: state.regions,
        max_output_width: navigable ? undefined : state.maxOutputWidth,
        revision: state.revision,
      }),
    });
    state.regions = result.regions.map(copyRegion);
    state.revision = result.revision;
    state.dirty = false;
    state.undo = [];
    state.redo = [];
    const chartSummary = state.charts.find((chart) => chart.name === state.chart.name);
    if (navigable && chartSummary) {
      chartSummary.navigable_inset_defined_count = state.regions.length;
      chartSummary.navigable_inset_enabled_count = state.regions.filter(
        (region) => region.enabled,
      ).length;
      populateChartSelect(state.chart.name);
    }
    render();
    const suffix = navigable ? ".navigable-insets.json" : "." + state.extractType + ".json";
    showMessage("Saved " + state.chart.name + suffix, false);
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
  const charts = visibleCharts();
  const index = charts.findIndex((chart) => chart.name === state.chart.name);
  const next = (index + direction + charts.length) % charts.length;
  await loadChart(charts[next].name);
}

function visibleCharts() {
  if (state.extractType !== "navigable-inset" || state.showAllNavigableCharts) {
    return state.charts;
  }
  return state.charts.filter((chart) =>
    chart.navigable_inset_candidates.length > 0
      || chart.navigable_inset_defined_count > 0,
  );
}

function populateChartSelect(preferredName) {
  const charts = visibleCharts();
  elements.chartSelect.replaceChildren();
  for (const chart of charts) {
    const option = document.createElement("option");
    option.value = chart.name;
    option.textContent = chartOptionLabel(chart);
    elements.chartSelect.append(option);
  }
  const selected = charts.some((chart) => chart.name === preferredName)
    ? preferredName
    : charts[0]?.name;
  elements.chartSelect.value = selected || "";
  return selected;
}

function chartOptionLabel(chart) {
  if (state.extractType !== "navigable-inset") {
    return chart.name;
  }
  const expected = chart.navigable_inset_candidates.length;
  if (expected === 0) {
    return chart.name + " - unlisted draft";
  }
  return chart.name + " - " + chart.navigable_inset_enabled_count
    + "/" + expected + " enabled";
}

function candidateDescription() {
  if (state.extractType !== "navigable-inset") {
    return "";
  }
  const chart = state.charts.find((candidate) => candidate.name === state.chart.name);
  const names = chart?.navigable_inset_candidates || [];
  return names.length ? "; candidates: " + names.join(", ") : "; unlisted draft";
}

function canLeaveDirtyChart() {
  return (!state.dirty && !state.draftBoundary?.length)
    || window.confirm("Discard unsaved extract-region edits?");
}

function markDirty() {
  state.dirty = true;
  updateUiState();
}

function updateUiState() {
  const drafting = state.draftBoundary !== null;
  if (drafting) {
    elements.regionTitle.textContent = "New inset outline";
  }
  elements.saveState.textContent = drafting ? "Drawing outline" : state.dirty ? "Unsaved" : "Saved";
  elements.saveState.classList.toggle("dirty", state.dirty || drafting);
  elements.saveLayout.disabled = state.busy || !state.dirty || drafting;
  elements.undo.disabled = drafting ? !state.draftBoundary.length : state.undo.length === 0;
  elements.redo.disabled = drafting || state.redo.length === 0;
  const selected = state.selectedIndex >= 0 && !drafting;
  const navigable = state.extractType === "navigable-inset";
  const georef = navigable && state.insetEditMode === "georef";
  elements.candidateScope.hidden = !navigable;
  elements.familySelect.disabled = state.busy || navigable;
  elements.navigableControls.hidden = !navigable;
  elements.regionInputs.hidden = navigable;
  elements.outputWidthLabel.hidden = navigable;
  elements.previewZoom.hidden = !navigable;
  elements.insetEditMode.hidden = !navigable;
  elements.insetEditModeButtons.forEach((button) => {
    button.disabled = drafting || (button.dataset.mode === "georef" && !selected);
    button.title = button.disabled ? "Finish an inset outline first." : "";
    button.classList.toggle("active", button.dataset.mode === state.insetEditMode);
  });
  elements.insetIdentity.hidden = !selected;
  elements.boundaryGuide.hidden = georef;
  elements.boundaryGuide.textContent = drafting
    ? "Click each corner in order around the inset: " + state.draftBoundary.length
      + " points placed. Click Finish outline (or the first point) to close it. Undo removes the last point; Escape cancels."
    : !selected
      ? "Click New inset, then click around its outline on the overview. Finish the outline before refining points or adding georeference coordinates."
      : "Click a numbered corner (or a point button below) to center the loupe. Drag the point to move it. N/P selects next/previous; arrow keys nudge. Insert point adds a midpoint after the selected corner. A closed outline needs at least 3 points; Delete inset removes the whole outline.";
  elements.boundaryControls.hidden = georef || !selected;
  elements.boundaryPointList.hidden = georef || !selected;
  elements.controlPointSection.hidden = !georef;
  elements.deleteRegion.disabled = !selected || georef;
  elements.deleteRegion.hidden = drafting;
  elements.deleteRegion.textContent = navigable ? "Delete inset" : "Delete";
  elements.moveEarlier.hidden = drafting;
  elements.moveLater.hidden = drafting;
  elements.moveEarlier.disabled = !selected || georef || state.selectedIndex === 0;
  elements.moveLater.disabled = !selected || georef
    || state.selectedIndex === state.regions.length - 1;
  elements.drawRegion.disabled = state.busy;
  elements.drawRegion.textContent = navigable
    ? (drafting ? "Cancel outline" : "New inset") : "Draw region";
  elements.drawRegion.title = navigable
    ? (drafting ? "Discard the unfinished outline." : "Click the corners of a new inset on the whole-chart overview.")
    : "Draw a new region on the chart overview";
  elements.finishOutline.hidden = !drafting;
  elements.finishOutline.disabled = state.busy || !drafting
    || !CutlinePoints.hasArea(state.draftBoundary);
  elements.finishOutline.title = elements.finishOutline.disabled
    ? "Place at least three corners enclosing an area." : "Close the outline and begin editing its points.";
  elements.drawRegion.classList.toggle("activeTool", state.drawMode || drafting);
  elements.overviewSvg.classList.toggle("drawMode", state.drawMode || drafting);
  elements.boundaryPointX.disabled = !navigable || !selected || georef;
  elements.boundaryPointY.disabled = !navigable || !selected || georef;
  elements.addBoundaryPoint.disabled = !navigable || !selected || georef;
  elements.addBoundaryPoint.textContent = "Insert point";
  elements.addBoundaryPoint.title = selected
    ? "Insert a midpoint after point " + (state.selectedVertex + 1) + ", then drag it to the desired position."
    : "Select an inset first.";
  elements.deleteBoundaryPoint.disabled = !navigable || !selected || georef
    || state.regions[state.selectedIndex].boundary.length <= 3;
  elements.deleteBoundaryPoint.title = elements.deleteBoundaryPoint.disabled
    ? "A closed outline needs at least three points. Use Delete inset to remove the entire outline."
    : "Remove point " + (state.selectedVertex + 1) + " from this outline.";
  elements.snapBoundaryPoint.disabled = !navigable || !selected || georef;
  elements.georefGuide.textContent = state.controlPointEditIndex !== null
    ? "Repositioning point " + (state.controlPointEditIndex + 1)
      + ": click its corrected position in the loupe."
    : state.controlPointPickStage === "loupe"
      ? "Now click the exact point in the loupe."
      : "Click an approximate point in the inset overview, or select a numbered point to move it.";
  elements.overviewSvg.classList.toggle(
    "pointMode",
    georef,
  );
  elements.previewStage.classList.toggle(
    "pointMode",
    state.controlPointPickStage === "loupe",
  );
}

function setBusy(busy) {
  state.busy = busy;
  elements.extractType.disabled = busy;
  elements.familySelect.disabled = busy || state.extractType === "navigable-inset";
  elements.chartSelect.disabled = busy;
  elements.showAllCharts.disabled = busy;
  elements.drawRegion.disabled = busy;
  elements.reloadLayout.disabled = busy;
  elements.saveLayout.disabled = busy || !state.dirty || state.draftBoundary !== null;
}

function handleKeyDown(event) {
  if (!state.chart) {
    return;
  }
  if (event.key === "Escape" && state.draftBoundary !== null) {
    event.preventDefault();
    toggleDrawMode();
    return;
  }
  if (event.key === "Escape" && (state.drawMode || state.controlPointPickStage)) {
    state.drawMode = false;
    state.controlPointPickStage = null;
    state.controlPointEditIndex = null;
    updateUiState();
    renderOverview();
    renderControlPointList(state.regions[state.selectedIndex]);
    updatePreview();
    return;
  }
  if (event.target.matches("input, select, textarea")) {
    return;
  }
  const key = event.key.toLowerCase();
  const boundary = state.extractType === "navigable-inset"
    && state.insetEditMode === "boundary" && state.draftBoundary === null
    && state.selectedIndex >= 0
    ? state.regions[state.selectedIndex].boundary : null;
  if (!event.ctrlKey && !event.metaKey && !event.altKey && (key === "n" || key === "p")) {
    event.preventDefault();
    if (boundary) {
      const index = (state.selectedVertex + (key === "n" ? 1 : -1) + boundary.length)
        % boundary.length;
      selectBoundaryPoint(index);
    } else if (state.regions.length && state.draftBoundary === null) {
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
  } else if (boundary && !event.ctrlKey && !event.metaKey && !event.altKey) {
    const direction = {
      ArrowLeft: [-1, 0], ArrowRight: [1, 0],
      ArrowUp: [0, -1], ArrowDown: [0, 1],
    }[event.key];
    if (direction) {
      event.preventDefault();
      const moved = CutlinePoints.nudge(
        boundary, state.selectedVertex, direction, event.shiftKey ? 10 : 1,
      );
      moved.points[state.selectedVertex] = [
        clamp(moved.points[state.selectedVertex][0], 0, state.chart.width),
        clamp(moved.points[state.selectedVertex][1], 0, state.chart.height),
      ];
      pushUndo();
      state.regions[state.selectedIndex] = withRegionBoundary(
        state.regions[state.selectedIndex], moved.points,
      );
      markDirty();
      selectBoundaryPoint(state.selectedVertex);
    }
  }
}

function copyRegion(region) {
  return {
    ...region,
    boundary: region.boundary
      ? region.boundary.map((point) => [point[0], point[1]])
      : undefined,
    control_points: (region.control_points || []).map((point) => ({
      kind: point.kind,
      pixel: [point.pixel[0], point.pixel[1]],
      latitude: point.latitude,
      longitude: point.longitude,
    })),
    diagnostics: region.diagnostics ? { ...region.diagnostics } : undefined,
  };
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
