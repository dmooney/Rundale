import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptPath = fileURLToPath(import.meta.url);
const appDir = path.resolve(path.dirname(scriptPath), '..');
const repoRoot = path.resolve(appDir, '../../..');

export const defaultSceneIndexPath = path.join(repoRoot, 'mods/rundale/scenes.json');
export const defaultRecipePath = path.join(
    repoRoot,
    'mods/rundale/scene-recipes/kilteevan-village-variants.json',
);

const stageLockedKinds = new Set(['ground', 'underlay', 'plate', 'sky', 'shadow', 'lighting']);
const requiredVariantCount = 10;
const percentPrecision = 1;
const scalePrecision = 3;
const opacityPrecision = 3;

function clone(value) {
    return JSON.parse(JSON.stringify(value));
}

async function readJson(filePath) {
    return JSON.parse(await readFile(filePath, 'utf8'));
}

function round(value, places) {
    const factor = 10 ** places;
    return Math.round(value * factor) / factor;
}

function clamp(value, min, max) {
    return Math.min(Math.max(value, min), max);
}

function slugify(value) {
    return String(value)
        .trim()
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, '-')
        .replace(/^-+|-+$/g, '');
}

function relativePath(filePath) {
    const relative = path.relative(repoRoot, filePath);
    return relative.startsWith('..') ? filePath : relative;
}

function resolveRepoPath(value, flag) {
    if (!value) {
        throw new Error(`${flag} requires a path`);
    }
    return path.isAbsolute(value) ? value : path.resolve(repoRoot, value);
}

function validateRecipe(recipe) {
    if (!recipe || typeof recipe !== 'object') {
        throw new Error('variant recipe must be a JSON object');
    }
    if (!recipe.source_slug) {
        throw new Error('variant recipe is missing source_slug');
    }
    if (!Array.isArray(recipe.variants)) {
        throw new Error('variant recipe is missing variants array');
    }
    if (recipe.variants.length !== requiredVariantCount) {
        throw new Error(
            `variant recipe must declare exactly ${requiredVariantCount} variants, got ${recipe.variants.length}`,
        );
    }

    const ids = new Set();
    for (const [index, variant] of recipe.variants.entries()) {
        if (!variant.id) {
            throw new Error(`variant ${index + 1} is missing id`);
        }
        if (ids.has(variant.id)) {
            throw new Error(`variant recipe has duplicate id '${variant.id}'`);
        }
        ids.add(variant.id);
        if (!variant.name || !variant.description) {
            throw new Error(`variant '${variant.id}' needs name and description`);
        }
    }
}

function findSourceScene(sceneIndex, sourceSlug) {
    const scene = sceneIndex.scenes?.find((candidate) => candidate.slug === sourceSlug);
    if (!scene) {
        throw new Error(`source scene '${sourceSlug}' not found`);
    }
    return scene;
}

function mergedTransform(layer, asset, variant) {
    const familyTransform = variant.family_transforms?.[asset?.kind || 'prop'] || {};
    const layerTransform = variant.layer_transforms?.[layer.id] || {};
    return {
        dx: (familyTransform.dx || 0) + (layerTransform.dx || 0),
        dy: (familyTransform.dy || 0) + (layerTransform.dy || 0),
        scale: (familyTransform.scale || 1) * (layerTransform.scale || 1),
        opacity: (familyTransform.opacity || 1) * (layerTransform.opacity || 1),
        flip: layerTransform.flip ?? familyTransform.flip,
    };
}

function applyLayerTransform(layer, asset, transform) {
    const next = clone(layer);
    const hasMovement = transform.dx !== 0 || transform.dy !== 0;
    const hasScale = transform.scale !== 1;
    const hasOpacity = transform.opacity !== 1;
    const stageLocked = stageLockedKinds.has(asset?.kind);

    if (hasMovement && !stageLocked && typeof next.x === 'number' && typeof next.y === 'number') {
        next.x = round(clamp(next.x + transform.dx, 2, 98), percentPrecision);
        next.y = round(clamp(next.y + transform.dy, 2, 98), percentPrecision);
    }

    if (hasScale && !stageLocked) {
        next.scale = round(clamp((next.scale ?? 1) * transform.scale, 0.08, 1.6), scalePrecision);
    }

    if (hasOpacity) {
        next.opacity = round(clamp((next.opacity ?? 1) * transform.opacity, 0.05, 1), opacityPrecision);
    }

    if (typeof transform.flip === 'boolean') {
        next.flip = transform.flip;
    }

    return next;
}

function applyHotspotOffsets(hotspots = [], offsets = {}) {
    return hotspots.map((hotspot) => {
        const next = clone(hotspot);
        const offset = offsets[next.id];
        if (!offset || !next.shape?.rect) {
            return next;
        }
        const rect = [...next.shape.rect];
        const width = rect[2] ?? 0;
        const height = rect[3] ?? 0;
        rect[0] = round(clamp((rect[0] ?? 0) + (offset.dx || 0), 0, 100 - width), percentPrecision);
        rect[1] = round(clamp((rect[1] ?? 0) + (offset.dy || 0), 0, 100 - height), percentPrecision);
        next.shape.rect = rect;
        return next;
    });
}

function applySlotOffsets(slots = [], offsets = {}) {
    return slots.map((slot) => {
        const next = clone(slot);
        const offset = offsets[next.id];
        if (!offset) {
            return next;
        }
        next.x = round(clamp((next.x ?? 50) + (offset.dx || 0), 2, 98), percentPrecision);
        next.y = round(clamp((next.y ?? 50) + (offset.dy || 0), 2, 98), percentPrecision);
        if (offset.scale) {
            next.scale = round(clamp((next.scale ?? 1) * offset.scale, 0.4, 1.6), scalePrecision);
        }
        return next;
    });
}

function makeVariantSlug(recipe, variant, index) {
    const prefix = recipe.output_slug_prefix || `${recipe.source_slug}-variant`;
    return `${prefix}-${String(index + 1).padStart(2, '0')}-${slugify(variant.id)}`;
}

function generateVariantScene({ sourceScene, assetsById, recipe, variant, index }) {
    const scene = clone(sourceScene);
    scene.slug = makeVariantSlug(recipe, variant, index);
    scene.location_id = (recipe.location_id_base || 15000) + index;
    scene.layers = sourceScene.layers.map((layer) => {
        const asset = assetsById.get(layer.asset);
        return applyLayerTransform(layer, asset, mergedTransform(layer, asset, variant));
    });
    scene.hotspots = applyHotspotOffsets(sourceScene.hotspots, variant.hotspot_offsets);
    scene.slots = applySlotOffsets(sourceScene.slots, variant.slot_offsets);
    return scene;
}

export function variantSignature(scene) {
    const stablePayload = {
        layers: scene.layers.map((layer) => [
            layer.id,
            layer.asset,
            layer.x,
            layer.y,
            layer.z,
            layer.scale ?? 1,
            layer.opacity ?? 1,
            Boolean(layer.flip),
        ]),
        hotspots: (scene.hotspots || []).map((hotspot) => [
            hotspot.id,
            hotspot.shape?.rect || null,
            hotspot.action || null,
        ]),
        slots: (scene.slots || []).map((slot) => [slot.id, slot.x, slot.y, slot.scale ?? 1, slot.prefer_npc ?? null]),
    };
    return createHash('sha256').update(JSON.stringify(stablePayload)).digest('hex').slice(0, 20);
}

function countChangedLayers(sourceScene, generatedScene) {
    let changed = 0;
    for (const sourceLayer of sourceScene.layers) {
        const layer = generatedScene.layers.find((candidate) => candidate.id === sourceLayer.id);
        if (!layer) {
            changed += 1;
            continue;
        }
        if (
            sourceLayer.x !== layer.x ||
            sourceLayer.y !== layer.y ||
            (sourceLayer.scale ?? 1) !== (layer.scale ?? 1) ||
            (sourceLayer.opacity ?? 1) !== (layer.opacity ?? 1) ||
            Boolean(sourceLayer.flip) !== Boolean(layer.flip)
        ) {
            changed += 1;
        }
    }
    return changed;
}

function layerStats(scene, assetsById) {
    let kitLayerCount = 0;
    let m2KitLayerCount = 0;
    const kinds = new Set();
    for (const layer of scene.layers) {
        const asset = assetsById.get(layer.asset);
        if (!asset?.image?.includes('/atoms/kit/')) {
            continue;
        }
        kitLayerCount += 1;
        kinds.add(asset.kind || 'prop');
        if (asset.image.includes('/atoms/kit/m2-')) {
            m2KitLayerCount += 1;
        }
    }
    return {
        layer_count: scene.layers.length,
        kit_layer_count: kitLayerCount,
        m2_kit_layer_count: m2KitLayerCount,
        kit_kinds: [...kinds].sort(),
    };
}

function assertGeneratedPack(pack) {
    const slugs = new Set();
    const locationIds = new Set();
    const signatures = new Set();
    for (const variant of pack.summary.variants) {
        if (slugs.has(variant.slug)) {
            throw new Error(`duplicate generated slug '${variant.slug}'`);
        }
        if (locationIds.has(variant.location_id)) {
            throw new Error(`duplicate generated location_id '${variant.location_id}'`);
        }
        if (signatures.has(variant.signature)) {
            throw new Error(`duplicate generated signature '${variant.signature}'`);
        }
        slugs.add(variant.slug);
        locationIds.add(variant.location_id);
        signatures.add(variant.signature);
    }
}

export function generateVillageVariantPack({
    sceneIndex,
    recipe,
    sceneIndexPath = defaultSceneIndexPath,
    recipePath = defaultRecipePath,
} = {}) {
    validateRecipe(recipe);
    const assetsById = new Map(sceneIndex.assets.map((asset) => [asset.id, asset]));
    const sourceScene = findSourceScene(sceneIndex, recipe.source_slug);
    const scenes = recipe.variants.map((variant, index) =>
        generateVariantScene({ sourceScene, assetsById, recipe, variant, index }),
    );
    const summaryVariants = scenes.map((scene, index) => ({
        index: index + 1,
        id: recipe.variants[index].id,
        name: recipe.variants[index].name,
        slug: scene.slug,
        location_id: scene.location_id,
        description: recipe.variants[index].description,
        signature: variantSignature(scene),
        changed_layer_count: countChangedLayers(sourceScene, scene),
        hotspot_count: scene.hotspots?.length || 0,
        slot_count: scene.slots?.length || 0,
        ...layerStats(scene, assetsById),
    }));
    const pack = {
        schema_version: 1,
        source: {
            scene_index: relativePath(sceneIndexPath),
            recipe: relativePath(recipePath),
            source_slug: sourceScene.slug,
            source_location_id: sourceScene.location_id,
        },
        assets: clone(sceneIndex.assets),
        sprites: clone(sceneIndex.sprites || []),
        fallback_sprites: clone(sceneIndex.fallback_sprites || {}),
        scenes,
        summary: {
            variant_count: scenes.length,
            art_direction: recipe.art_direction,
            variants: summaryVariants,
        },
    };
    assertGeneratedPack(pack);
    return pack;
}

export async function loadVillageVariantInputs({
    sceneIndexPath = defaultSceneIndexPath,
    recipePath = defaultRecipePath,
} = {}) {
    const [sceneIndex, recipe] = await Promise.all([readJson(sceneIndexPath), readJson(recipePath)]);
    return { sceneIndex, recipe, sceneIndexPath, recipePath };
}

function parseArgs(argv) {
    const args = {
        sceneIndexPath: defaultSceneIndexPath,
        recipePath: defaultRecipePath,
        outPath: null,
        summaryOutPath: null,
        summary: false,
    };
    for (let index = 0; index < argv.length; index += 1) {
        const arg = argv[index];
        const next = argv[index + 1];
        if (arg === '--scene-index') {
            args.sceneIndexPath = resolveRepoPath(next, arg);
            index += 1;
        } else if (arg === '--recipe') {
            args.recipePath = resolveRepoPath(next, arg);
            index += 1;
        } else if (arg === '--out') {
            args.outPath = resolveRepoPath(next, arg);
            index += 1;
        } else if (arg === '--summary-out') {
            args.summaryOutPath = resolveRepoPath(next, arg);
            index += 1;
        } else if (arg === '--summary') {
            args.summary = true;
        } else {
            throw new Error(`unknown argument '${arg}'`);
        }
    }
    return args;
}

async function writeJson(filePath, value) {
    await mkdir(path.dirname(filePath), { recursive: true });
    await writeFile(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

function printSummary(summary) {
    console.log(`Generated ${summary.variant_count} Kilteevan village variant(s).`);
    for (const variant of summary.variants) {
        console.log(
            `${String(variant.index).padStart(2, '0')}. ${variant.slug} ` +
                `layers=${variant.layer_count} kit=${variant.kit_layer_count} ` +
                `m2=${variant.m2_kit_layer_count} changed=${variant.changed_layer_count} ` +
                `signature=${variant.signature}`,
        );
    }
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    const inputs = await loadVillageVariantInputs(args);
    const pack = generateVillageVariantPack(inputs);
    if (args.outPath) {
        await writeJson(args.outPath, pack);
    }
    if (args.summaryOutPath) {
        await writeJson(args.summaryOutPath, pack.summary);
    }
    if (args.summary || (!args.outPath && !args.summaryOutPath)) {
        printSummary(pack.summary);
    }
}

if (process.argv[1] && path.resolve(process.argv[1]) === scriptPath) {
    try {
        await main();
    } catch (error) {
        console.error(error instanceof Error ? error.message : String(error));
        process.exit(1);
    }
}
