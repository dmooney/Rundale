import { fetchSceneState, normalizeBackendUrl, postCommand } from './scene-client.js';
import { appendTurnEntry, createTurnEntry, responseSummary } from './turn-log.js';
import {
    buildSceneDisplayModel,
    canvasPointToStage,
    findSceneTargetAtStagePoint,
    hotspotCommand,
    npcCommand,
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
const turnLog = document.querySelector('#turn-log');

let currentBackendUrl = normalizeBackendUrl(localStorage.getItem(storageKey) || '');
let currentSceneModel = buildSceneDisplayModel(null);
let currentPlateImage = null;
let currentSpriteImages = new Map();
let hoveredHotspotId = null;
let selectedHotspotId = null;
let hoveredNpcId = null;
let selectedNpcId = null;
let turnEntries = [];
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

function renderTurnLog() {
    turnLog.replaceChildren();
    if (turnEntries.length === 0) {
        const item = document.createElement('li');
        item.className = 'muted kind-system';
        item.textContent = 'No turns yet';
        turnLog.append(item);
        return;
    }
    for (const entry of turnEntries) {
        const item = document.createElement('li');
        item.className = `kind-${entry.kind}`;
        const label = document.createElement('span');
        label.className = 'entry-label';
        label.textContent = entry.label;
        const text = document.createElement('span');
        text.className = 'entry-text';
        text.textContent = entry.text;
        item.append(label, text);
        turnLog.append(item);
    }
    turnLog.scrollTop = turnLog.scrollHeight;
}

function appendTurn(kind, label, text) {
    turnEntries = appendTurnEntry(turnEntries, createTurnEntry(kind, label, text));
    renderTurnLog();
}

function renderError(error) {
    const model = buildSceneDisplayModel(null);
    currentSceneModel = model;
    currentPlateImage = null;
    currentSpriteImages = new Map();
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
    appendTurn('system', 'System', subtitle.textContent);
    return model;
}

function renderCurrentScene() {
    renderSceneModel(canvas, currentSceneModel, {
        plateImage: currentPlateImage,
        activeHotspotId: hoveredHotspotId || selectedHotspotId,
        activeNpcId: hoveredNpcId || selectedNpcId,
        spriteImages: currentSpriteImages,
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

async function loadSpriteImages(model) {
    if (model.kind !== 'scene' || model.npcs.length === 0) {
        return new Map();
    }
    const entries = await Promise.all(
        model.npcs.map(async (npc) => [npc.id, await loadImage(npc.spriteUrl)]),
    );
    return new Map(entries);
}

function setCommandLog(response) {
    commandLog.textContent = responseSummary(response);
}

async function refreshScene() {
    refreshButton.disabled = true;
    subtitle.textContent = 'Loading scene state';
    try {
        const scene = await fetchSceneState({ backendUrl: currentBackendUrl });
        const model = buildSceneDisplayModel(scene);
        currentSceneModel = model;
        currentPlateImage = null;
        currentSpriteImages = new Map();
        hoveredHotspotId = null;
        selectedHotspotId = null;
        hoveredNpcId = null;
        selectedNpcId = null;
        renderCurrentScene();
        updateInspector(model);
        if (model.kind === 'scene') {
            const [image, sprites] = await Promise.all([
                loadImage(model.plate),
                loadSpriteImages(model),
            ]);
            if (currentSceneModel === model) {
                currentPlateImage = image;
                currentSpriteImages = sprites;
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
    appendTurn('player', 'You', trimmed);
    try {
        const response = await postCommand({ text: trimmed, backendUrl: currentBackendUrl });
        setCommandLog(response);
        appendTurn('world', 'World', responseSummary(response));
        await refreshScene();
    } catch (error) {
        commandLog.textContent = error instanceof Error ? error.message : String(error);
        appendTurn('system', 'System', commandLog.textContent);
    }
}

function targetFromEvent(event) {
    const point = canvasPointToStage(canvas, event.clientX, event.clientY);
    return findSceneTargetAtStagePoint(currentSceneModel, point);
}

async function activateHotspot(hotspot) {
    if (!hotspot) {
        return;
    }
    selectedHotspotId = hotspot.id;
    selectedNpcId = null;
    renderCurrentScene();

    const action = hotspotCommand(hotspot);
    if (action.kind === 'inspect') {
        commandLog.textContent = action.text;
        appendTurn('inspect', 'Inspect', action.text);
        return;
    }

    if (action.command) {
        commandInput.value = action.command;
        await submitCommand(action.command);
    }
}

function activateNpc(npc) {
    if (!npc) {
        return;
    }
    selectedNpcId = npc.id;
    selectedHotspotId = null;
    const action = npcCommand(npc);
    commandInput.value = action.command;
    commandLog.textContent = `Ready to talk to ${action.label}.`;
    appendTurn('selection', 'Selected', `Ready to talk to ${action.label}.`);
    renderCurrentScene();
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
    const target = targetFromEvent(event);
    const nextHotspotId = target?.kind === 'hotspot' ? target.value.id : null;
    const nextNpcId = target?.kind === 'npc' ? target.value.id : null;
    if (hoveredHotspotId !== nextHotspotId || hoveredNpcId !== nextNpcId) {
        hoveredHotspotId = nextHotspotId;
        hoveredNpcId = nextNpcId;
        canvas.style.cursor = target ? 'pointer' : 'default';
        renderCurrentScene();
    }
});

canvas.addEventListener('mouseleave', () => {
    hoveredHotspotId = null;
    hoveredNpcId = null;
    canvas.style.cursor = 'default';
    renderCurrentScene();
});

canvas.addEventListener('click', (event) => {
    const target = targetFromEvent(event);
    if (target?.kind === 'npc') {
        activateNpc(target.value);
        return;
    }
    activateHotspot(target?.value);
});

window.addEventListener('resize', () => {
    renderCurrentScene();
});

renderTurnLog();
refreshScene();
