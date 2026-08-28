"use strict";

const PRESETS = {
  grove: {
    label: "Grove",
    src: "../grove-map-target-site-crop.png",
  },
  murphy: {
    label: "Murphy Farm",
    src: "../map-sources/murphy-farm-z17-map-crop.png",
  },
  kilteevan: {
    label: "Kilteevan",
    src: "../map-sources/kilteevan-z17-map-crop.png",
  },
};

const CATEGORIES = [
  { id: "structure", label: "Structure", color: "#cf3f2e" },
  { id: "road", label: "Road", color: "#d18a22" },
  { id: "unfenced_path", label: "Unfenced path / track", color: "#f3c84c" },
  { id: "admin_boundary", label: "Administrative boundary", color: "#8b66d9" },
  { id: "physical_boundary", label: "Physical boundary", color: "#7b5135" },
  { id: "hedge_bank_ditch", label: "Hedge / bank / ditch", color: "#4c8b3a" },
  { id: "dry_stone_wall", label: "Dry stone wall", color: "#68717a" },
  { id: "deciduous_tree", label: "Deciduous tree", color: "#238642" },
  { id: "coniferous_tree", label: "Coniferous tree", color: "#0d5946" },
  { id: "orchard_crops", label: "Orchard / crops", color: "#86aa2f" },
  { id: "rough_vegetation_bog", label: "Rough vegetation / bog", color: "#8c7d39" },
  { id: "water_wet_ground", label: "Water / wet ground", color: "#2e82c4" },
  { id: "printed_label", label: "Printed label", color: "#2f2f2f" },
  { id: "ignore_not_physical", label: "Ignore / not physical", color: "#b2a9a0" },
  { id: "uncertain", label: "Uncertain", color: "#e05299" },
];

const TOOL_CURSOR = {
  select: "default",
  point: "crosshair",
  line: "crosshair",
  polygon: "crosshair",
  box: "crosshair",
  pan: "grab",
};

const state = {
  image: null,
  imageInfo: null,
  annotations: [],
  selectedId: null,
  currentTool: "select",
  currentCategory: "structure",
  draft: null,
  view: { scale: 1, x: 0, y: 0 },
  history: [],
  interaction: null,
  editorHistoryToken: null,
  spacePanning: false,
  showLabels: true,
  showIndex: true,
  dimImage: false,
};

const els = {};

document.addEventListener("DOMContentLoaded", () => {
  bindElements();
  buildCategoryControls();
  buildEditorCategoryOptions();
  bindEvents();
  resizeCanvas();
  render();
});

function bindElements() {
  Object.assign(els, {
    canvas: document.getElementById("mapCanvas"),
    canvasWrap: document.getElementById("canvasWrap"),
    emptyState: document.getElementById("emptyState"),
    imageMeta: document.getElementById("imageMeta"),
    categoryList: document.getElementById("categoryList"),
    toolButtons: document.getElementById("toolButtons"),
    imageInput: document.getElementById("imageInput"),
    jsonInput: document.getElementById("jsonInput"),
    loadGrove: document.getElementById("loadGrove"),
    loadMurphy: document.getElementById("loadMurphy"),
    loadKilteevan: document.getElementById("loadKilteevan"),
    exportJson: document.getElementById("exportJson"),
    exportPng: document.getElementById("exportPng"),
    undo: document.getElementById("undo"),
    deleteSelected: document.getElementById("deleteSelected"),
    finishShape: document.getElementById("finishShape"),
    cancelDraft: document.getElementById("cancelDraft"),
    zoomIn: document.getElementById("zoomIn"),
    zoomOut: document.getElementById("zoomOut"),
    fitImage: document.getElementById("fitImage"),
    resetView: document.getElementById("resetView"),
    clearAll: document.getElementById("clearAll"),
    showLabels: document.getElementById("showLabels"),
    showIndex: document.getElementById("showIndex"),
    dimImage: document.getElementById("dimImage"),
    selectionEmpty: document.getElementById("selectionEmpty"),
    editor: document.getElementById("editor"),
    editCategory: document.getElementById("editCategory"),
    editLabel: document.getElementById("editLabel"),
    editConfidence: document.getElementById("editConfidence"),
    editNotes: document.getElementById("editNotes"),
    annotationCount: document.getElementById("annotationCount"),
    annotationList: document.getElementById("annotationList"),
  });
  els.ctx = els.canvas.getContext("2d");
}

function bindEvents() {
  window.addEventListener("resize", () => {
    resizeCanvas();
    render();
  });

  els.loadGrove.addEventListener("click", () => loadPreset("grove"));
  els.loadMurphy.addEventListener("click", () => loadPreset("murphy"));
  els.loadKilteevan.addEventListener("click", () => loadPreset("kilteevan"));

  els.imageInput.addEventListener("change", event => {
    const file = event.target.files?.[0];
    if (!file) return;
    const url = URL.createObjectURL(file);
    loadImage(url, file.name, { revokeUrl: url });
  });

  els.jsonInput.addEventListener("change", event => {
    const file = event.target.files?.[0];
    if (!file) return;
    importJson(file);
  });

  els.toolButtons.addEventListener("click", event => {
    const button = event.target.closest("[data-tool]");
    if (!button) return;
    setTool(button.dataset.tool);
  });

  els.canvas.addEventListener("pointerdown", onPointerDown);
  els.canvas.addEventListener("pointermove", onPointerMove);
  els.canvas.addEventListener("pointerup", onPointerUp);
  els.canvas.addEventListener("pointercancel", onPointerUp);
  els.canvas.addEventListener("dblclick", event => {
    if (state.draft && (state.draft.type === "line" || state.draft.type === "polygon")) {
      event.preventDefault();
      finishDraft();
    }
  });

  els.canvas.addEventListener("wheel", event => {
    if (!state.image) return;
    event.preventDefault();
    const rect = els.canvas.getBoundingClientRect();
    const canvasPoint = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    zoomAt(canvasPoint, event.deltaY < 0 ? 1.12 : 1 / 1.12);
  }, { passive: false });

  document.addEventListener("keydown", event => {
    if (isTypingInEditor(event.target)) return;
    if (event.key === " ") {
      state.spacePanning = true;
      updateCanvasCursor();
    } else if (event.key === "Enter") {
      finishDraft();
    } else if (event.key === "Escape") {
      cancelDraft();
    } else if (event.key === "Backspace" || event.key === "Delete") {
      deleteSelected();
    } else if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "z") {
      undo();
    }
  });

  document.addEventListener("keyup", event => {
    if (event.key === " ") {
      state.spacePanning = false;
      updateCanvasCursor();
    }
  });

  els.exportJson.addEventListener("click", exportJson);
  els.exportPng.addEventListener("click", exportPng);
  els.undo.addEventListener("click", undo);
  els.deleteSelected.addEventListener("click", deleteSelected);
  els.finishShape.addEventListener("click", finishDraft);
  els.cancelDraft.addEventListener("click", cancelDraft);
  els.zoomIn.addEventListener("click", () => zoomAt(centerOfCanvas(), 1.2));
  els.zoomOut.addEventListener("click", () => zoomAt(centerOfCanvas(), 1 / 1.2));
  els.fitImage.addEventListener("click", fitImage);
  els.resetView.addEventListener("click", resetView);
  els.clearAll.addEventListener("click", clearAll);

  els.showLabels.addEventListener("change", () => {
    state.showLabels = els.showLabels.checked;
    render();
  });
  els.showIndex.addEventListener("change", () => {
    state.showIndex = els.showIndex.checked;
    render();
  });
  els.dimImage.addEventListener("change", () => {
    state.dimImage = els.dimImage.checked;
    render();
  });

  els.editCategory.addEventListener("change", () => patchSelected({ category: els.editCategory.value }));
  els.editLabel.addEventListener("input", () => patchSelected({ label: els.editLabel.value }));
  els.editConfidence.addEventListener("change", () => patchSelected({ confidence: els.editConfidence.value }));
  els.editNotes.addEventListener("input", () => patchSelected({ notes: els.editNotes.value }));
  [els.editCategory, els.editLabel, els.editConfidence, els.editNotes].forEach(field => {
    field.addEventListener("focus", () => rememberEditorUndo(field.id));
  });

  els.annotationList.addEventListener("click", event => {
    const item = event.target.closest("[data-id]");
    if (!item) return;
    selectAnnotation(item.dataset.id);
  });
}

function buildCategoryControls() {
  els.categoryList.innerHTML = "";
  for (const category of CATEGORIES) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `category-button${category.id === state.currentCategory ? " active" : ""}`;
    button.dataset.category = category.id;
    button.innerHTML = `<span class="swatch" style="background:${category.color}"></span><span>${escapeHtml(category.label)}</span>`;
    button.addEventListener("click", () => {
      state.currentCategory = category.id;
      updateCategoryControls();
    });
    els.categoryList.append(button);
  }
}

function buildEditorCategoryOptions() {
  els.editCategory.innerHTML = "";
  for (const category of CATEGORIES) {
    const option = document.createElement("option");
    option.value = category.id;
    option.textContent = category.label;
    els.editCategory.append(option);
  }
}

function updateCategoryControls() {
  els.categoryList.querySelectorAll("[data-category]").forEach(button => {
    button.classList.toggle("active", button.dataset.category === state.currentCategory);
  });
}

function setTool(tool) {
  state.currentTool = tool;
  cancelDraft(false);
  els.toolButtons.querySelectorAll("[data-tool]").forEach(button => {
    button.classList.toggle("active", button.dataset.tool === tool);
  });
  updateCanvasCursor();
}

function loadPreset(key) {
  const preset = PRESETS[key];
  loadImage(preset.src, preset.label);
}

function loadImage(src, name, options = {}) {
  const img = new Image();
  img.onload = () => {
    if (options.revokeUrl) {
      URL.revokeObjectURL(options.revokeUrl);
    }
    state.image = img;
    state.imageInfo = {
      name,
      src,
      width: img.naturalWidth,
      height: img.naturalHeight,
      loadedAt: new Date().toISOString(),
    };
    state.annotations = [];
    state.selectedId = null;
    state.draft = null;
    state.history = [];
    els.emptyState.hidden = true;
    updateImageMeta();
    fitImage();
    updateSidebar();
  };
  img.onerror = () => {
    if (options.revokeUrl) {
      URL.revokeObjectURL(options.revokeUrl);
    }
    alert(`Could not load image: ${name}`);
  };
  img.src = src;
}

function updateImageMeta() {
  if (!state.imageInfo) {
    els.imageMeta.textContent = "No image loaded";
    return;
  }
  const { name, width, height } = state.imageInfo;
  els.imageMeta.textContent = `${name} · ${width} x ${height}`;
}

function resizeCanvas() {
  const rect = els.canvasWrap.getBoundingClientRect();
  const ratio = window.devicePixelRatio || 1;
  els.canvas.width = Math.max(1, Math.floor(rect.width * ratio));
  els.canvas.height = Math.max(1, Math.floor(rect.height * ratio));
  els.canvas.style.width = `${rect.width}px`;
  els.canvas.style.height = `${rect.height}px`;
  els.ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
}

function fitImage() {
  if (!state.imageInfo) return;
  const rect = els.canvas.getBoundingClientRect();
  const margin = 34;
  const scale = Math.min(
    (rect.width - margin) / state.imageInfo.width,
    (rect.height - margin) / state.imageInfo.height,
  );
  state.view.scale = clamp(scale, 0.05, 16);
  state.view.x = (rect.width - state.imageInfo.width * state.view.scale) / 2;
  state.view.y = (rect.height - state.imageInfo.height * state.view.scale) / 2;
  render();
}

function resetView() {
  if (!state.imageInfo) return;
  const rect = els.canvas.getBoundingClientRect();
  state.view.scale = 1;
  state.view.x = Math.round((rect.width - state.imageInfo.width) / 2);
  state.view.y = Math.round((rect.height - state.imageInfo.height) / 2);
  render();
}

function render() {
  const ctx = els.ctx;
  const rect = els.canvas.getBoundingClientRect();
  ctx.clearRect(0, 0, rect.width, rect.height);
  ctx.save();
  if (state.image) {
    ctx.translate(state.view.x, state.view.y);
    ctx.scale(state.view.scale, state.view.scale);
    ctx.globalAlpha = state.dimImage ? 0.45 : 1;
    ctx.drawImage(state.image, 0, 0);
    ctx.globalAlpha = 1;
    drawAnnotations(ctx);
    drawDraft(ctx);
  }
  ctx.restore();
  updateButtons();
}

function drawAnnotations(ctx) {
  state.annotations.forEach((annotation, index) => {
    drawAnnotation(ctx, annotation, {
      index: index + 1,
      selected: annotation.id === state.selectedId,
      alpha: 1,
    });
  });
}

function drawAnnotation(ctx, annotation, options = {}) {
  const category = categoryFor(annotation.category);
  const points = annotation.points.map(fromNormalized);
  const selected = Boolean(options.selected);
  const color = category.color;
  ctx.save();
  ctx.lineWidth = selected ? imageLineWidth(4) : imageLineWidth(3);
  ctx.strokeStyle = color;
  ctx.fillStyle = color;
  ctx.globalAlpha = options.alpha ?? 1;
  ctx.lineJoin = "round";
  ctx.lineCap = "round";

  if (annotation.type === "point") {
    drawPoint(ctx, points[0], selected ? 7 : 5, color);
  } else if (annotation.type === "line") {
    drawPolyline(ctx, points, false);
  } else if (annotation.type === "polygon") {
    drawPolyline(ctx, points, true);
    ctx.globalAlpha = 0.12;
    ctx.fill();
    ctx.globalAlpha = options.alpha ?? 1;
  } else if (annotation.type === "box") {
    const box = normalizeBox(points);
    ctx.strokeRect(box.x, box.y, box.w, box.h);
    ctx.globalAlpha = 0.1;
    ctx.fillRect(box.x, box.y, box.w, box.h);
    ctx.globalAlpha = options.alpha ?? 1;
  }

  if (selected) {
    for (const point of points) {
      drawHandle(ctx, point);
    }
  }

  if (state.showIndex || state.showLabels) {
    drawAnnotationLabel(ctx, annotation, points, options.index, category);
  }

  ctx.restore();
}

function drawDraft(ctx) {
  if (!state.draft) return;
  const category = categoryFor(state.currentCategory);
  const draft = {
    id: "draft",
    type: state.draft.type,
    category: state.currentCategory,
    label: "Draft",
    confidence: "medium",
    notes: "",
    points: state.draft.points,
  };
  drawAnnotation(ctx, draft, { selected: false, alpha: 0.75 });
  if (state.draft.preview) {
    const a = fromNormalized(state.draft.points[state.draft.points.length - 1]);
    const b = fromNormalized(state.draft.preview);
    ctx.save();
    ctx.strokeStyle = category.color;
    ctx.lineWidth = imageLineWidth(2);
    ctx.setLineDash([imageLineWidth(5), imageLineWidth(4)]);
    ctx.beginPath();
    ctx.moveTo(a.x, a.y);
    ctx.lineTo(b.x, b.y);
    ctx.stroke();
    ctx.restore();
  }
}

function drawPoint(ctx, point, radius, color) {
  const r = imageLineWidth(radius);
  ctx.beginPath();
  ctx.arc(point.x, point.y, r, 0, Math.PI * 2);
  ctx.fillStyle = color;
  ctx.fill();
  ctx.lineWidth = imageLineWidth(2);
  ctx.strokeStyle = "white";
  ctx.stroke();
}

function drawPolyline(ctx, points, closed) {
  if (!points.length) return;
  ctx.beginPath();
  ctx.moveTo(points[0].x, points[0].y);
  for (const point of points.slice(1)) {
    ctx.lineTo(point.x, point.y);
  }
  if (closed && points.length > 2) {
    ctx.closePath();
  }
  ctx.stroke();
}

function drawHandle(ctx, point) {
  const size = imageLineWidth(8);
  ctx.save();
  ctx.fillStyle = "white";
  ctx.strokeStyle = "#1d1b18";
  ctx.lineWidth = imageLineWidth(1.5);
  ctx.beginPath();
  ctx.rect(point.x - size / 2, point.y - size / 2, size, size);
  ctx.fill();
  ctx.stroke();
  ctx.restore();
}

function drawAnnotationLabel(ctx, annotation, points, index, category) {
  const anchor = labelAnchor(annotation.type, points);
  const parts = [];
  if (state.showIndex) parts.push(String(index));
  if (state.showLabels && annotation.label) parts.push(annotation.label);
  if (!parts.length) return;
  const text = parts.join(" ");
  const fontSize = imageLineWidth(12);
  ctx.save();
  ctx.font = `600 ${fontSize}px ui-sans-serif, system-ui`;
  const padding = imageLineWidth(4);
  const width = ctx.measureText(text).width + padding * 2;
  const height = fontSize + padding * 2;
  const x = anchor.x + imageLineWidth(7);
  const y = anchor.y - height - imageLineWidth(7);
  ctx.fillStyle = "rgba(255, 253, 248, 0.92)";
  ctx.strokeStyle = category.color;
  ctx.lineWidth = imageLineWidth(1.4);
  ctx.beginPath();
  ctx.roundRect(x, y, width, height, imageLineWidth(4));
  ctx.fill();
  ctx.stroke();
  ctx.fillStyle = "#201d19";
  ctx.fillText(text, x + padding, y + padding + fontSize * 0.78);
  ctx.restore();
}

function onPointerDown(event) {
  if (!state.image) return;
  try {
    els.canvas.setPointerCapture(event.pointerId);
  } catch {
    // Pointer capture can fail for synthetic or already-ended events.
  }
  const canvasPoint = canvasPointFromEvent(event);
  const imagePoint = canvasToImage(canvasPoint);
  const normalizedPoint = toNormalized(imagePoint);
  const isPan = state.currentTool === "pan" || state.spacePanning || event.button === 1;

  if (isPan) {
    state.interaction = {
      type: "pan",
      start: canvasPoint,
      view: { ...state.view },
    };
    updateCanvasCursor(true);
    return;
  }

  if (state.currentTool === "select") {
    const handleHit = hitTestHandle(canvasPoint);
    if (handleHit) {
      pushHistory();
      state.selectedId = handleHit.id;
      state.interaction = { type: "drag-handle", ...handleHit };
      updateSidebar();
      render();
      return;
    }

    const annotation = hitTestAnnotation(canvasPoint);
    selectAnnotation(annotation?.id ?? null);
    return;
  }

  if (!pointInsideImage(imagePoint)) return;

  if (state.currentTool === "point") {
    pushHistory();
    const annotation = makeAnnotation("point", [normalizedPoint]);
    state.annotations.push(annotation);
    selectAnnotation(annotation.id);
    return;
  }

  if (state.currentTool === "box") {
    pushHistory();
    state.draft = { type: "box", points: [normalizedPoint, normalizedPoint] };
    state.interaction = { type: "draw-box" };
    render();
    return;
  }

  if (state.currentTool === "line" || state.currentTool === "polygon") {
    if (!state.draft || state.draft.type !== state.currentTool) {
      pushHistory();
      state.draft = { type: state.currentTool, points: [], preview: null };
    }
    state.draft.points.push(normalizedPoint);
    state.draft.preview = normalizedPoint;
    render();
  }
}

function onPointerMove(event) {
  if (!state.image) return;
  const canvasPoint = canvasPointFromEvent(event);
  const imagePoint = canvasToImage(canvasPoint);
  const normalizedPoint = clampNormalized(toNormalized(imagePoint));

  if (state.interaction?.type === "pan") {
    const dx = canvasPoint.x - state.interaction.start.x;
    const dy = canvasPoint.y - state.interaction.start.y;
    state.view.x = state.interaction.view.x + dx;
    state.view.y = state.interaction.view.y + dy;
    render();
    return;
  }

  if (state.interaction?.type === "drag-handle") {
    const annotation = annotationById(state.interaction.id);
    if (!annotation) return;
    annotation.points[state.interaction.pointIndex] = normalizedPoint;
    render();
    updateSidebar(false);
    return;
  }

  if (state.interaction?.type === "draw-box" && state.draft?.type === "box") {
    state.draft.points[1] = normalizedPoint;
    render();
    return;
  }

  if (state.draft && (state.draft.type === "line" || state.draft.type === "polygon")) {
    state.draft.preview = normalizedPoint;
    render();
  }
}

function onPointerUp(event) {
  if (state.interaction?.type === "draw-box" && state.draft?.type === "box") {
    const [a, b] = state.draft.points;
    if (distance(a, b) > 0.004) {
      const annotation = makeAnnotation("box", state.draft.points);
      state.annotations.push(annotation);
      state.draft = null;
      selectAnnotation(annotation.id);
    } else {
      state.draft = null;
      render();
    }
  }
  state.interaction = null;
  updateCanvasCursor();
  try {
    els.canvas.releasePointerCapture(event.pointerId);
  } catch {
    // Pointer capture may already be gone after a cancel.
  }
}

function finishDraft() {
  if (!state.draft) return;
  const minPoints = state.draft.type === "polygon" ? 3 : 2;
  if (state.draft.points.length < minPoints) {
    cancelDraft();
    return;
  }
  const annotation = makeAnnotation(state.draft.type, state.draft.points);
  state.annotations.push(annotation);
  state.draft = null;
  selectAnnotation(annotation.id);
}

function cancelDraft(renderAfter = true) {
  state.draft = null;
  state.interaction = null;
  if (renderAfter) render();
}

function makeAnnotation(type, points) {
  return {
    id: `a-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`,
    type,
    category: state.currentCategory,
    label: categoryFor(state.currentCategory).label,
    confidence: "medium",
    notes: "",
    points: points.map(clampNormalized),
  };
}

function selectAnnotation(id) {
  state.selectedId = id;
  state.editorHistoryToken = null;
  updateSidebar();
  render();
}

function patchSelected(patch) {
  const annotation = selectedAnnotation();
  if (!annotation) return;
  Object.assign(annotation, patch);
  updateAnnotationList();
  render();
}

function rememberEditorUndo(fieldId) {
  if (!state.selectedId) return;
  const token = `${state.selectedId}:${fieldId}`;
  if (state.editorHistoryToken === token) return;
  pushHistory();
  state.editorHistoryToken = token;
  updateButtons();
}

function deleteSelected() {
  if (!state.selectedId) return;
  pushHistory();
  state.annotations = state.annotations.filter(annotation => annotation.id !== state.selectedId);
  state.selectedId = null;
  updateSidebar();
  render();
}

function clearAll() {
  if (!state.annotations.length) return;
  if (!confirm("Clear all annotations for this crop?")) return;
  pushHistory();
  state.annotations = [];
  state.selectedId = null;
  state.draft = null;
  updateSidebar();
  render();
}

function pushHistory() {
  state.history.push(JSON.stringify({
    annotations: state.annotations,
    selectedId: state.selectedId,
  }));
  if (state.history.length > 80) {
    state.history.shift();
  }
}

function undo() {
  const snapshot = state.history.pop();
  if (!snapshot) return;
  const parsed = JSON.parse(snapshot);
  state.annotations = parsed.annotations;
  state.selectedId = parsed.selectedId;
  state.draft = null;
  updateSidebar();
  render();
}

function updateSidebar(syncEditor = true) {
  updateAnnotationList();
  const selected = selectedAnnotation();
  els.selectionEmpty.hidden = Boolean(selected);
  els.editor.hidden = !selected;
  if (selected && syncEditor) {
    els.editCategory.value = selected.category;
    els.editLabel.value = selected.label ?? "";
    els.editConfidence.value = selected.confidence ?? "medium";
    els.editNotes.value = selected.notes ?? "";
  }
}

function updateAnnotationList() {
  els.annotationCount.textContent = `${state.annotations.length} ${state.annotations.length === 1 ? "item" : "items"}`;
  els.annotationList.innerHTML = "";
  state.annotations.forEach((annotation, index) => {
    const category = categoryFor(annotation.category);
    const item = document.createElement("li");
    item.dataset.id = annotation.id;
    item.className = annotation.id === state.selectedId ? "active" : "";
    item.innerHTML = `
      <div class="annotation-title">
        <span class="swatch" style="background:${category.color}"></span>
        <span>${index + 1}. ${escapeHtml(annotation.label || category.label)}</span>
      </div>
      <div class="annotation-meta">${escapeHtml(category.label)} · ${escapeHtml(annotation.type)} · ${escapeHtml(annotation.confidence || "medium")}</div>
    `;
    els.annotationList.append(item);
  });
}

function updateButtons() {
  const hasImage = Boolean(state.image);
  els.exportJson.disabled = !hasImage;
  els.exportPng.disabled = !hasImage;
  els.undo.disabled = !state.history.length;
  els.deleteSelected.disabled = !state.selectedId;
  els.finishShape.disabled = !state.draft;
  els.cancelDraft.disabled = !state.draft;
  els.clearAll.disabled = !state.annotations.length;
  els.zoomIn.disabled = !hasImage;
  els.zoomOut.disabled = !hasImage;
  els.fitImage.disabled = !hasImage;
  els.resetView.disabled = !hasImage;
}

function updateCanvasCursor(forceGrabbing = false) {
  if (forceGrabbing) {
    els.canvas.style.cursor = "grabbing";
  } else if (state.spacePanning) {
    els.canvas.style.cursor = "grab";
  } else {
    els.canvas.style.cursor = TOOL_CURSOR[state.currentTool] || "default";
  }
}

function importJson(file) {
  const reader = new FileReader();
  reader.onload = () => {
    try {
      const data = JSON.parse(String(reader.result));
      const annotations = Array.isArray(data.annotations) ? data.annotations : [];
      pushHistory();
      state.annotations = annotations.map(sanitizeAnnotation).filter(Boolean);
      state.selectedId = state.annotations[0]?.id ?? null;
      updateSidebar();
      render();
    } catch (error) {
      alert(`Could not import JSON: ${error.message}`);
    }
  };
  reader.onerror = () => {
    alert(`Could not read JSON file: ${reader.error?.message || file.name}`);
  };
  reader.readAsText(file);
}

function exportJson() {
  if (!state.imageInfo) return;
  const payload = {
    schemaVersion: 1,
    kind: "graphics-v2-map-annotation",
    image: {
      name: state.imageInfo.name,
      src: state.imageInfo.src,
      width: state.imageInfo.width,
      height: state.imageInfo.height,
    },
    categories: CATEGORIES,
    annotations: state.annotations,
    exportedAt: new Date().toISOString(),
  };
  const slug = slugify(state.imageInfo.name.replace(/\.[^.]+$/, ""));
  downloadBlob(
    `${slug || "map-crop"}-annotations.json`,
    new Blob([`${JSON.stringify(payload, null, 2)}\n`], { type: "application/json" }),
  );
}

function exportPng() {
  if (!state.imageInfo || !state.image) return;
  const canvas = document.createElement("canvas");
  canvas.width = state.imageInfo.width;
  canvas.height = state.imageInfo.height;
  const ctx = canvas.getContext("2d");
  ctx.drawImage(state.image, 0, 0);
  const oldView = state.view;
  const oldSelectedId = state.selectedId;
  state.view = { scale: 1, x: 0, y: 0 };
  state.selectedId = null;
  drawAnnotations(ctx);
  state.view = oldView;
  state.selectedId = oldSelectedId;
  canvas.toBlob(blob => {
    if (!blob) {
      alert("Could not export PNG. Try serving the tool from localhost instead of file://.");
      return;
    }
    const slug = slugify(state.imageInfo.name.replace(/\.[^.]+$/, ""));
    downloadBlob(`${slug || "map-crop"}-annotation-overlay.png`, blob);
  }, "image/png");
}

function sanitizeAnnotation(raw) {
  if (!raw || !Array.isArray(raw.points)) return null;
  const type = ["point", "line", "polygon", "box"].includes(raw.type) ? raw.type : "point";
  const category = CATEGORIES.some(item => item.id === raw.category) ? raw.category : "uncertain";
  const points = normalizeImportedPoints(type, raw.points.map(point => clampNormalized({
    x: Number(point.x),
    y: Number(point.y),
  })).filter(point => Number.isFinite(point.x) && Number.isFinite(point.y)));
  if (points.length < minPointsForType(type)) return null;
  return {
    id: String(raw.id || `a-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`),
    type,
    category,
    label: String(raw.label || categoryFor(category).label),
    confidence: ["high", "medium", "low"].includes(raw.confidence) ? raw.confidence : "medium",
    notes: String(raw.notes || ""),
    points,
  };
}

function minPointsForType(type) {
  if (type === "point") return 1;
  if (type === "polygon") return 3;
  return 2;
}

function normalizeImportedPoints(type, points) {
  if (type === "point") return points.slice(0, 1);
  if (type === "box") return points.slice(0, 2);
  return points;
}

function annotationById(id) {
  return state.annotations.find(annotation => annotation.id === id);
}

function selectedAnnotation() {
  return annotationById(state.selectedId);
}

function categoryFor(id) {
  return CATEGORIES.find(category => category.id === id) || CATEGORIES[CATEGORIES.length - 1];
}

function hitTestHandle(canvasPoint) {
  const selected = selectedAnnotation();
  if (!selected) return null;
  const points = selected.points.map(fromNormalized).map(imageToCanvas);
  const tolerance = 11;
  for (let index = points.length - 1; index >= 0; index -= 1) {
    if (distance(points[index], canvasPoint) <= tolerance) {
      return { id: selected.id, pointIndex: index };
    }
  }
  return null;
}

function hitTestAnnotation(canvasPoint) {
  const imagePoint = canvasToImage(canvasPoint);
  const tolerance = 10 / state.view.scale;
  for (let index = state.annotations.length - 1; index >= 0; index -= 1) {
    const annotation = state.annotations[index];
    const points = annotation.points.map(fromNormalized);
    if (annotation.type === "point" && distance(points[0], imagePoint) <= tolerance * 1.4) {
      return annotation;
    }
    if (annotation.type === "line" && distanceToPolyline(imagePoint, points, false) <= tolerance) {
      return annotation;
    }
    if (annotation.type === "polygon") {
      if (pointInPolygon(imagePoint, points) || distanceToPolyline(imagePoint, points, true) <= tolerance) {
        return annotation;
      }
    }
    if (annotation.type === "box") {
      const box = normalizeBox(points);
      const inside = imagePoint.x >= box.x && imagePoint.x <= box.x + box.w && imagePoint.y >= box.y && imagePoint.y <= box.y + box.h;
      const nearEdge = Math.abs(imagePoint.x - box.x) <= tolerance
        || Math.abs(imagePoint.x - (box.x + box.w)) <= tolerance
        || Math.abs(imagePoint.y - box.y) <= tolerance
        || Math.abs(imagePoint.y - (box.y + box.h)) <= tolerance;
      if (inside && nearEdge) return annotation;
    }
  }
  return null;
}

function pointInsideImage(point) {
  return state.imageInfo
    && point.x >= 0
    && point.y >= 0
    && point.x <= state.imageInfo.width
    && point.y <= state.imageInfo.height;
}

function canvasPointFromEvent(event) {
  const rect = els.canvas.getBoundingClientRect();
  return { x: event.clientX - rect.left, y: event.clientY - rect.top };
}

function canvasToImage(point) {
  return {
    x: (point.x - state.view.x) / state.view.scale,
    y: (point.y - state.view.y) / state.view.scale,
  };
}

function imageToCanvas(point) {
  return {
    x: point.x * state.view.scale + state.view.x,
    y: point.y * state.view.scale + state.view.y,
  };
}

function toNormalized(point) {
  return {
    x: point.x / state.imageInfo.width,
    y: point.y / state.imageInfo.height,
  };
}

function fromNormalized(point) {
  return {
    x: point.x * state.imageInfo.width,
    y: point.y * state.imageInfo.height,
  };
}

function clampNormalized(point) {
  return {
    x: clamp(point.x, 0, 1),
    y: clamp(point.y, 0, 1),
  };
}

function zoomAt(canvasPoint, factor) {
  if (!state.imageInfo) return;
  const before = canvasToImage(canvasPoint);
  state.view.scale = clamp(state.view.scale * factor, 0.05, 18);
  state.view.x = canvasPoint.x - before.x * state.view.scale;
  state.view.y = canvasPoint.y - before.y * state.view.scale;
  render();
}

function centerOfCanvas() {
  const rect = els.canvas.getBoundingClientRect();
  return { x: rect.width / 2, y: rect.height / 2 };
}

function imageLineWidth(width) {
  return width / state.view.scale;
}

function labelAnchor(type, points) {
  if (type === "box") {
    const box = normalizeBox(points);
    return { x: box.x, y: box.y };
  }
  const sum = points.reduce((acc, point) => ({ x: acc.x + point.x, y: acc.y + point.y }), { x: 0, y: 0 });
  return { x: sum.x / points.length, y: sum.y / points.length };
}

function normalizeBox(points) {
  const [a, b] = points;
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  return {
    x,
    y,
    w: Math.abs(a.x - b.x),
    h: Math.abs(a.y - b.y),
  };
}

function distance(a, b) {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

function distanceToPolyline(point, points, closed) {
  let min = Infinity;
  for (let index = 0; index < points.length - 1; index += 1) {
    min = Math.min(min, distanceToSegment(point, points[index], points[index + 1]));
  }
  if (closed && points.length > 2) {
    min = Math.min(min, distanceToSegment(point, points[points.length - 1], points[0]));
  }
  return min;
}

function distanceToSegment(point, a, b) {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const lengthSquared = dx * dx + dy * dy;
  if (lengthSquared === 0) return distance(point, a);
  const t = clamp(((point.x - a.x) * dx + (point.y - a.y) * dy) / lengthSquared, 0, 1);
  return distance(point, { x: a.x + t * dx, y: a.y + t * dy });
}

function pointInPolygon(point, polygon) {
  let inside = false;
  for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i, i += 1) {
    const pi = polygon[i];
    const pj = polygon[j];
    const intersects = ((pi.y > point.y) !== (pj.y > point.y))
      && (point.x < ((pj.x - pi.x) * (point.y - pi.y)) / (pj.y - pi.y) + pi.x);
    if (intersects) inside = !inside;
  }
  return inside;
}

function downloadBlob(filename, blob) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.append(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}

function slugify(value) {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

function escapeHtml(value) {
  return String(value)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function isTypingInEditor(target) {
  return ["INPUT", "TEXTAREA", "SELECT"].includes(target?.tagName);
}
