export const STAGE_WIDTH = 1280;
export const STAGE_HEIGHT = 720;
export const SPRITE_WIDTH = 48;
export const SPRITE_HEIGHT = 72;
export const NPC_DEFAULT_Z = 50;
const ANIMATION_MODES = new Set(['drift', 'shimmer', 'flicker']);

function numberOr(value, fallback) {
    return Number.isFinite(value) ? value : fallback;
}

function clamp(value, min, max) {
    return Math.max(min, Math.min(max, value));
}

export function normalizeLayerAnimation(animation) {
    if (!animation || typeof animation !== 'object') {
        return null;
    }
    const mode = String(animation.mode || '').toLowerCase();
    if (!ANIMATION_MODES.has(mode)) {
        return null;
    }
    return {
        mode,
        amplitudeX: clamp(numberOr(animation.amplitude_x ?? animation.amplitudeX, 0), -24, 24),
        amplitudeY: clamp(numberOr(animation.amplitude_y ?? animation.amplitudeY, 0), -24, 24),
        alpha: clamp(numberOr(animation.alpha, 0), 0, 0.5),
        periodMs: clamp(Math.round(numberOr(animation.period_ms ?? animation.periodMs, 4000)), 250, 60000),
        phase: clamp(numberOr(animation.phase, 0), 0, 1),
    };
}

export function computeLayerAnimationFrame(animation, elapsedMs) {
    const normalized = normalizeLayerAnimation(animation);
    if (!normalized) {
        return { x: 0, y: 0, alpha: 0 };
    }
    const cycle = ((elapsedMs / normalized.periodMs + normalized.phase) % 1 + 1) % 1;
    const wave = Math.sin(cycle * Math.PI * 2);
    if (normalized.mode === 'flicker') {
        const secondary = Math.sin((cycle * 2.37 + 0.18) * Math.PI * 2);
        return {
            x: 0,
            y: 0,
            alpha: normalized.alpha * (wave * 0.55 + secondary * 0.45),
        };
    }
    if (normalized.mode === 'shimmer') {
        const offsetWave = Math.sin((cycle + 0.25) * Math.PI * 2);
        return {
            x: normalized.amplitudeX * wave,
            y: normalized.amplitudeY * offsetWave,
            alpha: normalized.alpha * Math.sin((cycle + 0.14) * Math.PI * 2),
        };
    }
    return {
        x: normalized.amplitudeX * wave,
        y: normalized.amplitudeY * wave,
        alpha: normalized.alpha * wave,
    };
}

function percentToPixels(value, total) {
    return Math.round((numberOr(value, 0) / 100) * total * 1000) / 1000;
}

function actionLabel(action) {
    if (!action || typeof action !== 'object') {
        return 'inspect';
    }
    if ('travel_to' in action) {
        return `travel:${action.travel_to}`;
    }
    if ('talk_to' in action) {
        return `talk:${action.talk_to}`;
    }
    if ('inspect' in action) {
        return 'inspect';
    }
    return 'action';
}

function destinationFromLabel(label) {
    const normalized = String(label || '').trim();
    const match = normalized.match(/\b(?:to|toward|towards)\s+(.+)$/i);
    return (match?.[1] || normalized).replace(/^the\s+/i, 'The ');
}

export function hotspotCommand(hotspot) {
    const activation = hotspot?.activation;
    if (activation?.kind === 'travel' && activation.command) {
        return {
            kind: 'travel',
            command: activation.command,
            label: hotspot.label,
        };
    }
    if (activation?.kind === 'inspect') {
        return {
            kind: 'inspect',
            text: String(activation.text || hotspot?.label || 'Nothing to inspect.'),
            label: hotspot?.label,
        };
    }
    if (activation?.kind === 'talk' && activation.command) {
        return {
            kind: 'talk',
            command: activation.command,
            label: hotspot?.label,
        };
    }

    const action = hotspot?.rawAction;
    if (!action || typeof action !== 'object') {
        return { kind: 'inspect', text: hotspot?.label || 'Inspect' };
    }
    if ('travel_to' in action) {
        return {
            kind: 'travel',
            command: `go to ${destinationFromLabel(hotspot.label)}`,
            label: hotspot.label,
        };
    }
    if ('inspect' in action) {
        return {
            kind: 'inspect',
            text: String(action.inspect || hotspot.label || 'Nothing to inspect.'),
            label: hotspot.label,
        };
    }
    if ('talk_to' in action) {
        return {
            kind: 'talk',
            command: `talk to ${hotspot.label}`,
            label: hotspot.label,
        };
    }
    return { kind: 'inspect', text: hotspot.label || 'Inspect' };
}

function rectBounds(shape, width = STAGE_WIDTH, height = STAGE_HEIGHT) {
    if (!shape || !Array.isArray(shape.rect)) {
        return null;
    }
    const [x, y, rectWidth, rectHeight] = shape.rect;
    return {
        x: percentToPixels(x, width),
        y: percentToPixels(y, height),
        width: percentToPixels(rectWidth, width),
        height: percentToPixels(rectHeight, height),
    };
}

function polygonPoints(shape, width = STAGE_WIDTH, height = STAGE_HEIGHT) {
    if (!shape || !Array.isArray(shape.polygon)) {
        return null;
    }
    return shape.polygon.map(([x, y]) => ({
        x: percentToPixels(x, width),
        y: percentToPixels(y, height),
    }));
}

function npcBounds(x, y, scale) {
    const width = SPRITE_WIDTH * scale;
    const height = SPRITE_HEIGHT * scale;
    return {
        x: Math.round((x - width / 2) * 1000) / 1000,
        y: Math.round((y - height) * 1000) / 1000,
        width: Math.round(width * 1000) / 1000,
        height: Math.round(height * 1000) / 1000,
    };
}

export function buildSceneDisplayModel(scene) {
    if (!scene) {
        return {
            kind: 'empty',
            title: 'No active diorama scene',
            subtitle: 'Diorama is disabled or this location has no scene.',
            stageWidth: STAGE_WIDTH,
            stageHeight: STAGE_HEIGHT,
            layers: [],
            hotspots: [],
            npcs: [],
            slots: [],
            overflow: [],
            plate: '',
            variant: '-',
            location: '-',
        };
    }

    const [stageWidth, stageHeight] = Array.isArray(scene.native_size)
        ? scene.native_size
        : [STAGE_WIDTH, STAGE_HEIGHT];
    const layers = (scene.layers || []).map((layer) => ({
        id: layer.id,
        assetId: layer.asset_id,
        kind: layer.kind || 'prop',
        assetUrl: layer.asset_url,
        x: percentToPixels(layer.x, stageWidth),
        y: percentToPixels(layer.y, stageHeight),
        xPercent: numberOr(layer.x, 0),
        yPercent: numberOr(layer.y, 0),
        z: Number.isFinite(layer.z) ? layer.z : 0,
        scale: numberOr(layer.scale, 1),
        opacity: numberOr(layer.opacity, 1),
        flip: Boolean(layer.flip),
        anchor: Array.isArray(layer.anchor) ? layer.anchor : [50, 100],
        animation: normalizeLayerAnimation(layer.animation),
        labels: (layer.labels || []).map((label) => ({
            text: label.text || '',
            anchor: Array.isArray(label.anchor) ? label.anchor : [50, 50],
            rotation: numberOr(label.rotation, 0),
        })),
    }));
    const hotspots = (scene.hotspots || []).map((hotspot) => ({
        id: hotspot.id,
        label: hotspot.label || hotspot.id,
        action: actionLabel(hotspot.action),
        rawAction: hotspot.action || null,
        activation: hotspot.activation || null,
        bounds: rectBounds(hotspot.shape, stageWidth, stageHeight),
        polygon: polygonPoints(hotspot.shape, stageWidth, stageHeight),
    }));
    const slots = (scene.slots || []).map((slot) => ({
        id: slot.id,
        x: percentToPixels(slot.x, stageWidth),
        y: percentToPixels(slot.y, stageHeight),
        scale: numberOr(slot.scale, 1),
        occupiedNpcId: slot.occupied_npc_id,
    }));
    const npcs = (scene.npcs || []).map((npc) => {
        const x = percentToPixels(npc.x, stageWidth);
        const y = percentToPixels(npc.y, stageHeight);
        const scale = numberOr(npc.scale, 1);
        return {
            id: npc.npc_id,
            slotId: npc.slot_id,
            label: npc.display_name || `NPC ${npc.npc_id}`,
            mood: npc.mood || 'present',
            moodEmoji: npc.mood_emoji || '',
            x,
            y,
            scale,
            z: Number.isFinite(npc.z) ? npc.z : NPC_DEFAULT_Z,
            depthY: y,
            flip: Boolean(npc.flip),
            spriteUrl: npc.sprite_url,
            bounds: npcBounds(x, y, scale),
        };
    });

    return {
        kind: 'scene',
        title: scene.location_name || 'Unknown scene',
        subtitle: `${scene.variant || 'day'}${scene.indoor ? ' interior' : ' exterior'}`,
        slug: scene.slug || '',
        stageWidth,
        stageHeight,
        location: scene.location_name || '-',
        variant: scene.variant || '-',
        plate: scene.plate_url || '',
        underlay: scene.underlay_url || '',
        weatherOverlay: scene.weather_overlay || null,
        layers,
        hotspots,
        slots,
        npcs,
        overflow: scene.overflow_npcs || [],
    };
}

export function buildWorldDrawList(model) {
    if (!model || model.kind !== 'scene') {
        return [];
    }
    const layerDrawables = model.layers.map((layer, index) => ({
        kind: 'layer',
        id: `layer:${layer.id}`,
        source: layer,
        z: Number.isFinite(layer.z) ? layer.z : 0,
        depthY: Number.isFinite(layer.y) ? layer.y : 0,
        stableOrder: index,
    }));
    const npcDrawables = model.npcs.map((npc, index) => ({
        kind: 'npc',
        id: `npc:${npc.id}`,
        source: npc,
        z: Number.isFinite(npc.z) ? npc.z : NPC_DEFAULT_Z,
        depthY: Number.isFinite(npc.depthY) ? npc.depthY : npc.y,
        stableOrder: model.layers.length + index,
    }));
    return [...layerDrawables, ...npcDrawables].sort((a, b) => {
        const z = a.z - b.z;
        if (z !== 0) {
            return z;
        }
        const depth = a.depthY - b.depthY;
        if (depth !== 0) {
            return depth;
        }
        return a.stableOrder - b.stableOrder;
    });
}

export function findHotspotAtStagePoint(model, point) {
    if (!model || model.kind !== 'scene') {
        return null;
    }
    for (const hotspot of [...model.hotspots].reverse()) {
        if (hotspot.polygon && pointInPolygon(point, hotspot.polygon)) {
            return hotspot;
        }
        const bounds = hotspot.bounds;
        if (bounds) {
            if (
                point.x >= bounds.x &&
                point.x <= bounds.x + bounds.width &&
                point.y >= bounds.y &&
                point.y <= bounds.y + bounds.height
            ) {
                return hotspot;
            }
        }
    }
    return null;
}

export function pointInPolygon(point, polygon) {
    let inside = false;
    for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i++) {
        const xi = polygon[i].x;
        const yi = polygon[i].y;
        const xj = polygon[j].x;
        const yj = polygon[j].y;
        const intersects =
            yi > point.y !== yj > point.y &&
            point.x < ((xj - xi) * (point.y - yi)) / (yj - yi || 1) + xi;
        if (intersects) {
            inside = !inside;
        }
    }
    return inside;
}

export function findNpcAtStagePoint(model, point) {
    if (!model || model.kind !== 'scene') {
        return null;
    }
    for (const npc of [...model.npcs].reverse()) {
        const bounds = npc.bounds;
        if (!bounds) {
            continue;
        }
        if (
            point.x >= bounds.x &&
            point.x <= bounds.x + bounds.width &&
            point.y >= bounds.y &&
            point.y <= bounds.y + bounds.height
        ) {
            return npc;
        }
    }
    return null;
}

export function findSceneTargetAtStagePoint(model, point) {
    const npc = findNpcAtStagePoint(model, point);
    if (npc) {
        return { kind: 'npc', value: npc };
    }
    const hotspot = findHotspotAtStagePoint(model, point);
    if (hotspot) {
        return { kind: 'hotspot', value: hotspot };
    }
    return null;
}

export function npcCommand(npc) {
    return {
        kind: 'talk',
        command: `talk to ${npc?.label || 'them'}`,
        label: npc?.label || 'them',
    };
}

export function canvasPointToStage(canvas, clientX, clientY, model = null) {
    const rect = canvas.getBoundingClientRect();
    const width = rect.width || model?.stageWidth || STAGE_WIDTH;
    const height = rect.height || model?.stageHeight || STAGE_HEIGHT;
    const stageWidth = model?.stageWidth || STAGE_WIDTH;
    const stageHeight = model?.stageHeight || STAGE_HEIGHT;
    return {
        x: ((clientX - rect.left) / width) * stageWidth,
        y: ((clientY - rect.top) / height) * stageHeight,
    };
}

function syncCanvasPixels(canvas) {
    const ratio = Math.min(globalThis.devicePixelRatio || 1, 2);
    const width = Math.max(1, Math.floor(canvas.clientWidth || STAGE_WIDTH));
    const height = Math.max(1, Math.floor(canvas.clientHeight || STAGE_HEIGHT));
    const pixelWidth = Math.floor(width * ratio);
    const pixelHeight = Math.floor(height * ratio);
    if (canvas.width !== pixelWidth || canvas.height !== pixelHeight) {
        canvas.width = pixelWidth;
        canvas.height = pixelHeight;
    }
    const ctx = canvas.getContext('2d');
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    return { ctx, width, height };
}

function drawFallbackPlate(ctx, width, height) {
    ctx.fillStyle = '#17211b';
    ctx.fillRect(0, 0, width, height);
    ctx.fillStyle = '#26442e';
    ctx.fillRect(width * 0.08, height * 0.12, width * 0.84, height * 0.76);
    ctx.fillStyle = '#3f7d42';
    ctx.fillRect(width * 0.1, height * 0.15, width * 0.8, height * 0.7);
    ctx.fillStyle = 'rgba(246, 244, 234, 0.18)';
    ctx.fillRect(width * 0.1, height * 0.15, width * 0.8, 2);
}

function drawEmpty(ctx, width, height, model) {
    drawFallbackPlate(ctx, width, height);
    ctx.fillStyle = '#f4f1e6';
    ctx.font = '600 28px system-ui, sans-serif';
    ctx.textAlign = 'center';
    ctx.fillText(model.title, width / 2, height / 2 - 12);
    ctx.font = '16px system-ui, sans-serif';
    ctx.fillStyle = '#cbd6cc';
    ctx.fillText(model.subtitle, width / 2, height / 2 + 20);
}

function scaleBounds(bounds, width, height) {
    return {
        x: (bounds.x / STAGE_WIDTH) * width,
        y: (bounds.y / STAGE_HEIGHT) * height,
        width: (bounds.width / STAGE_WIDTH) * width,
        height: (bounds.height / STAGE_HEIGHT) * height,
    };
}

function drawPlate(ctx, model, width, height, plateImage) {
    if (plateImage?.complete && plateImage.naturalWidth > 0) {
        ctx.drawImage(plateImage, 0, 0, width, height);
        ctx.fillStyle = 'rgba(0, 0, 0, 0.12)';
        ctx.fillRect(0, 0, width, height);
        return;
    }
    drawFallbackPlate(ctx, width, height);
}

function hotspotCueKind(hotspot) {
    const command = hotspotCommand(hotspot);
    return command.kind === 'travel' ? 'travel' : 'inspect';
}

function drawCanvasHotspotCue(ctx, hotspot, bounds, selected) {
    const cueKind = hotspotCueKind(hotspot);
    const centerX = bounds.x + bounds.width / 2;
    const centerY =
        cueKind === 'travel' ? bounds.y + bounds.height * 0.82 : bounds.y + bounds.height / 2;

    ctx.save();
    ctx.lineWidth = selected ? 3 : 2;
    ctx.strokeStyle = selected ? 'rgba(255, 244, 184, 0.92)' : 'rgba(236, 216, 136, 0.76)';
    ctx.fillStyle = selected ? 'rgba(255, 232, 130, 0.14)' : 'rgba(255, 232, 130, 0.08)';

    if (cueKind === 'travel') {
        const radiusX = clamp(bounds.width * 0.22, 24, 58);
        const radiusY = clamp(bounds.height * 0.08, 7, 18);
        ctx.beginPath();
        ctx.ellipse(centerX, centerY, radiusX, radiusY, 0, 0, Math.PI * 2);
        ctx.fill();
        ctx.stroke();
        ctx.beginPath();
        ctx.ellipse(centerX, centerY, radiusX * 0.62, radiusY * 0.56, 0, 0, Math.PI * 2);
        ctx.stroke();
    } else {
        const radius = clamp(Math.min(bounds.width, bounds.height) * 0.26, 18, 34);
        const gaps = [
            [-0.18, 1.05],
            [1.42, 2.66],
            [3.1, 4.32],
            [4.72, 5.78],
        ];
        for (const [start, end] of gaps) {
            ctx.beginPath();
            ctx.arc(centerX, centerY, radius, start, end);
            ctx.stroke();
        }
    }

    ctx.restore();
}

function drawHotspots(ctx, model, width, height, options = {}) {
    for (const hotspot of model.hotspots) {
        if (!hotspot.bounds) {
            continue;
        }
        const bounds = scaleBounds(hotspot.bounds, width, height);
        const active = hotspot.id === options.activeHotspotId;
        const selected = hotspot.id === options.selectedHotspotId;
        if (!active && !selected) {
            continue;
        }
        drawCanvasHotspotCue(ctx, hotspot, bounds, selected);
    }
}

function imageForNpc(spriteImages, npc) {
    if (spriteImages instanceof Map) {
        return spriteImages.get(npc.id);
    }
    return spriteImages?.[npc.id];
}

function drawCanvasNpcCue(ctx, bounds, selected) {
    const centerX = bounds.x + bounds.width / 2;
    const footY = bounds.y + bounds.height - 4;
    ctx.save();
    ctx.lineWidth = selected ? 3 : 2;
    ctx.fillStyle = selected ? 'rgba(255, 232, 130, 0.16)' : 'rgba(255, 232, 130, 0.1)';
    ctx.strokeStyle = selected ? 'rgba(255, 244, 184, 0.88)' : 'rgba(236, 216, 136, 0.68)';
    ctx.beginPath();
    ctx.ellipse(centerX, footY, Math.max(18, bounds.width * 0.55), 8, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
    ctx.restore();
}

function drawNpcCaption(ctx, npc, bounds, selected) {
    ctx.save();
    ctx.textAlign = 'center';
    ctx.font = '700 13px system-ui, sans-serif';
    ctx.fillStyle = selected ? '#fff4b8' : '#fffaf0';
    ctx.fillText(npc.label, bounds.x + bounds.width / 2, bounds.y - 10);
    ctx.restore();
}

function drawSpriteFallback(ctx, bounds, active) {
    const centerX = bounds.x + bounds.width / 2;
    const top = bounds.y;
    const unit = bounds.width / 12;

    ctx.save();
    ctx.fillStyle = active ? '#e0bf76' : '#d2ad76';
    ctx.fillRect(centerX - unit * 2.1, top + unit * 1.7, unit * 4.2, unit * 3.1);
    ctx.fillStyle = '#24211d';
    ctx.fillRect(centerX - unit * 2.5, top + unit * 0.6, unit * 5, unit * 1.6);
    ctx.fillStyle = active ? '#4d4936' : '#3c4034';
    ctx.fillRect(centerX - unit * 3.1, top + unit * 5.1, unit * 6.2, unit * 6.4);
    ctx.fillStyle = '#253036';
    ctx.fillRect(centerX - unit * 2.6, top + unit * 11.3, unit * 2.1, unit * 5.2);
    ctx.fillRect(centerX + unit * 0.5, top + unit * 11.3, unit * 2.1, unit * 5.2);
    ctx.fillStyle = '#1b1915';
    ctx.fillRect(centerX - unit * 3, top + unit * 16.2, unit * 2.8, unit);
    ctx.fillRect(centerX + unit * 0.2, top + unit * 16.2, unit * 2.8, unit);
    ctx.restore();
}

function drawNpcs(ctx, model, width, height, options = {}) {
    ctx.textAlign = 'center';
    for (const npc of model.npcs) {
        const bounds = scaleBounds(npc.bounds, width, height);
        const active = npc.id === options.activeNpcId;
        const selected = npc.id === options.selectedNpcId;
        const sprite = imageForNpc(options.spriteImages, npc);
        if (active || selected) {
            drawCanvasNpcCue(ctx, bounds, selected);
        }
        if (sprite?.complete && sprite.naturalWidth > 0) {
            ctx.save();
            if (npc.flip) {
                ctx.translate(bounds.x + bounds.width, bounds.y);
                ctx.scale(-1, 1);
                ctx.drawImage(sprite, 0, 0, bounds.width, bounds.height);
            } else {
                ctx.drawImage(sprite, bounds.x, bounds.y, bounds.width, bounds.height);
            }
            ctx.restore();
        } else {
            drawSpriteFallback(ctx, bounds, active);
        }
        if (active || selected) {
            drawNpcCaption(ctx, npc, bounds, selected);
        }
    }
}

export function renderSceneModel(canvas, model, options = {}) {
    const { ctx, width, height } = syncCanvasPixels(canvas);
    ctx.clearRect(0, 0, width, height);

    if (model.kind === 'empty') {
        drawEmpty(ctx, width, height, model);
        return model;
    }

    drawPlate(ctx, model, width, height, options.plateImage);
    drawHotspots(ctx, model, width, height, {
        activeHotspotId: options.activeHotspotId,
        selectedHotspotId: options.selectedHotspotId,
    });
    drawNpcs(ctx, model, width, height, {
        activeNpcId: options.activeNpcId,
        selectedNpcId: options.selectedNpcId,
        spriteImages: options.spriteImages,
    });
    return model;
}

export function renderSceneCanvas(canvas, scene, options = {}) {
    const model = buildSceneDisplayModel(scene);
    return renderSceneModel(canvas, model, options);
}
