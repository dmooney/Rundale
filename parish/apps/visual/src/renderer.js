export const STAGE_WIDTH = 1280;
export const STAGE_HEIGHT = 720;
export const SPRITE_WIDTH = 48;
export const SPRITE_HEIGHT = 72;

function numberOr(value, fallback) {
    return Number.isFinite(value) ? value : fallback;
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

function rectBounds(shape) {
    if (!shape || !Array.isArray(shape.rect)) {
        return null;
    }
    const [x, y, width, height] = shape.rect;
    return {
        x: percentToPixels(x, STAGE_WIDTH),
        y: percentToPixels(y, STAGE_HEIGHT),
        width: percentToPixels(width, STAGE_WIDTH),
        height: percentToPixels(height, STAGE_HEIGHT),
    };
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
            hotspots: [],
            npcs: [],
            slots: [],
            overflow: [],
            plate: '',
            variant: '-',
            location: '-',
        };
    }

    const hotspots = (scene.hotspots || []).map((hotspot) => ({
        id: hotspot.id,
        label: hotspot.label || hotspot.id,
        action: actionLabel(hotspot.action),
        rawAction: hotspot.action || null,
        bounds: rectBounds(hotspot.shape),
    }));
    const slots = (scene.slots || []).map((slot) => ({
        id: slot.id,
        x: percentToPixels(slot.x, STAGE_WIDTH),
        y: percentToPixels(slot.y, STAGE_HEIGHT),
        scale: numberOr(slot.scale, 1),
        occupiedNpcId: slot.occupied_npc_id,
    }));
    const npcs = (scene.npcs || []).map((npc) => {
        const x = percentToPixels(npc.x, STAGE_WIDTH);
        const y = percentToPixels(npc.y, STAGE_HEIGHT);
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
            flip: Boolean(npc.flip),
            spriteUrl: npc.sprite_url,
            bounds: npcBounds(x, y, scale),
        };
    });

    return {
        kind: 'scene',
        title: scene.location_name || 'Unknown scene',
        subtitle: `${scene.variant || 'day'}${scene.indoor ? ' interior' : ' exterior'}`,
        location: scene.location_name || '-',
        variant: scene.variant || '-',
        plate: scene.plate_url || '',
        hotspots,
        slots,
        npcs,
        overflow: scene.overflow_npcs || [],
    };
}

export function findHotspotAtStagePoint(model, point) {
    if (!model || model.kind !== 'scene') {
        return null;
    }
    for (const hotspot of [...model.hotspots].reverse()) {
        const bounds = hotspot.bounds;
        if (!bounds) {
            continue;
        }
        if (
            point.x >= bounds.x &&
            point.x <= bounds.x + bounds.width &&
            point.y >= bounds.y &&
            point.y <= bounds.y + bounds.height
        ) {
            return hotspot;
        }
    }
    return null;
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

export function canvasPointToStage(canvas, clientX, clientY) {
    const rect = canvas.getBoundingClientRect();
    const width = rect.width || STAGE_WIDTH;
    const height = rect.height || STAGE_HEIGHT;
    return {
        x: ((clientX - rect.left) / width) * STAGE_WIDTH,
        y: ((clientY - rect.top) / height) * STAGE_HEIGHT,
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
    ctx.font = '14px ui-monospace, monospace';
    ctx.fillStyle = '#d8e5d7';
    ctx.textAlign = 'left';
    ctx.fillText(model.plate || 'Loading plate image', width * 0.12, height * 0.27);
}

function drawHotspots(ctx, model, width, height, activeHotspotId) {
    ctx.lineWidth = 2;
    ctx.font = '600 13px system-ui, sans-serif';
    ctx.textAlign = 'left';
    for (const hotspot of model.hotspots) {
        if (!hotspot.bounds) {
            continue;
        }
        const bounds = scaleBounds(hotspot.bounds, width, height);
        const active = hotspot.id === activeHotspotId;
        ctx.fillStyle = active ? 'rgba(245, 223, 131, 0.32)' : 'rgba(230, 187, 93, 0.18)';
        ctx.strokeStyle = active ? '#fff3a6' : '#e6bb5d';
        ctx.fillRect(bounds.x, bounds.y, bounds.width, bounds.height);
        ctx.lineWidth = active ? 3 : 2;
        ctx.strokeRect(bounds.x, bounds.y, bounds.width, bounds.height);
        ctx.fillStyle = '#fff7d6';
        ctx.fillText(hotspot.label, bounds.x + 8, bounds.y + 20);
    }
}

function drawSlots(ctx, model, width, height) {
    ctx.textAlign = 'center';
    ctx.font = '600 12px system-ui, sans-serif';
    for (const slot of model.slots) {
        const x = (slot.x / STAGE_WIDTH) * width;
        const y = (slot.y / STAGE_HEIGHT) * height;
        ctx.strokeStyle = 'rgba(246, 244, 234, 0.68)';
        ctx.beginPath();
        ctx.ellipse(x, y, 28 * slot.scale, 10 * slot.scale, 0, 0, Math.PI * 2);
        ctx.stroke();
        ctx.fillStyle = 'rgba(246, 244, 234, 0.72)';
        ctx.fillText(slot.id, x, y + 25);
    }
}

function imageForNpc(spriteImages, npc) {
    if (spriteImages instanceof Map) {
        return spriteImages.get(npc.id);
    }
    return spriteImages?.[npc.id];
}

function drawSpriteFallback(ctx, bounds, active) {
    ctx.fillStyle = active ? '#e6bb5d' : '#d95843';
    ctx.fillRect(
        bounds.x + bounds.width * 0.28,
        bounds.y + bounds.height * 0.2,
        bounds.width * 0.44,
        bounds.height * 0.36,
    );
    ctx.beginPath();
    ctx.arc(
        bounds.x + bounds.width / 2,
        bounds.y + bounds.height * 0.18,
        bounds.width * 0.22,
        0,
        Math.PI * 2,
    );
    ctx.fill();
}

function drawNpcs(ctx, model, width, height, options = {}) {
    ctx.textAlign = 'center';
    for (const npc of model.npcs) {
        const bounds = scaleBounds(npc.bounds, width, height);
        const active = npc.id === options.activeNpcId;
        const sprite = imageForNpc(options.spriteImages, npc);
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
        if (active) {
            ctx.strokeStyle = '#fff3a6';
            ctx.lineWidth = 2;
            ctx.strokeRect(bounds.x - 4, bounds.y - 4, bounds.width + 8, bounds.height + 8);
        }
        ctx.fillStyle = '#fffaf0';
        ctx.font = '700 13px system-ui, sans-serif';
        ctx.fillText(npc.label, bounds.x + bounds.width / 2, bounds.y - 10);
        if (npc.moodEmoji) {
            ctx.fillText(npc.moodEmoji, bounds.x + bounds.width / 2, bounds.y + bounds.height + 16);
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
    ctx.fillStyle = '#f4f1e6';
    ctx.font = '700 26px system-ui, sans-serif';
    ctx.textAlign = 'left';
    ctx.fillText(model.title, width * 0.12, height * 0.22);
    drawHotspots(ctx, model, width, height, options.activeHotspotId);
    drawSlots(ctx, model, width, height);
    drawNpcs(ctx, model, width, height, {
        activeNpcId: options.activeNpcId,
        spriteImages: options.spriteImages,
    });
    return model;
}

export function renderSceneCanvas(canvas, scene, options = {}) {
    const model = buildSceneDisplayModel(scene);
    return renderSceneModel(canvas, model, options);
}
