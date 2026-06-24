import * as PIXI from '/vendor/pixi.mjs';
import { buildWorldDrawList, computeLayerAnimationFrame, pointInPolygon } from './renderer.js';

const BASE_NPC_WIDTH = 48;
const BASE_NPC_HEIGHT = 72;
const NPC_CUE_ASSET = '/assets/cues/npc-select.png';
const HOTSPOT_CUE_ASSETS = {
    travel: '/assets/cues/travel-hover.png',
    inspect: '/assets/cues/inspect-hover.png',
    talk: '/assets/cues/inspect-hover.png',
};
const COMPOSITOR_TELEMETRY_KEY = '__rundaleVisualCompositor';

function resolveAssetUrl(url) {
    if (!url || /^https?:\/\//i.test(url) || url.startsWith('data:')) {
        return url;
    }
    return url.startsWith('/') ? url : `/${url}`;
}

function textureSize(texture) {
    return {
        width: texture?.width || 1,
        height: texture?.height || 1,
    };
}

function clamp(value, min, max) {
    return Math.max(min, Math.min(max, value));
}

function isStageLayer(layer) {
    return ['underlay', 'plate', 'sky', 'ground'].includes(layer.kind);
}

function preparePixelTexture(texture) {
    if (texture?.source) {
        texture.source.scaleMode = 'nearest';
        texture.source.mipmapFilter = 'nearest';
    }
    return texture;
}

async function loadTexture(url) {
    const resolved = resolveAssetUrl(url);
    if (!resolved) {
        return null;
    }
    try {
        return preparePixelTexture(await PIXI.Assets.load(resolved));
    } catch (_error) {
        return null;
    }
}

function clearContainer(container) {
    for (const child of container.removeChildren()) {
        child.destroy({ children: true });
    }
}

function applyAnchor(sprite, anchor) {
    sprite.anchor.set((anchor?.[0] ?? 50) / 100, (anchor?.[1] ?? 100) / 100);
}

function boundsForHotspot(hotspot) {
    if (hotspot.bounds) {
        return hotspot.bounds;
    }
    if (!hotspot.polygon?.length) {
        return null;
    }
    const xs = hotspot.polygon.map((point) => point.x);
    const ys = hotspot.polygon.map((point) => point.y);
    const x = Math.min(...xs);
    const y = Math.min(...ys);
    return {
        x,
        y,
        width: Math.max(...xs) - x,
        height: Math.max(...ys) - y,
    };
}

function hotspotKind(hotspot) {
    return hotspot.activation?.kind || String(hotspot.action || '').split(':', 1)[0] || 'inspect';
}

function cueKindForHotspot(hotspot) {
    const kind = hotspotKind(hotspot);
    return kind === 'travel' ? 'travel' : 'inspect';
}

function hotspotCuePlacement(hotspot, selected) {
    const bounds = boundsForHotspot(hotspot);
    if (!bounds) {
        return null;
    }
    const kind = cueKindForHotspot(hotspot);
    const centerX = bounds.x + bounds.width / 2;
    const centerY = bounds.y + bounds.height / 2;
    const selectedScale = selected ? 1.08 : 1;
    if (kind === 'travel') {
        return {
            kind,
            x: centerX,
            y: centerY,
            width: clamp(bounds.width * 0.44, 76, 156) * selectedScale,
            height: clamp(bounds.height * 0.16, 28, 54) * selectedScale,
        };
    }
    const size = clamp(Math.min(bounds.width, bounds.height) * 0.54, 42, 92) * selectedScale;
    return {
        kind,
        x: centerX,
        y: centerY,
        width: size,
        height: size,
    };
}

function drawNpcFallback(npc, active) {
    const s = npc.scale;
    const seed = Number(npc.id || 0);
    const coat = active ? 0x6f5a35 : [0x4a3a2b, 0x394332, 0x3a3440, 0x5a3b30][seed % 4];
    const trim = active ? 0xd9b468 : [0x8b7a56, 0x6f7b54, 0x7a6f61, 0x8a6348][seed % 4];
    const skin = [0xb78361, 0xc69a78, 0x9b654a][seed % 3];
    const hair = [0x2d2119, 0x4c3322, 0x6b5430][seed % 3];
    const figure = new PIXI.Graphics();

    figure.ellipse(0, -3 * s, 22 * s, 7 * s).fill({ color: 0x0d0c0a, alpha: 0.36 });
    figure.rect(-11 * s, -42 * s, 22 * s, 35 * s).fill(coat);
    figure.rect(-15 * s, -36 * s, 5 * s, 25 * s).fill(0x2f281f);
    figure.rect(10 * s, -36 * s, 5 * s, 25 * s).fill(0x2f281f);
    figure.rect(-9 * s, -7 * s, 7 * s, 10 * s).fill(0x211b17);
    figure.rect(2 * s, -7 * s, 7 * s, 10 * s).fill(0x211b17);
    figure.rect(-9 * s, -56 * s, 18 * s, 16 * s).fill(skin);
    figure.rect(-11 * s, -60 * s, 22 * s, 7 * s).fill(hair);
    figure.rect(-14 * s, -65 * s, 28 * s, 5 * s).fill(0x2b241b);
    figure.rect(-7 * s, -71 * s, 14 * s, 8 * s).fill(0x2b241b);
    figure.rect(-7 * s, -37 * s, 14 * s, 4 * s).fill(trim);
    figure.rect(-8 * s, -29 * s, 16 * s, 2 * s).fill(0x2a231b);
    figure.setStrokeStyle({ width: Math.max(1, 2 * s), color: 0x17130f, alpha: 0.85 });
    figure.moveTo(-11 * s, -42 * s).lineTo(-11 * s, -7 * s).lineTo(11 * s, -7 * s).lineTo(11 * s, -42 * s);
    figure.stroke();
    return figure;
}

function shouldUseFallbackNpcSprite(npc) {
    return !npc.spriteUrl;
}

function textStyle(size, fill = 0xfff7da) {
    return {
        fontFamily: 'Georgia, "Times New Roman", serif',
        fontSize: size,
        fontWeight: '700',
        fill,
        stroke: { color: 0x2c2118, width: Math.max(2, Math.floor(size / 8)) },
        dropShadow: {
            color: 0x000000,
            alpha: 0.3,
            blur: 3,
            distance: 2,
        },
    };
}

export class PixiSceneRenderer {
    constructor({ host, onPointerTarget = () => {}, onActivate = () => {}, proofAtomOnly = false }) {
        this.host = host;
        this.onPointerTarget = onPointerTarget;
        this.onActivate = onActivate;
        this.proofAtomOnly = Boolean(proofAtomOnly);
        this.app = null;
        this.root = new PIXI.Container();
        this.worldContainer = new PIXI.Container();
        this.hotspotContainer = new PIXI.Container();
        this.overlayContainer = new PIXI.Container();
        this.transition = new PIXI.Graphics();
        this.npcContainers = new Map();
        this.hotspotCueTextures = new Map();
        this.npcCueTexture = null;
        this.animatedLayers = [];
        this.animationElapsedMs = 0;
        this.model = null;
        this.activeHotspotId = null;
        this.selectedHotspotId = null;
        this.activeNpcId = null;
        this.selectedNpcId = null;
        this.fit = { scale: 1, x: 0, y: 0 };
        this.compositorTelemetry = this.createCompositorTelemetry(null);
    }

    createCompositorTelemetry(model) {
        return {
            mode: this.proofAtomOnly ? 'atom-only' : 'normal',
            slug: model?.slug || '',
            location: model?.location || '',
            layerSprites: [],
            npcSprites: [],
            hotspotCues: [],
            fallbackUnderlayUsed: false,
            fallbackPlateUsed: false,
            fallbackAssetUrl: '',
            missingLayerAssets: [],
            drawListCount: 0,
        };
    }

    publishCompositorTelemetry() {
        globalThis[COMPOSITOR_TELEMETRY_KEY] = {
            ...this.compositorTelemetry,
            layerSprites: [...this.compositorTelemetry.layerSprites],
            npcSprites: [...this.compositorTelemetry.npcSprites],
            hotspotCues: [...this.compositorTelemetry.hotspotCues],
            missingLayerAssets: [...this.compositorTelemetry.missingLayerAssets],
        };
    }

    async init() {
        this.app = new PIXI.Application();
        await this.app.init({
            resizeTo: this.host,
            antialias: false,
            background: '#11130f',
            preference: 'webgl',
        });
        await Promise.all([this.loadHotspotCueTextures(), this.loadNpcCueTexture()]);
        this.app.canvas.setAttribute('aria-label', 'Current Rundale scene');
        this.app.canvas.className = 'game-canvas';
        this.host.append(this.app.canvas);
        this.app.stage.addChild(this.root, this.overlayContainer, this.transition);
        this.root.addChild(this.worldContainer, this.hotspotContainer);
        this.app.canvas.addEventListener('pointermove', (event) => this.handlePointerMove(event));
        this.app.canvas.addEventListener('pointerleave', () => this.handlePointerLeave());
        this.app.canvas.addEventListener('click', (event) => this.handleClick(event));
        this.app.ticker.add((time) => this.updateLayerAnimations(time.deltaMS));
        window.addEventListener('resize', () => this.resize());
    }

    async loadHotspotCueTextures() {
        const entries = await Promise.all(
            Object.entries(HOTSPOT_CUE_ASSETS).map(async ([kind, url]) => [kind, await loadTexture(url)]),
        );
        this.hotspotCueTextures = new Map(entries.filter(([, texture]) => texture));
    }

    async loadNpcCueTexture() {
        this.npcCueTexture = await loadTexture(NPC_CUE_ASSET);
    }

    async setScene(model, options = {}) {
        this.model = model;
        this.activeHotspotId = options.activeHotspotId || null;
        this.selectedHotspotId = options.selectedHotspotId || null;
        this.activeNpcId = options.activeNpcId || null;
        this.selectedNpcId = options.selectedNpcId || null;
        this.compositorTelemetry = this.createCompositorTelemetry(model);
        clearContainer(this.worldContainer);
        clearContainer(this.hotspotContainer);
        clearContainer(this.overlayContainer);
        this.transition.clear();
        this.npcContainers.clear();
        this.animatedLayers = [];
        this.animationElapsedMs = 0;

        if (model.kind === 'empty') {
            this.drawEmpty(model);
            this.resize();
            this.publishCompositorTelemetry();
            return;
        }

        await this.drawWorld(model);
        this.drawWeather(model);
        this.drawHotspots();
        this.resize();
        this.flashTransition();
        this.publishCompositorTelemetry();
    }

    setInteractionState({ activeHotspotId, selectedHotspotId, activeNpcId, selectedNpcId }) {
        this.activeHotspotId = activeHotspotId || null;
        this.selectedHotspotId = selectedHotspotId || null;
        this.activeNpcId = activeNpcId || null;
        this.selectedNpcId = selectedNpcId || null;
        this.drawHotspots();
        if (this.model?.kind === 'scene') {
            this.drawNpcHighlights();
        }
        this.publishCompositorTelemetry();
    }

    resize() {
        if (!this.app || !this.model) {
            return;
        }
        const screen = this.app.screen;
        const stageWidth = this.model.stageWidth || 1280;
        const stageHeight = this.model.stageHeight || 720;
        const scale = Math.max(screen.width / stageWidth, screen.height / stageHeight);
        let x = Math.round((screen.width - stageWidth * scale) / 2);
        const y = Math.round((screen.height - stageHeight * scale) / 2);
        if (screen.width < 700 && screen.height > screen.width && this.model.kind === 'scene') {
            const focusX = this.model.slug === 'kilteevan-village' ? stageWidth * 0.42 : stageWidth * 0.5;
            const minX = screen.width - stageWidth * scale;
            x = Math.round(screen.width / 2 - focusX * scale);
            x = Math.min(0, Math.max(minX, x));
        }
        this.fit = { scale, x, y };
        this.root.position.set(x, y);
        this.root.scale.set(scale);
        this.overlayContainer.position.set(x, y);
        this.overlayContainer.scale.set(scale);
        this.transition.clear();
        this.transition.rect(0, 0, screen.width, screen.height).fill({ color: 0x050604, alpha: 0 });
    }

    async drawWorld(model) {
        if (!this.proofAtomOnly && model.underlay && model.layers.length === 0) {
            const texture = await loadTexture(model.underlay);
            if (texture) {
                const underlay = new PIXI.Sprite(texture);
                underlay.width = model.stageWidth;
                underlay.height = model.stageHeight;
                underlay.alpha = 0.98;
                this.worldContainer.addChild(underlay);
                this.compositorTelemetry.fallbackUnderlayUsed = true;
                this.compositorTelemetry.fallbackAssetUrl = model.underlay;
            }
        }

        const drawList = buildWorldDrawList(model);
        this.compositorTelemetry.drawListCount = drawList.length;
        for (const drawable of drawList) {
            if (drawable.kind === 'layer') {
                await this.drawLayer(drawable.source, model);
            } else if (drawable.kind === 'npc') {
                await this.drawNpc(drawable.source);
            }
        }

        if (!this.proofAtomOnly && this.worldContainer.children.length === 0 && model.plate) {
            const texture = await loadTexture(model.plate);
            if (texture) {
                const fallback = new PIXI.Sprite(texture);
                fallback.width = model.stageWidth;
                fallback.height = model.stageHeight;
                fallback.alpha = 0.96;
                this.worldContainer.addChild(fallback);
                this.compositorTelemetry.fallbackPlateUsed = true;
                this.compositorTelemetry.fallbackAssetUrl = model.plate;
            }
        }
        this.drawNpcHighlights();
    }

    async drawLayer(layer, model) {
        const texture = await loadTexture(layer.assetUrl);
        if (!texture) {
            this.compositorTelemetry.missingLayerAssets.push({
                id: layer.id,
                assetId: layer.assetId,
                kind: layer.kind,
                assetUrl: layer.assetUrl,
            });
            return;
        }
        const sprite = new PIXI.Sprite(texture);
        if (isStageLayer(layer)) {
            sprite.anchor.set(0, 0);
            sprite.position.set(0, 0);
            sprite.width = model.stageWidth;
            sprite.height = model.stageHeight;
        } else {
            applyAnchor(sprite, layer.anchor);
            sprite.position.set(layer.x, layer.y);
            sprite.scale.set(layer.flip ? -layer.scale : layer.scale, layer.scale);
        }
        sprite.alpha = layer.opacity;
        sprite.label = layer.id;
        this.worldContainer.addChild(sprite);
        this.compositorTelemetry.layerSprites.push({
            id: layer.id,
            assetId: layer.assetId,
            kind: layer.kind,
            assetUrl: layer.assetUrl,
            stageLayer: isStageLayer(layer),
            z: layer.z,
            width: Math.round(sprite.width),
            height: Math.round(sprite.height),
            opacity: layer.opacity,
        });
        if (layer.animation) {
            this.animatedLayers.push({
                sprite,
                animation: layer.animation,
                baseX: sprite.position.x,
                baseY: sprite.position.y,
                baseAlpha: sprite.alpha,
            });
        }
        this.drawLayerLabels(layer, texture);
    }

    updateLayerAnimations(deltaMs = 0) {
        if (!this.animatedLayers.length) {
            return;
        }
        this.animationElapsedMs += deltaMs;
        for (const layer of this.animatedLayers) {
            const frame = computeLayerAnimationFrame(layer.animation, this.animationElapsedMs);
            layer.sprite.position.set(Math.round(layer.baseX + frame.x), Math.round(layer.baseY + frame.y));
            layer.sprite.alpha = Math.max(0, Math.min(1, layer.baseAlpha + frame.alpha));
        }
    }

    drawLayerLabels(layer, texture) {
        const size = textureSize(texture);
        for (const label of layer.labels || []) {
            if (!label.text) {
                continue;
            }
            const anchorX = (layer.anchor?.[0] ?? 50) / 100;
            const anchorY = (layer.anchor?.[1] ?? 100) / 100;
            const labelX = layer.x + ((label.anchor[0] / 100 - anchorX) * size.width * layer.scale);
            const labelY = layer.y + ((label.anchor[1] / 100 - anchorY) * size.height * layer.scale);
            const text = new PIXI.Text({
                text: label.text,
                style: textStyle(16, 0x2b2115),
            });
            text.anchor.set(0.5, 0.5);
            text.position.set(labelX, labelY);
            text.rotation = (label.rotation * Math.PI) / 180;
            this.worldContainer.addChild(text);
        }
    }

    async drawNpc(npc) {
        const container = new PIXI.Container();
        container.position.set(npc.x, npc.y);
        container.label = `npc-${npc.id}`;
        container.__npcId = npc.id;
        const texture = shouldUseFallbackNpcSprite(npc) ? null : await loadTexture(npc.spriteUrl);
        if (texture) {
            const sprite = new PIXI.Sprite(texture);
            sprite.anchor.set(0.5, 1);
            const widthScale = (BASE_NPC_WIDTH * npc.scale) / Math.max(1, texture.width);
            const heightScale = (BASE_NPC_HEIGHT * npc.scale) / Math.max(1, texture.height);
            sprite.scale.set(npc.flip ? -widthScale : widthScale, heightScale);
            container.addChild(sprite);
        } else {
            container.addChild(drawNpcFallback(npc, false));
        }
        this.compositorTelemetry.npcSprites.push({
            id: npc.id,
            label: npc.label,
            slotId: npc.slotId,
            spriteUrl: npc.spriteUrl || '',
            fallback: !texture,
        });
        const label = new PIXI.Text({ text: npc.label, style: textStyle(11) });
        label.__isNpcLabel = true;
        label.alpha = 0;
        label.anchor.set(0.5, 1);
        label.position.set(0, -BASE_NPC_HEIGHT * npc.scale - 8);
        container.addChild(label);
        this.worldContainer.addChild(container);
        this.npcContainers.set(npc.id, container);
    }

    drawNpcHighlights() {
        for (const child of this.npcContainers.values()) {
            for (const existing of child.children.filter((candidate) => candidate.__isNpcHighlight)) {
                child.removeChild(existing);
                existing.destroy();
            }
            const active = child.__npcId === this.activeNpcId || child.__npcId === this.selectedNpcId;
            for (const label of child.children.filter((candidate) => candidate.__isNpcLabel)) {
                label.alpha = active ? 1 : 0;
            }
            if (!active) {
                continue;
            }
            if (!this.npcCueTexture) {
                continue;
            }
            const selected = child.__npcId === this.selectedNpcId;
            const cue = new PIXI.Sprite(this.npcCueTexture);
            cue.__isNpcHighlight = true;
            cue.anchor.set(0.5);
            cue.position.set(0, -5);
            cue.width = selected ? 82 : 74;
            cue.height = selected ? 38 : 34;
            cue.alpha = selected ? 0.96 : 0.78;
            child.addChildAt(cue, 0);
        }
    }

    drawHotspots() {
        clearContainer(this.hotspotContainer);
        this.compositorTelemetry.hotspotCues = [];
        if (!this.model || this.model.kind !== 'scene') {
            return;
        }
        for (const hotspot of this.model.hotspots) {
            const active = hotspot.id === this.activeHotspotId;
            const selected = hotspot.id === this.selectedHotspotId;
            if (!active && !selected) {
                continue;
            }
            const placement = hotspotCuePlacement(hotspot, selected);
            const texture = placement ? this.hotspotCueTextures.get(placement.kind) : null;
            if (!placement || !texture) {
                continue;
            }
            const sprite = new PIXI.Sprite(texture);
            sprite.anchor.set(0.5);
            sprite.position.set(Math.round(placement.x), Math.round(placement.y));
            sprite.width = Math.round(placement.width);
            sprite.height = Math.round(placement.height);
            sprite.alpha = selected ? 0.96 : 0.78;
            sprite.label = `hotspot-cue-${hotspot.id}`;
            this.hotspotContainer.addChild(sprite);
            this.compositorTelemetry.hotspotCues.push({
                id: hotspot.id,
                kind: placement.kind,
                active,
                selected,
            });
        }
    }

    drawWeather(model) {
        const overlay = new PIXI.Graphics();
        const weather = String(model.weatherOverlay || '').toLowerCase();
        if (weather.includes('rain')) {
            overlay.rect(0, 0, model.stageWidth, model.stageHeight).fill({ color: 0x26334b, alpha: 0.16 });
            overlay.setStrokeStyle({ width: 2, color: 0xd9e0dd, alpha: 0.28 });
            for (let x = 0; x < model.stageWidth; x += 46) {
                overlay.moveTo(x, 0).lineTo(x - 34, model.stageHeight);
            }
            overlay.stroke();
        } else if (weather.includes('fog')) {
            overlay.rect(0, 0, model.stageWidth, model.stageHeight).fill({ color: 0xd8d4bf, alpha: 0.18 });
        } else {
            overlay.rect(0, 0, model.stageWidth, model.stageHeight).fill({ color: 0x4e3a2a, alpha: 0.06 });
        }
        this.overlayContainer.addChild(overlay);
    }

    drawEmpty(model) {
        const screen = this.app.screen;
        const bg = new PIXI.Graphics();
        bg.rect(0, 0, screen.width || 1280, screen.height || 720).fill(0x11130f);
        const label = new PIXI.Text({
            text: model.title,
            style: textStyle(28),
        });
        label.anchor.set(0.5);
        label.position.set((screen.width || 1280) / 2, (screen.height || 720) / 2);
        this.overlayContainer.addChild(bg, label);
    }

    flashTransition() {
        if (!this.app) {
            return;
        }
        this.startTransition(0.32, 280);
    }

    startTransition(maxAlpha = 0.5, durationMs = 520) {
        if (!this.app) {
            return;
        }
        this.transition.clear();
        this.transition.rect(0, 0, this.app.screen.width, this.app.screen.height).fill({
            color: 0x050604,
            alpha: maxAlpha,
        });
        let elapsed = 0;
        const ticker = (time) => {
            elapsed += time.deltaMS;
            const alpha = Math.max(0, maxAlpha * (1 - elapsed / durationMs));
            this.transition.alpha = alpha / maxAlpha;
            if (elapsed >= durationMs) {
                this.app.ticker.remove(ticker);
                this.transition.clear();
                this.transition.alpha = 1;
            }
        };
        this.app.ticker.add(ticker);
    }

    stagePointFromEvent(event) {
        const rect = this.app.canvas.getBoundingClientRect();
        return {
            x: (event.clientX - rect.left - this.fit.x) / this.fit.scale,
            y: (event.clientY - rect.top - this.fit.y) / this.fit.scale,
        };
    }

    handlePointerMove(event) {
        if (!this.model || this.model.kind !== 'scene') {
            this.onPointerTarget(null);
            return;
        }
        const point = this.stagePointFromEvent(event);
        const target = this.findTarget(point);
        this.app.canvas.style.cursor = target ? 'pointer' : 'default';
        this.onPointerTarget(target);
    }

    handlePointerLeave() {
        this.app.canvas.style.cursor = 'default';
        this.onPointerTarget(null);
    }

    handleClick(event) {
        if (!this.model || this.model.kind !== 'scene') {
            return;
        }
        const target = this.findTarget(this.stagePointFromEvent(event));
        if (target) {
            this.onActivate(target);
        }
    }

    findTarget(point) {
        for (const npc of [...this.model.npcs].reverse()) {
            if (
                point.x >= npc.bounds.x &&
                point.x <= npc.bounds.x + npc.bounds.width &&
                point.y >= npc.bounds.y &&
                point.y <= npc.bounds.y + npc.bounds.height
            ) {
                return { kind: 'npc', value: npc };
            }
        }
        for (const hotspot of [...this.model.hotspots].reverse()) {
            if (hotspot.polygon?.length && pointInPolygon(point, hotspot.polygon)) {
                return { kind: 'hotspot', value: hotspot };
            }
            if (
                hotspot.bounds &&
                point.x >= hotspot.bounds.x &&
                point.x <= hotspot.bounds.x + hotspot.bounds.width &&
                point.y >= hotspot.bounds.y &&
                point.y <= hotspot.bounds.y + hotspot.bounds.height
            ) {
                return { kind: 'hotspot', value: hotspot };
            }
        }
        return null;
    }
}
