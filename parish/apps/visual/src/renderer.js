const STAGE_WIDTH = 1280;
const STAGE_HEIGHT = 720;

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
        bounds: rectBounds(hotspot.shape),
    }));
    const slots = (scene.slots || []).map((slot) => ({
        id: slot.id,
        x: percentToPixels(slot.x, STAGE_WIDTH),
        y: percentToPixels(slot.y, STAGE_HEIGHT),
        scale: numberOr(slot.scale, 1),
        occupiedNpcId: slot.occupied_npc_id,
    }));
    const npcs = (scene.npcs || []).map((npc) => ({
        id: npc.npc_id,
        slotId: npc.slot_id,
        label: npc.display_name || `NPC ${npc.npc_id}`,
        mood: npc.mood || 'present',
        x: percentToPixels(npc.x, STAGE_WIDTH),
        y: percentToPixels(npc.y, STAGE_HEIGHT),
        scale: numberOr(npc.scale, 1),
        spriteUrl: npc.sprite_url,
    }));

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

function drawBackground(ctx, width, height) {
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
    drawBackground(ctx, width, height);
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

function drawHotspots(ctx, model, width, height) {
    ctx.lineWidth = 2;
    ctx.font = '600 13px system-ui, sans-serif';
    ctx.textAlign = 'left';
    for (const hotspot of model.hotspots) {
        if (!hotspot.bounds) {
            continue;
        }
        const bounds = scaleBounds(hotspot.bounds, width, height);
        ctx.fillStyle = 'rgba(230, 187, 93, 0.18)';
        ctx.strokeStyle = '#e6bb5d';
        ctx.fillRect(bounds.x, bounds.y, bounds.width, bounds.height);
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

function drawNpcs(ctx, model, width, height) {
    ctx.textAlign = 'center';
    for (const npc of model.npcs) {
        const x = (npc.x / STAGE_WIDTH) * width;
        const y = (npc.y / STAGE_HEIGHT) * height;
        const radius = 18 * npc.scale;
        ctx.fillStyle = '#d95843';
        ctx.beginPath();
        ctx.arc(x, y - radius, radius, 0, Math.PI * 2);
        ctx.fill();
        ctx.fillStyle = '#fffaf0';
        ctx.font = '700 13px system-ui, sans-serif';
        ctx.fillText(npc.label, x, y - radius - 12);
    }
}

function drawScene(ctx, width, height, model) {
    drawBackground(ctx, width, height);
    ctx.fillStyle = '#f4f1e6';
    ctx.font = '700 26px system-ui, sans-serif';
    ctx.textAlign = 'left';
    ctx.fillText(model.title, width * 0.12, height * 0.22);
    ctx.font = '14px ui-monospace, monospace';
    ctx.fillStyle = '#d8e5d7';
    ctx.fillText(model.plate || 'No plate URL', width * 0.12, height * 0.27);
    drawHotspots(ctx, model, width, height);
    drawSlots(ctx, model, width, height);
    drawNpcs(ctx, model, width, height);
}

export function renderSceneCanvas(canvas, scene) {
    const model = buildSceneDisplayModel(scene);
    const { ctx, width, height } = syncCanvasPixels(canvas);
    ctx.clearRect(0, 0, width, height);
    if (model.kind === 'empty') {
        drawEmpty(ctx, width, height, model);
    } else {
        drawScene(ctx, width, height, model);
    }
    return model;
}
