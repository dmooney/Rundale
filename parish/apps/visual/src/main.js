import { fetchSceneState, normalizeBackendUrl, postCommand } from './scene-client.js';
import { buildSceneDisplayModel, renderSceneCanvas } from './renderer.js';

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
    renderSceneCanvas(canvas, null);
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
        const model = renderSceneCanvas(canvas, scene);
        updateInspector(model);
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

window.addEventListener('resize', () => {
    refreshScene();
});

refreshScene();
