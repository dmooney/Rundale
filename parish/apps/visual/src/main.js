import { fetchSceneState, normalizeBackendUrl, postCommand } from './scene-client.js';
import {
    buildSceneDisplayModel,
    canvasPointToStage,
    findHotspotAtStagePoint,
    hotspotCommand,
    renderSceneModel,
} from './renderer.js';

const storageKey = 'parish.visual.backendUrl';

const canvas = document.querySelector('#scene-canvas');
const title = document.querySelector('#scene-title');
const subtitle = document.querySelector('#scene-subtitle');
const form = document.querySelector('#settings-form');
const backendInput = document.querySelector('#backend-url');
const refreshButton = document.querySelector('#refresh-button');
const commandForm = document.querySelector('#command-form');
const commandInput = document.querySelector('#command-input');
const crossroadsButton = document.querySelector('#crossroads-button');
const commandLog = document.querySelector('#command-log');
const metricLocation = document.querySelector('#metric-location');
const metricVariant = document.querySelector('#metric-variant');
const metricPlate = document.querySelector('#metric-plate');
const metricHotspots = document.querySelector('#metric-hotspots');
const metricPeople = document.querySelector('#metric-people');
const hotspotList = document.querySelector('#hotspot-list');
const peopleList = document.querySelector('#people-list');

let currentBackendUrl = normalizeBackendUrl(localStorage.getItem(storageKey) || '');
let currentSceneModel = buildSceneDisplayModel(null);
let currentPlateImage = null;
let hoveredHotspotId = null;
let selectedHotspotId = null;
const plateCache = new Map();

backendInput.value = currentBackendUrl;

function setList(list, items, renderItem) {
    list.replaceChildren();
    if (items.length === 0) {
        const item = document.createElement('li');
        item.className = 'muted';
        item.textContent = 'None';
        list.append(item);
        return;
    }
    for (const value of items) {
        const item = document.createElement('li');
        item.textContent = renderItem(value);
        list.append(item);
    }
}

function updateInspector(model) {
    title.textContent = model.title;
    subtitle.textContent = model.subtitle;
    metricLocation.textContent = model.location;
    metricVariant.textContent = model.variant;
    metricPlate.textContent = model.plate || '-';
    metricHotspots.textContent = String(model.hotspots.length);
    metricPeople.textContent = String(model.npcs.length + model.overflow.length);
    setList(hotspotList, model.hotspots, (hotspot) => `${hotspot.label} (${hotspot.action})`);
    setList(peopleList, model.npcs, (npc) => `${npc.label} at ${npc.slotId}`);
}

function renderError(error) {
    const model = buildSceneDisplayModel(null);
    currentSceneModel = model;
    currentPlateImage = null;
    renderCurrentScene();
    title.textContent = 'Scene unavailable';
    subtitle.textContent = error instanceof Error ? error.message : String(error);
    metricLocation.textContent = '-';
    metricVariant.textContent = '-';
    metricPlate.textContent = '-';
    metricHotspots.textContent = '0';
    metricPeople.textContent = '0';
    setList(hotspotList, [], () => '');
    setList(peopleList, [], () => '');
    return model;
}

function renderCurrentScene() {
    renderSceneModel(canvas, currentSceneModel, {
        plateImage: currentPlateImage,
        activeHotspotId: hoveredHotspotId || selectedHotspotId,
    });
}

function resolvePlateUrl(url) {
    if (!url || /^https?:\/\//i.test(url) || url.startsWith('data:')) {
        return url;
    }
    return url.startsWith('/') ? url : `/${url}`;
}

function loadImage(url) {
    if (!url) {
        return Promise.resolve(null);
    }
    const resolved = resolvePlateUrl(url);
    if (plateCache.has(resolved)) {
        return plateCache.get(resolved);
    }

    const promise = new Promise((resolve) => {
        const image = new Image();
        image.onload = () => resolve(image);
        image.onerror = () => resolve(null);
        image.src = resolved;
    });
    plateCache.set(resolved, promise);
    return promise;
}

function setCommandLog(response) {
    const lines = Array.isArray(response?.lines) ? response.lines : [];
    const last = lines.at(-1);
    commandLog.textContent = last?.text || response?.outcome || 'Done';
}

async function refreshScene() {
    refreshButton.disabled = true;
    subtitle.textContent = 'Loading scene state';
    try {
        const scene = await fetchSceneState({ backendUrl: currentBackendUrl });
        const model = buildSceneDisplayModel(scene);
        currentSceneModel = model;
        currentPlateImage = null;
        hoveredHotspotId = null;
        selectedHotspotId = null;
        renderCurrentScene();
        updateInspector(model);
        if (model.kind === 'scene') {
            const image = await loadImage(model.plate);
            if (currentSceneModel === model) {
                currentPlateImage = image;
                renderCurrentScene();
            }
        }
    } catch (error) {
        renderError(error);
    } finally {
        refreshButton.disabled = false;
    }
}

async function submitCommand(text) {
    const trimmed = String(text || '').trim();
    if (!trimmed) {
        return;
    }
    commandLog.textContent = 'Sending';
    try {
        const response = await postCommand({ text: trimmed, backendUrl: currentBackendUrl });
        setCommandLog(response);
        await refreshScene();
    } catch (error) {
        commandLog.textContent = error instanceof Error ? error.message : String(error);
    }
}

function hotspotFromEvent(event) {
    const point = canvasPointToStage(canvas, event.clientX, event.clientY);
    return findHotspotAtStagePoint(currentSceneModel, point);
}

async function activateHotspot(hotspot) {
    if (!hotspot) {
        return;
    }
    selectedHotspotId = hotspot.id;
    renderCurrentScene();

    const action = hotspotCommand(hotspot);
    if (action.kind === 'inspect') {
        commandLog.textContent = action.text;
        return;
    }

    if (action.command) {
        commandInput.value = action.command;
        await submitCommand(action.command);
    }
}

form.addEventListener('submit', (event) => {
    event.preventDefault();
    currentBackendUrl = normalizeBackendUrl(backendInput.value);
    localStorage.setItem(storageKey, currentBackendUrl);
    refreshScene();
});

refreshButton.addEventListener('click', () => {
    refreshScene();
});

commandForm.addEventListener('submit', (event) => {
    event.preventDefault();
    submitCommand(commandInput.value);
});

crossroadsButton.addEventListener('click', () => {
    commandInput.value = 'go to The Crossroads';
    submitCommand(commandInput.value);
});

canvas.addEventListener('mousemove', (event) => {
    const hotspot = hotspotFromEvent(event);
    const nextHotspotId = hotspot?.id || null;
    if (hoveredHotspotId !== nextHotspotId) {
        hoveredHotspotId = nextHotspotId;
        canvas.style.cursor = hotspot ? 'pointer' : 'default';
        renderCurrentScene();
    }
});

canvas.addEventListener('mouseleave', () => {
    hoveredHotspotId = null;
    canvas.style.cursor = 'default';
    renderCurrentScene();
});

canvas.addEventListener('click', (event) => {
    const hotspot = hotspotFromEvent(event);
    activateHotspot(hotspot);
});

window.addEventListener('resize', () => {
    renderCurrentScene();
});

refreshScene();
