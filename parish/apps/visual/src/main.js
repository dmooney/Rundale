import { fetchSceneState, normalizeBackendUrl, postCommand } from './scene-client.js';
import { buildSceneDisplayModel, hotspotCommand, npcCommand } from './renderer.js';
import { PixiSceneRenderer } from './pixi-renderer.js';
import { appendTurnEntry, createTurnEntry, responseSummary } from './turn-log.js';

const storageKey = 'parish.visual.backendUrl';
const queryParams = new URLSearchParams(window.location.search);
const proofAtomOnly =
    queryParams.get('visualProofMode') === 'atom-only' || queryParams.get('compositor') === 'atom-only';
const INTERACTION_TELEMETRY_KEY = '__rundaleVisualInteraction';

const stageHost = document.querySelector('#game-stage');
const caption = document.querySelector('#caption');
const statusLabel = document.querySelector('#status-label');
const locationLabel = document.querySelector('#location-label');
const actionPrompt = document.querySelector('#action-prompt');
const actionButton = document.querySelector('#action-button');
const actionLabel = document.querySelector('#action-label');
const commandPanel = document.querySelector('#command-panel');
const commandForm = document.querySelector('#command-form');
const commandInput = document.querySelector('#command-input');
const commandButton = commandForm.querySelector('button[type="submit"]');
const settingsForm = document.querySelector('#settings-form');
const backendInput = document.querySelector('#backend-url');
const refreshButton = document.querySelector('#refresh-button');
const turnLog = document.querySelector('#turn-log');

let currentBackendUrl = normalizeBackendUrl(localStorage.getItem(storageKey) || '');
let currentSceneModel = buildSceneDisplayModel(null);
let hoveredHotspotId = null;
let selectedHotspotId = null;
let hoveredNpcId = null;
let selectedNpcId = null;
let hoveredTarget = null;
let selectedTarget = null;
let promptTarget = null;
let turnEntries = [];
let isRefreshing = false;
let isSending = false;
let renderer = null;
let hoverTelemetryKey = '';
let interactionEventSeq = 0;
const submittedCommands = [];
const interactionEvents = [];

backendInput.value = currentBackendUrl;

function delay(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
}

function targetTelemetry(target) {
    if (!target) {
        return null;
    }
    if (target.kind === 'hotspot') {
        const action = hotspotCommand(target.value);
        return {
            kind: 'hotspot',
            id: target.value.id,
            label: target.value.label,
            actionKind: action.kind,
            command: action.command || '',
            text: action.text || '',
            targetLabel: target.value.activation?.target_label || '',
        };
    }
    if (target.kind === 'npc') {
        const action = npcCommand(target.value);
        return {
            kind: 'npc',
            id: target.value.id,
            label: action.label,
            command: action.command,
            slotId: target.value.slotId || '',
        };
    }
    return null;
}

function promptTelemetry() {
    if (actionPrompt.hidden || !promptTarget) {
        return null;
    }
    return {
        verb: actionButton.textContent,
        label: actionLabel.textContent,
        target: targetTelemetry(promptTarget),
    };
}

function publishInteractionTelemetry() {
    globalThis[INTERACTION_TELEMETRY_KEY] = {
        location: locationLabel.textContent,
        caption: caption.textContent,
        status: statusLabel.textContent,
        hoveredTarget: targetTelemetry(hoveredTarget),
        selectedTarget: targetTelemetry(selectedTarget),
        prompt: promptTelemetry(),
        submittedCommands: [...submittedCommands],
        events: [...interactionEvents],
        busy: {
            refreshing: isRefreshing,
            sending: isSending,
        },
    };
}

function recordInteractionEvent(type, detail = {}) {
    interactionEventSeq += 1;
    interactionEvents.push({
        seq: interactionEventSeq,
        type,
        ...detail,
    });
    while (interactionEvents.length > 80) {
        interactionEvents.shift();
    }
    publishInteractionTelemetry();
}

function setCaption(text) {
    caption.textContent = text || 'Click into the world.';
    publishInteractionTelemetry();
}

function setStatus(text) {
    statusLabel.textContent = text;
    publishInteractionTelemetry();
}

function setBusy(busy) {
    commandButton.disabled = busy;
    actionButton.disabled = busy || !promptTarget;
    refreshButton.disabled = busy;
    commandInput.disabled = busy;
    stageHost.classList.toggle('is-busy', busy);
    publishInteractionTelemetry();
}

function sceneCaption(model) {
    return model.kind === 'scene' ? `${model.location}.` : model.subtitle;
}

function renderTurnLog() {
    turnLog.replaceChildren();
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

function syncInteractionState() {
    stageHost.classList.toggle('is-interactive', Boolean(hoveredHotspotId || hoveredNpcId));
    renderer?.setInteractionState({
        activeHotspotId: hoveredHotspotId,
        selectedHotspotId,
        activeNpcId: hoveredNpcId,
        selectedNpcId,
    });
}

function promptForTarget(target) {
    if (target?.kind === 'npc') {
        const action = npcCommand(target.value);
        return {
            verb: 'Talk',
            label: action.label,
            command: action.command,
            target,
        };
    }
    if (target?.kind !== 'hotspot') {
        return null;
    }

    const action = hotspotCommand(target.value);
    if (action.kind === 'travel') {
        return {
            verb: 'Go',
            label: target.value.activation?.target_label || action.label,
            command: action.command,
            target,
        };
    }
    if (action.kind === 'talk') {
        return {
            verb: 'Talk',
            label: action.label,
            command: action.command,
            target,
        };
    }
    return {
        verb: 'Look',
        label: action.label || target.value.label,
        text: action.text,
        target,
    };
}

function showActionPrompt(target) {
    const prompt = promptForTarget(target);
    promptTarget = prompt?.target || null;
    if (!prompt) {
        actionPrompt.hidden = true;
        actionButton.textContent = '';
        actionLabel.textContent = '';
        actionButton.disabled = true;
        publishInteractionTelemetry();
        return;
    }
    actionPrompt.hidden = false;
    actionPrompt.dataset.kind = prompt.verb.toLowerCase();
    actionButton.textContent = prompt.verb;
    actionLabel.textContent = prompt.label;
    actionButton.disabled = isRefreshing || isSending;
    publishInteractionTelemetry();
}

async function showModel(model) {
    currentSceneModel = model;
    hoveredHotspotId = null;
    hoveredNpcId = null;
    selectedHotspotId = null;
    selectedNpcId = null;
    hoveredTarget = null;
    selectedTarget = null;
    showActionPrompt(null);
    locationLabel.textContent = model.kind === 'scene' ? model.location : 'No scene';
    setCaption(sceneCaption(model));
    await renderer.setScene(model, {
        activeHotspotId: hoveredHotspotId,
        selectedHotspotId,
        activeNpcId: hoveredNpcId,
        selectedNpcId,
    });
    syncInteractionState();
    recordInteractionEvent('scene-shown', {
        slug: model.slug || '',
        location: model.kind === 'scene' ? model.location : 'No scene',
    });
}

async function refreshScene() {
    if (isRefreshing) {
        return false;
    }
    isRefreshing = true;
    setBusy(true);
    setStatus('Loading');
    try {
        const scene = await fetchSceneState({ backendUrl: currentBackendUrl });
        const model = buildSceneDisplayModel(scene);
        setStatus(model.kind === 'scene' ? 'Ready' : 'No scene');
        await showModel(model);
        return true;
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        await showModel(buildSceneDisplayModel(null));
        setStatus('Offline');
        setCaption(message);
        appendTurn('system', 'System', message);
        return false;
    } finally {
        isRefreshing = false;
        setBusy(isSending);
    }
}

async function submitCommand(text) {
    const trimmed = String(text || '').trim();
    if (!trimmed || isSending || isRefreshing) {
        return;
    }
    isSending = true;
    setBusy(true);
    setStatus('Sending');
    setCaption(trimmed);
    submittedCommands.push(trimmed);
    while (submittedCommands.length > 20) {
        submittedCommands.shift();
    }
    recordInteractionEvent('submit-command', { command: trimmed });
    appendTurn('player', 'You', trimmed);
    try {
        const response = await postCommand({ text: trimmed, backendUrl: currentBackendUrl });
        const summary = responseSummary(response);
        appendTurn('world', 'World', summary);
        setCaption(summary);
        const refreshed = await refreshScene();
        if (refreshed) {
            setStatus('Ready');
        }
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        appendTurn('system', 'System', message);
        setCaption(message);
        setStatus('Error');
    } finally {
        isSending = false;
        setBusy(false);
    }
}

async function activateHotspot(hotspot) {
    if (!hotspot) {
        return;
    }
    selectedHotspotId = hotspot.id;
    selectedNpcId = null;
    selectedTarget = { kind: 'hotspot', value: hotspot };
    showActionPrompt(selectedTarget);
    syncInteractionState();
    const action = hotspotCommand(hotspot);
    recordInteractionEvent('activate-hotspot', {
        target: targetTelemetry(selectedTarget),
    });
    if (action.kind === 'inspect') {
        setCaption(action.text);
        appendTurn('inspect', 'Inspect', action.text);
        recordInteractionEvent('inspect-hotspot', {
            target: targetTelemetry(selectedTarget),
            text: action.text,
        });
        return;
    }
    if (action.command) {
        commandInput.value = action.command;
        if (action.kind === 'travel') {
            setStatus('Moving');
            renderer?.startTransition();
            recordInteractionEvent('transition-start', {
                target: targetTelemetry(selectedTarget),
                command: action.command,
            });
            await delay(260);
        }
        await submitCommand(action.command);
    }
}

function activateNpc(npc) {
    if (!npc) {
        return;
    }
    selectedNpcId = npc.id;
    selectedHotspotId = null;
    selectedTarget = { kind: 'npc', value: npc };
    showActionPrompt(selectedTarget);
    syncInteractionState();
    const action = npcCommand(npc);
    commandInput.value = action.command;
    setCaption(`Ready to talk to ${action.label}.`);
    appendTurn('selection', 'Selected', `Ready to talk to ${action.label}.`);
    recordInteractionEvent('select-npc', {
        target: targetTelemetry(selectedTarget),
    });
}

function handlePointerTarget(target) {
    hoveredTarget = target || null;
    hoveredHotspotId = target?.kind === 'hotspot' ? target.value.id : null;
    hoveredNpcId = target?.kind === 'npc' ? target.value.id : null;
    syncInteractionState();
    const nextHoverTelemetryKey = target
        ? `${target.kind}:${target.value.id ?? target.value.label ?? ''}`
        : '';
    if (nextHoverTelemetryKey !== hoverTelemetryKey) {
        hoverTelemetryKey = nextHoverTelemetryKey;
        recordInteractionEvent('hover', {
            target: targetTelemetry(target),
        });
    }
    if (target?.kind === 'hotspot') {
        showActionPrompt(target);
        setCaption(target.value.label);
    } else if (target?.kind === 'npc') {
        showActionPrompt(target);
        setCaption(target.value.label);
    } else if (currentSceneModel.kind === 'scene') {
        if (selectedTarget) {
            showActionPrompt(selectedTarget);
        }
        setCaption(sceneCaption(currentSceneModel));
    }
}

function handleActivate(target) {
    if (target?.kind === 'npc') {
        activateNpc(target.value);
    } else if (target?.kind === 'hotspot') {
        activateHotspot(target.value);
    }
}

function activatePromptTarget() {
    handleActivate(promptTarget || hoveredTarget || selectedTarget);
}

settingsForm.addEventListener('submit', (event) => {
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

commandPanel.addEventListener('toggle', () => {
    if (commandPanel.open) {
        commandInput.focus();
    }
});

actionButton.addEventListener('click', () => {
    activatePromptTarget();
});

async function boot() {
    renderer = new PixiSceneRenderer({
        host: stageHost,
        onPointerTarget: handlePointerTarget,
        onActivate: handleActivate,
        proofAtomOnly,
    });
    await renderer.init();
    renderTurnLog();
    await refreshScene();
}

boot().catch((error) => {
    const message = error instanceof Error ? error.message : String(error);
    setStatus('Error');
    setCaption(message);
    appendTurn('system', 'System', message);
});

publishInteractionTelemetry();
