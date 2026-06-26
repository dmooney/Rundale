import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { parsePng } from './audit-scene-atoms.mjs';

const scriptPath = fileURLToPath(import.meta.url);
const appDir = path.resolve(path.dirname(scriptPath), '..');
const repoRoot = path.resolve(appDir, '../../..');

export const defaultSpecPath = path.join(repoRoot, 'mods/rundale/scene-recipes/kilteevan-generated-plate-m9.json');
export const defaultOutputPath = path.join(
    repoRoot,
    '.proofs/visual-generated-kilteevan-plate-m9/generated-plate-manifest.json',
);

function stableJson(value) {
    if (Array.isArray(value)) {
        return `[${value.map(stableJson).join(',')}]`;
    }
    if (value && typeof value === 'object') {
        return `{${Object.keys(value)
            .sort()
            .map((key) => `${JSON.stringify(key)}:${stableJson(value[key])}`)
            .join(',')}}`;
    }
    return JSON.stringify(value);
}

function sha256(value) {
    return createHash('sha256').update(value).digest('hex');
}

function repoRelative(filePath) {
    const relative = path.relative(repoRoot, filePath);
    return relative.startsWith('..') ? filePath : relative;
}

function assertPercentPoint(point, label, errors) {
    if (!Array.isArray(point) || point.length !== 2) {
        errors.push(`${label} must be a [x, y] percent point`);
        return;
    }
    const [x, y] = point;
    if (!Number.isFinite(x) || !Number.isFinite(y) || x < 0 || x > 100 || y < 0 || y > 100) {
        errors.push(`${label} percent point ${JSON.stringify(point)} is out of bounds`);
    }
}

function connectedComponents(nodes, edges) {
    const graph = new Map(nodes.map((node) => [node.id, new Set()]));
    for (const [from, to] of edges) {
        if (!graph.has(from) || !graph.has(to)) {
            continue;
        }
        graph.get(from).add(to);
        graph.get(to).add(from);
    }
    const seen = new Set();
    let components = 0;
    for (const node of graph.keys()) {
        if (seen.has(node)) {
            continue;
        }
        components += 1;
        const queue = [node];
        seen.add(node);
        while (queue.length) {
            const current = queue.shift();
            for (const next of graph.get(current) || []) {
                if (!seen.has(next)) {
                    seen.add(next);
                    queue.push(next);
                }
            }
        }
    }
    return components;
}

function pointNearPolyline(point, polyline, tolerance = 7.5) {
    const [px, py] = point;
    for (let index = 1; index < polyline.length; index += 1) {
        const [ax, ay] = polyline[index - 1];
        const [bx, by] = polyline[index];
        const dx = bx - ax;
        const dy = by - ay;
        const lengthSquared = dx * dx + dy * dy || 1;
        const t = Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / lengthSquared));
        const cx = ax + t * dx;
        const cy = ay + t * dy;
        if (Math.hypot(px - cx, py - cy) <= tolerance) {
            return true;
        }
    }
    return false;
}

export async function readPlateSpec(specPath = defaultSpecPath) {
    return JSON.parse(await readFile(specPath, 'utf8'));
}

export function validatePlateSpec(spec) {
    const errors = [];
    const warnings = [];
    const layout = spec.layout || {};
    const roadNodes = layout.roads?.nodes || [];
    const roadEdges = layout.roads?.edges || [];
    const nodeIds = new Set(roadNodes.map((node) => node.id));

    if (spec.scene_slug !== 'kilteevan-village') {
        errors.push('scene_slug must be kilteevan-village');
    }
    if (!Array.isArray(spec.native_size) || spec.native_size[0] !== 1280 || spec.native_size[1] !== 720) {
        errors.push('native_size must be [1280, 720]');
    }
    if (spec.asset?.kind !== 'plate') {
        errors.push('asset.kind must be plate so Pixi treats it as a full-stage layer');
    }
    if (!spec.asset?.image?.startsWith('assets/scenes/kilteevan-village/generated/')) {
        errors.push('asset.image must live under the Kilteevan generated scene asset directory');
    }

    for (const node of roadNodes) {
        assertPercentPoint(node.percent, `road node ${node.id}`, errors);
    }
    for (const [from, to] of roadEdges) {
        if (!nodeIds.has(from)) {
            errors.push(`road edge references missing node ${from}`);
        }
        if (!nodeIds.has(to)) {
            errors.push(`road edge references missing node ${to}`);
        }
    }
    if (roadNodes.length > 0 && connectedComponents(roadNodes, roadEdges) !== 1) {
        errors.push('road graph must be one connected component');
    }

    const stream = layout.stream;
    if (!stream?.continuity_required) {
        errors.push('stream.continuity_required must be true');
    }
    if (!Array.isArray(stream?.polyline_percent) || stream.polyline_percent.length < 3) {
        errors.push('stream.polyline_percent must contain at least three points');
    } else {
        stream.polyline_percent.forEach((point, index) => assertPercentPoint(point, `stream point ${index}`, errors));
        const first = stream.polyline_percent[0];
        const last = stream.polyline_percent.at(-1);
        if (!(first[0] <= 1 || first[1] <= 1 || first[0] >= 99 || first[1] >= 99)) {
            errors.push('stream must enter from a scene edge');
        }
        if (!(last[0] <= 1 || last[1] <= 1 || last[0] >= 99 || last[1] >= 99)) {
            errors.push('stream must exit at a scene edge');
        }
    }

    const bridge = layout.bridge;
    assertPercentPoint(bridge?.center_percent, 'bridge center', errors);
    if (bridge?.spans_stream !== stream?.id) {
        errors.push('bridge.spans_stream must reference the stream id');
    }
    if (!bridge?.must_be_directly_over_water) {
        errors.push('bridge.must_be_directly_over_water must be true');
    }
    if (
        Array.isArray(stream?.polyline_percent) &&
        !pointNearPolyline(bridge?.center_percent || [], stream.polyline_percent)
    ) {
        errors.push('bridge center must sit near the stream polyline');
    }
    for (const nodeId of bridge?.connects_road_nodes || []) {
        if (!nodeIds.has(nodeId)) {
            errors.push(`bridge connects missing road node ${nodeId}`);
        }
    }

    for (const cottage of layout.cottage_pads || []) {
        if (!nodeIds.has(cottage.door_node)) {
            errors.push(`${cottage.id} door_node must be in the road graph`);
        }
        assertPercentPoint(cottage.chimney_socket_percent, `${cottage.id} chimney socket`, errors);
        if (!cottage.forbids?.includes('water')) {
            errors.push(`${cottage.id} must forbid water overlap`);
        }
    }

    for (const prop of layout.props || []) {
        if (!nodeIds.has(prop.node)) {
            errors.push(`${prop.id} node must be in the road graph`);
        }
        if (!prop.forbids?.includes('water')) {
            errors.push(`${prop.id} must forbid water overlap`);
        }
    }

    for (const slot of layout.npc_sockets || []) {
        if (!nodeIds.has(slot.node)) {
            errors.push(`${slot.id} node must be in the road graph`);
        }
        if (!slot.forbids?.includes('water')) {
            errors.push(`${slot.id} must forbid water overlap`);
        }
    }

    const cottageIds = new Set((layout.cottage_pads || []).map((cottage) => cottage.id));
    for (const smoke of layout.smoke || []) {
        const cottageId = String(smoke.origin || '').split('.', 1)[0];
        if (!cottageIds.has(cottageId) || !String(smoke.origin).endsWith('.chimney_socket_percent')) {
            errors.push(`${smoke.id} must originate from a cottage chimney socket`);
        }
    }

    const hotspotIds = new Set();
    for (const hotspot of layout.hotspots || []) {
        if (hotspotIds.has(hotspot.id)) {
            errors.push(`duplicate hotspot id ${hotspot.id}`);
        }
        hotspotIds.add(hotspot.id);
        if (!nodeIds.has(hotspot.node)) {
            errors.push(`${hotspot.id} node must be in the road graph`);
        }
    }
    for (const required of ['road-to-crossroads', 'village-well', 'village-signpost', 'wooden-bridge']) {
        if (!hotspotIds.has(required)) {
            errors.push(`missing required hotspot ${required}`);
        }
    }

    if (!spec.prompt_requirements?.some((term) => term.includes('continuous stream'))) {
        warnings.push('prompt_requirements should explicitly mention continuous stream');
    }
    if (!spec.negative_constraints?.includes('no props over water')) {
        errors.push('negative_constraints must include no props over water');
    }
    if (!spec.negative_constraints?.includes('no baked people or NPCs')) {
        errors.push('negative_constraints must include no baked people or NPCs');
    }

    return {
        ok: errors.length === 0,
        errors,
        warnings,
        road_node_count: roadNodes.length,
        road_edge_count: roadEdges.length,
        stream_point_count: stream?.polyline_percent?.length || 0,
        hotspot_count: layout.hotspots?.length || 0,
    };
}

export function buildPlatePrompt(spec) {
    const style = spec.art_direction?.style_terms || [];
    const palette = spec.art_direction?.palette || [];
    const requirements = spec.prompt_requirements || [];
    const negatives = spec.negative_constraints || [];
    return [
        'Use case: historical-scene',
        'Asset type: full-screen 1280x720 game background plate for a 2D adventure game.',
        `Primary request: Create ${spec.scene_slug} as a finished ${spec.art_direction?.finish || 'pixel-art game background'} set in ${spec.art_direction?.period || 'rural Ireland'}.`,
        `Camera: ${spec.camera?.projection || 'high 3/4 isometric'}; influences: ${(spec.camera?.influences || []).join(', ')}.`,
        `Style: ${style.join(', ')}.`,
        `Palette/lighting: ${palette.join(', ')}; ${spec.art_direction?.lighting || 'overcast daylight'}.`,
        `Physical layout requirements: ${requirements.join('; ')}.`,
        `Road graph nodes: ${(spec.layout?.roads?.nodes || []).map((node) => node.id).join(', ')}.`,
        `Negative constraints: ${negatives.join('; ')}.`,
    ].join('\n');
}

export async function buildPlateManifest(spec, { specPath = defaultSpecPath } = {}) {
    const validation = validatePlateSpec(spec);
    const prompt = buildPlatePrompt(spec);
    const assetPath = path.join(repoRoot, 'mods/rundale', spec.asset.image);
    const assetBuffer = await readFile(assetPath);
    const png = parsePng(assetBuffer);
    const imageValidation = {
        path: repoRelative(assetPath),
        width: png.width,
        height: png.height,
        sha256: sha256(assetBuffer),
        native_size_matches: png.width === spec.native_size[0] && png.height === spec.native_size[1],
    };
    if (!imageValidation.native_size_matches) {
        validation.errors.push(
            `generated plate ${imageValidation.path} is ${png.width}x${png.height}, expected ${spec.native_size.join('x')}`,
        );
        validation.ok = false;
    }

    return {
        id: spec.id,
        scene_slug: spec.scene_slug,
        location_id: spec.location_id,
        spec_path: repoRelative(specPath),
        spec_sha256: sha256(stableJson(spec)),
        generated_at_policy:
            'deterministic manifest; raster generated via built-in image_gen and saved as committed PNG',
        asset: spec.asset,
        validation,
        image_validation: imageValidation,
        prompt,
        negative_constraints: spec.negative_constraints,
    };
}

function parseArgs(argv) {
    const args = { specPath: defaultSpecPath, outputPath: defaultOutputPath };
    for (let index = 0; index < argv.length; index += 1) {
        const arg = argv[index];
        if (arg === '--spec') {
            args.specPath = path.resolve(repoRoot, argv[++index]);
        } else if (arg === '--out') {
            args.outputPath = path.resolve(repoRoot, argv[++index]);
        } else {
            throw new Error(`unknown argument ${arg}`);
        }
    }
    return args;
}

async function main() {
    const { specPath, outputPath } = parseArgs(process.argv.slice(2));
    const spec = await readPlateSpec(specPath);
    const manifest = await buildPlateManifest(spec, { specPath });
    await mkdir(path.dirname(outputPath), { recursive: true });
    await writeFile(outputPath, `${JSON.stringify(manifest, null, 2)}\n`);
    if (!manifest.validation.ok || !manifest.image_validation.native_size_matches) {
        console.error(JSON.stringify(manifest.validation, null, 2));
        process.exitCode = 1;
        return;
    }
    console.log(repoRelative(outputPath));
}

if (process.argv[1] === scriptPath) {
    main().catch((error) => {
        console.error(error);
        process.exitCode = 1;
    });
}
