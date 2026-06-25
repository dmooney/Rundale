import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptPath = fileURLToPath(import.meta.url);
const appDir = path.resolve(path.dirname(scriptPath), '..');
const repoRoot = path.resolve(appDir, '../../..');

export const defaultSceneIndexPath = path.join(repoRoot, 'mods/rundale/scenes.json');
export const defaultRecipePath = path.join(repoRoot, 'mods/rundale/scene-recipes/outdoor-village-layouts.json');

const requiredAssetIds = new Set([
    'kilteevan-ground-base',
    'kilteevan-ground-patch',
    'kilteevan-contact-shadows',
    'kilteevan-muddy-lane',
    'kilteevan-stream',
    'kilteevan-bridge',
    'kilteevan-left-cottage',
    'kilteevan-right-cottage',
    'kilteevan-well',
    'kilteevan-wall',
    'kilteevan-signpost',
    'kilteevan-cart',
    'kilteevan-hedgerow',
    'kilteevan-smoke',
    'kilteevan-damp-vignette',
    'kilteevan-kit-flower-bramble-a',
    'kilteevan-kit-flower-bramble-b',
    'kilteevan-kit-m2-road-bend-a',
    'kilteevan-kit-m2-road-straight-a',
    'kilteevan-kit-m2-road-fork-a',
    'kilteevan-kit-m2-road-stub-a',
    'kilteevan-kit-m2-puddle-chip-a',
    'kilteevan-kit-m2-puddle-wide-a',
    'kilteevan-kit-m2-puddle-cluster-a',
    'kilteevan-kit-m2-wall-straight-a',
    'kilteevan-kit-m2-wall-corner-a',
    'kilteevan-kit-m2-wall-curve-a',
    'kilteevan-kit-m2-bramble-hedge-a',
    'kilteevan-kit-m2-flower-bush-a',
    'kilteevan-kit-m2-grass-tuft-a',
    'kilteevan-kit-m2-purple-flower-a',
    'kilteevan-kit-m2-roof-corner-a',
    'kilteevan-kit-m2-cottage-window-a',
    'kilteevan-kit-m2-cottage-door-a',
    'kilteevan-kit-m2-roof-cottage-a',
    'kilteevan-kit-m2-chimney-a',
    'kilteevan-kit-m2-signpost-a',
    'kilteevan-kit-m2-well-rim-a',
    'kilteevan-kit-m2-cart-wheel-a',
    'kilteevan-kit-m2-wood-planks-a',
    'kilteevan-kit-m2-smoke-wisp-a',
    'kilteevan-kit-m2-mud-chip-a',
    'kilteevan-kit-mud-edge-a',
    'kilteevan-kit-mud-edge-b',
    'kilteevan-kit-wall-stones-a',
]);

const waterAtoms = [
    'kilteevan-kit-m2-puddle-chip-a',
    'kilteevan-kit-m2-puddle-wide-a',
    'kilteevan-kit-m2-puddle-cluster-a',
];
const foliageAtoms = [
    'kilteevan-kit-m2-bramble-hedge-a',
    'kilteevan-kit-m2-flower-bush-a',
    'kilteevan-kit-m2-grass-tuft-a',
    'kilteevan-kit-m2-purple-flower-a',
    'kilteevan-kit-flower-bramble-a',
    'kilteevan-kit-flower-bramble-b',
];
const terrainAtoms = [
    'kilteevan-kit-m2-grass-tuft-a',
    'kilteevan-kit-m2-flower-bush-a',
    'kilteevan-kit-m2-bramble-hedge-a',
    'kilteevan-kit-m2-purple-flower-a',
];
const mudAtoms = ['kilteevan-kit-m2-mud-chip-a'];

const defaultTerrainProfile = {
    name: 'Generated terrain',
    grade: 'level',
    ground: 'wet green',
    path: 'mud lane',
    water: 'none',
    vegetation: 'rough grass',
    lighting: 'overcast',
    base_opacity: 0.14,
    ground_patch_count: 32,
    vegetation_patch_count: 18,
    mud_patch_count: 10,
    bank_patch_count: 6,
    path_width_scale: 1,
    water_bank_width: 1,
    puddle_density: 0.25,
};

const defaultIsoGrid = {
    cols: 24,
    rows: 18,
    percent_origin: [3, 5],
    percent_size: [94, 90],
    rendered_water_margin_cells: 1,
};

const builtinPrefabCatalog = {
    'bridge-crossing': {
        requires: ['path_crosses_water', 'continuous_water_under_span'],
        ports: ['path_a', 'path_b', 'water_a', 'water_b'],
    },
    cottage: {
        requires: ['door_on_path', 'chimney_socket'],
        forbids: ['water', 'rendered_water'],
    },
    'cart-pullout': {
        requires: ['adjacent_path'],
        forbids: ['water', 'rendered_water', 'building'],
    },
    'well-node': {
        requires: ['adjacent_path'],
        forbids: ['water'],
    },
    'signpost-node': {
        requires: ['adjacent_path'],
        forbids: ['water'],
    },
    'market-node': {
        requires: ['adjacent_path'],
        forbids: ['water'],
    },
    'npc-standing-slot': {
        requires: ['adjacent_path'],
        forbids: ['water'],
    },
};

function clone(value) {
    return JSON.parse(JSON.stringify(value));
}

async function readJson(filePath) {
    return JSON.parse(await readFile(filePath, 'utf8'));
}

function round(value, places = 1) {
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

function hashHex(value) {
    return createHash('sha256').update(JSON.stringify(value)).digest('hex');
}

function hashByte(seed, index = 0) {
    return createHash('sha256').update(`${seed}:${index}`).digest()[0];
}

function unitFromSeed(seed, index) {
    return hashByte(seed, index) / 255;
}

function point(value, context) {
    if (!Array.isArray(value) || value.length !== 2) {
        throw new Error(`${context} must be a [x,y] point`);
    }
    const [x, y] = value;
    if (!Number.isFinite(x) || !Number.isFinite(y)) {
        throw new Error(`${context} must contain finite numbers`);
    }
    return { x, y };
}

function nodePoint(layout, nodeId) {
    if (!layout.nodes || !Object.hasOwn(layout.nodes, nodeId)) {
        throw new Error(`layout '${layout.id}' references missing node '${nodeId}'`);
    }
    return point(layout.nodes[nodeId], `node '${nodeId}'`);
}

function pointInBounds(candidate) {
    return candidate.x >= 0 && candidate.x <= 100 && candidate.y >= 0 && candidate.y <= 100;
}

function distance(a, b) {
    return Math.hypot(a.x - b.x, a.y - b.y);
}

function segmentLength(segment) {
    return distance(segment.a, segment.b);
}

function lerp(a, b, t) {
    return { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t };
}

function midpoint(a, b) {
    return lerp(a, b, 0.5);
}

function distancePointToSegment(p, a, b) {
    const lengthSquared = (b.x - a.x) ** 2 + (b.y - a.y) ** 2;
    if (lengthSquared === 0) {
        return distance(p, a);
    }
    const t = clamp(((p.x - a.x) * (b.x - a.x) + (p.y - a.y) * (b.y - a.y)) / lengthSquared, 0, 1);
    return distance(p, lerp(a, b, t));
}

function orientation(a, b, c) {
    const value = (b.y - a.y) * (c.x - b.x) - (b.x - a.x) * (c.y - b.y);
    if (Math.abs(value) < 0.0001) {
        return 0;
    }
    return value > 0 ? 1 : 2;
}

function onSegment(a, b, c) {
    return (
        b.x <= Math.max(a.x, c.x) + 0.0001 &&
        b.x + 0.0001 >= Math.min(a.x, c.x) &&
        b.y <= Math.max(a.y, c.y) + 0.0001 &&
        b.y + 0.0001 >= Math.min(a.y, c.y)
    );
}

function segmentsIntersect(a, b, c, d) {
    const o1 = orientation(a, b, c);
    const o2 = orientation(a, b, d);
    const o3 = orientation(c, d, a);
    const o4 = orientation(c, d, b);
    if (o1 !== o2 && o3 !== o4) {
        return true;
    }
    return (
        (o1 === 0 && onSegment(a, c, b)) ||
        (o2 === 0 && onSegment(a, d, b)) ||
        (o3 === 0 && onSegment(c, a, d)) ||
        (o4 === 0 && onSegment(c, b, d))
    );
}

function segmentDistance(first, second) {
    if (segmentsIntersect(first.a, first.b, second.a, second.b)) {
        return 0;
    }
    return Math.min(
        distancePointToSegment(first.a, second.a, second.b),
        distancePointToSegment(first.b, second.a, second.b),
        distancePointToSegment(second.a, first.a, first.b),
        distancePointToSegment(second.b, first.a, first.b),
    );
}

function pathSegment(layout, pathId) {
    const pathDef = layout.paths?.find((candidate) => candidate.id === pathId);
    if (!pathDef) {
        throw new Error(`layout '${layout.id}' references missing path '${pathId}'`);
    }
    return { id: pathDef.id, a: nodePoint(layout, pathDef.from), b: nodePoint(layout, pathDef.to) };
}

function waterwaySegments(waterway) {
    return (waterway.points || []).slice(0, -1).map((raw, index) => ({
        id: `${waterway.id}:${index}`,
        a: point(raw, `waterway '${waterway.id}' point ${index}`),
        b: point(waterway.points[index + 1], `waterway '${waterway.id}' point ${index + 1}`),
        waterway,
    }));
}

function distanceToWaterway(candidate, waterway) {
    return Math.min(...waterwaySegments(waterway).map((segment) => distancePointToSegment(candidate, segment.a, segment.b)));
}

function nearestPathDistance(layout, candidate) {
    return Math.min(...(layout.paths || []).map((pathDef) => {
        const segment = pathSegment(layout, pathDef.id);
        return distancePointToSegment(candidate, segment.a, segment.b);
    }));
}

function pointInWater(layout, candidate) {
    return (layout.waterways || []).some((waterway) => {
        const threshold = Math.max(1.8, (waterway.width || 6) * 0.35);
        return distanceToWaterway(candidate, waterway) <= threshold;
    });
}

function pointInRenderedWater(layout, candidate) {
    return (layout.waterways || []).some((waterway) => {
        const threshold = Math.max(5.5, (waterway.width || 6) * 0.95);
        return distanceToWaterway(candidate, waterway) <= threshold;
    });
}

function pointInRect(candidate, rect) {
    const [x, y, width, height] = rect;
    return candidate.x >= x && candidate.x <= x + width && candidate.y >= y && candidate.y <= y + height;
}

function pointInPolygon(candidate, polygon) {
    let inside = false;
    for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i, i += 1) {
        const a = point(polygon[i], `polygon point ${i}`);
        const b = point(polygon[j], `polygon point ${j}`);
        const intersects =
            a.y > candidate.y !== b.y > candidate.y &&
            candidate.x < ((b.x - a.x) * (candidate.y - a.y)) / (b.y - a.y || 0.000001) + a.x;
        if (intersects) {
            inside = !inside;
        }
    }
    return inside;
}

function validateVisualWaterExclusions(exclusions = []) {
    const errors = [];
    uniqueOrError(exclusions.map((exclusion) => exclusion.id), 'visual water exclusions', errors);
    for (const exclusion of exclusions) {
        if (Array.isArray(exclusion.rect)) {
            if (exclusion.rect.length !== 4) {
                errors.push(`visual water exclusion '${exclusion.id}' rect must have four values`);
                continue;
            }
            const [x, y, width, height] = exclusion.rect;
            if (![x, y, width, height].every(Number.isFinite) || width <= 0 || height <= 0) {
                errors.push(`visual water exclusion '${exclusion.id}' rect must be finite with positive size`);
            }
            if (x < 0 || y < 0 || x + width > 100 || y + height > 100) {
                errors.push(`visual water exclusion '${exclusion.id}' rect is out of bounds`);
            }
            continue;
        }
        if (Array.isArray(exclusion.polygon)) {
            if (exclusion.polygon.length < 3) {
                errors.push(`visual water exclusion '${exclusion.id}' polygon needs at least three points`);
                continue;
            }
            for (const [index, raw] of exclusion.polygon.entries()) {
                const candidate = point(raw, `visual water exclusion '${exclusion.id}' polygon point ${index}`);
                if (!pointInBounds(candidate)) {
                    errors.push(`visual water exclusion '${exclusion.id}' polygon point ${index} is out of bounds`);
                }
            }
            continue;
        }
        errors.push(`visual water exclusion '${exclusion.id}' needs rect or polygon`);
    }
    if (errors.length) {
        throw new Error(`visual water exclusions invalid: ${errors.join('; ')}`);
    }
}

function pointInVisualWater(layout, candidate, visualWaterExclusions = []) {
    return (
        pointInRenderedWater(layout, candidate) ||
        visualWaterExclusions.some((exclusion) => {
            if (Array.isArray(exclusion.rect)) {
                return pointInRect(candidate, exclusion.rect);
            }
            if (Array.isArray(exclusion.polygon)) {
                return pointInPolygon(candidate, exclusion.polygon);
            }
            return false;
        })
    );
}

function gridSpec(raw = {}) {
    const source = raw?.grid ? raw.grid : raw;
    const merged = { ...defaultIsoGrid, ...(source || {}) };
    return {
        cols: Math.trunc(merged.cols),
        rows: Math.trunc(merged.rows),
        percent_origin: [...merged.percent_origin],
        percent_size: [...merged.percent_size],
        rendered_water_margin_cells: Math.max(0, Math.trunc(merged.rendered_water_margin_cells ?? 1)),
    };
}

function validateGridSpec(rawGrid) {
    const grid = gridSpec(rawGrid);
    const errors = [];
    if (!Number.isInteger(grid.cols) || grid.cols < 8) {
        errors.push('grid cols must be an integer >= 8');
    }
    if (!Number.isInteger(grid.rows) || grid.rows < 8) {
        errors.push('grid rows must be an integer >= 8');
    }
    const [originX, originY] = grid.percent_origin;
    const [width, height] = grid.percent_size;
    if (![originX, originY, width, height].every(Number.isFinite)) {
        errors.push('grid percent_origin and percent_size must contain finite numbers');
    }
    if (width <= 0 || height <= 0 || originX < 0 || originY < 0 || originX + width > 100 || originY + height > 100) {
        errors.push('grid percent bounds must fit inside 0..100');
    }
    if (errors.length) {
        throw new Error(`grid invalid: ${errors.join('; ')}`);
    }
    return grid;
}

function cellKey(cell) {
    return `${cell.col},${cell.row}`;
}

function cellFromKey(key) {
    const [col, row] = key.split(',').map((value) => Number.parseInt(value, 10));
    return { col, row };
}

function pointToCell(grid, candidate) {
    const [originX, originY] = grid.percent_origin;
    const [width, height] = grid.percent_size;
    return {
        col: Math.trunc(clamp(Math.round(((candidate.x - originX) / width) * (grid.cols - 1)), 0, grid.cols - 1)),
        row: Math.trunc(clamp(Math.round(((candidate.y - originY) / height) * (grid.rows - 1)), 0, grid.rows - 1)),
    };
}

function cellCenter(grid, cell) {
    const [originX, originY] = grid.percent_origin;
    const [width, height] = grid.percent_size;
    return {
        x: originX + (cell.col / Math.max(1, grid.cols - 1)) * width,
        y: originY + (cell.row / Math.max(1, grid.rows - 1)) * height,
    };
}

function addCell(set, cell) {
    set.add(cellKey(cell));
}

function lineCells(grid, a, b) {
    const start = pointToCell(grid, a);
    const end = pointToCell(grid, b);
    const steps = Math.max(Math.abs(end.col - start.col), Math.abs(end.row - start.row), 1);
    const cells = new Set();
    for (let step = 0; step <= steps; step += 1) {
        const t = step / steps;
        addCell(cells, {
            col: Math.trunc(clamp(Math.round(start.col + (end.col - start.col) * t), 0, grid.cols - 1)),
            row: Math.trunc(clamp(Math.round(start.row + (end.row - start.row) * t), 0, grid.rows - 1)),
        });
    }
    return cells;
}

function expandedCells(cells, grid, radius) {
    const expanded = new Set(cells);
    for (const key of cells) {
        const cell = cellFromKey(key);
        for (let dx = -radius; dx <= radius; dx += 1) {
            for (let dy = -radius; dy <= radius; dy += 1) {
                if (Math.abs(dx) + Math.abs(dy) > radius) {
                    continue;
                }
                const next = { col: cell.col + dx, row: cell.row + dy };
                if (next.col >= 0 && next.col < grid.cols && next.row >= 0 && next.row < grid.rows) {
                    addCell(expanded, next);
                }
            }
        }
    }
    return expanded;
}

function cellsFromRect(grid, rect) {
    const [x, y, width, height] = rect;
    const cells = new Set();
    for (let col = 0; col < grid.cols; col += 1) {
        for (let row = 0; row < grid.rows; row += 1) {
            const center = cellCenter(grid, { col, row });
            if (center.x >= x && center.x <= x + width && center.y >= y && center.y <= y + height) {
                addCell(cells, { col, row });
            }
        }
    }
    return cells;
}

function cellsFromPolygon(grid, polygon) {
    const cells = new Set();
    for (let col = 0; col < grid.cols; col += 1) {
        for (let row = 0; row < grid.rows; row += 1) {
            const center = cellCenter(grid, { col, row });
            if (pointInPolygon(center, polygon)) {
                addCell(cells, { col, row });
            }
        }
    }
    return cells;
}

function unionCells(...sets) {
    const union = new Set();
    for (const set of sets) {
        for (const key of set || []) {
            union.add(key);
        }
    }
    return union;
}

function subtractCells(cells, subtract) {
    const result = new Set();
    for (const key of cells) {
        if (!subtract.has(key)) {
            result.add(key);
        }
    }
    return result;
}

function neighborKeys(key, grid) {
    const { col, row } = cellFromKey(key);
    const neighbors = [];
    for (let dx = -1; dx <= 1; dx += 1) {
        for (let dy = -1; dy <= 1; dy += 1) {
            if (dx === 0 && dy === 0) {
                continue;
            }
            neighbors.push({ col: col + dx, row: row + dy });
        }
    }
    return neighbors
        .filter((cell) => cell.col >= 0 && cell.col < grid.cols && cell.row >= 0 && cell.row < grid.rows)
        .map(cellKey);
}

function connectedComponents(cells, grid) {
    const pending = new Set(cells);
    const components = [];
    while (pending.size) {
        const first = pending.values().next().value;
        const component = new Set();
        const queue = [first];
        pending.delete(first);
        while (queue.length) {
            const current = queue.shift();
            component.add(current);
            for (const next of neighborKeys(current, grid)) {
                if (pending.has(next)) {
                    pending.delete(next);
                    queue.push(next);
                }
            }
        }
        components.push(component);
    }
    return components;
}

function waterwayProgressAtPoint(candidate, waterway) {
    const segments = waterwaySegments(waterway);
    const totalLength = segments.reduce((sum, segment) => sum + segmentLength(segment), 0);
    let walked = 0;
    let best = { distance: Number.POSITIVE_INFINITY, progress: 0 };
    for (const segment of segments) {
        const length = segmentLength(segment);
        const lengthSquared = (segment.b.x - segment.a.x) ** 2 + (segment.b.y - segment.a.y) ** 2;
        const t =
            lengthSquared === 0
                ? 0
                : clamp(
                      ((candidate.x - segment.a.x) * (segment.b.x - segment.a.x) +
                          (candidate.y - segment.a.y) * (segment.b.y - segment.a.y)) /
                          lengthSquared,
                      0,
                      1,
                  );
        const projected = lerp(segment.a, segment.b, t);
        const candidateDistance = distance(candidate, projected);
        if (candidateDistance < best.distance) {
            best = {
                distance: candidateDistance,
                progress: totalLength > 0 ? (walked + t * length) / totalLength : 0,
            };
        }
        walked += length;
    }
    return best;
}

function prefabCatalogForRecipe(recipe = {}) {
    return { ...builtinPrefabCatalog, ...(recipe.prefab_catalog || {}) };
}

function terrainProfilesForRecipe(recipe = {}) {
    return recipe.terrain_profiles || {};
}

function sortedPlainObject(value) {
    return Object.fromEntries(Object.entries(value || {}).sort(([a], [b]) => a.localeCompare(b)));
}

function terrainProfileForLayout(recipe, layout) {
    const profiles = terrainProfilesForRecipe(recipe);
    if (!layout.terrain_profile) {
        throw new Error(`layout '${layout.id}' is missing terrain_profile`);
    }
    const profile = profiles[layout.terrain_profile];
    if (!profile) {
        throw new Error(`layout '${layout.id}' references missing terrain profile '${layout.terrain_profile}'`);
    }
    return { id: layout.terrain_profile, ...defaultTerrainProfile, ...profile };
}

function validateTerrainProfiles(profiles) {
    const errors = [];
    if (!profiles || typeof profiles !== 'object' || Array.isArray(profiles)) {
        throw new Error('terrain_profiles must be an object');
    }
    if (Object.keys(profiles).length === 0) {
        throw new Error('terrain_profiles must declare at least one profile');
    }
    const numericRanges = {
        base_opacity: [0.05, 0.28],
        ground_patch_count: [8, 80],
        vegetation_patch_count: [0, 80],
        mud_patch_count: [0, 80],
        bank_patch_count: [0, 80],
        path_width_scale: [0.5, 1.8],
        water_bank_width: [0.5, 1.8],
        puddle_density: [0, 1],
    };
    for (const [id, rawProfile] of Object.entries(profiles)) {
        if (!id) {
            errors.push('terrain profile contains an empty id');
            continue;
        }
        if (!rawProfile || typeof rawProfile !== 'object' || Array.isArray(rawProfile)) {
            errors.push(`terrain profile '${id}' must be an object`);
            continue;
        }
        for (const field of ['name', 'grade', 'ground', 'path', 'vegetation', 'lighting']) {
            if (!rawProfile[field]) {
                errors.push(`terrain profile '${id}' is missing ${field}`);
            }
        }
        for (const [field, [min, max]] of Object.entries(numericRanges)) {
            const value = rawProfile[field] ?? defaultTerrainProfile[field];
            if (!Number.isFinite(value) || value < min || value > max) {
                errors.push(`terrain profile '${id}' ${field} must be ${min}..${max}`);
            }
        }
    }
    if (errors.length) {
        throw new Error(`terrain profiles invalid: ${errors.join('; ')}`);
    }
}

function terrainProfileConfigSignature(profile) {
    return hashHex(sortedPlainObject(profile)).slice(0, 20);
}

export function terrainSignature(layout, profile) {
    const payload = {
        profile: terrainProfileConfigSignature(profile),
        grade: profile.grade,
        ground: profile.ground,
        path: profile.path,
        water: profile.water,
        vegetation: profile.vegetation,
        paths: (layout.paths || []).map((pathDef) => [
            pathDef.id,
            layout.nodes?.[pathDef.from]?.map((value) => round(value, 1)),
            layout.nodes?.[pathDef.to]?.map((value) => round(value, 1)),
        ]),
        waterway_silhouette: (layout.waterways || []).map((waterway) => [
            waterway.kind || 'water',
            waterway.width || 6,
            waterway.points.map((raw) => raw.map((value) => round(value, 1))),
        ]),
        bridges: (layout.bridges || []).map((bridge) => [bridge.path, bridge.waterway, bridge.node]),
    };
    return hashHex(payload).slice(0, 20);
}

function validatePrefabCatalog(catalog) {
    const errors = [];
    for (const [id, prefab] of Object.entries(catalog || {})) {
        if (!prefab || typeof prefab !== 'object') {
            errors.push(`prefab '${id}' must be an object`);
            continue;
        }
        if (!Array.isArray(prefab.ports)) {
            errors.push(`prefab '${id}' must declare ports`);
        }
        if (!Array.isArray(prefab.requires)) {
            errors.push(`prefab '${id}' must declare requires`);
        }
    }
    if (errors.length) {
        throw new Error(`prefab catalog invalid: ${errors.join('; ')}`);
    }
}

function prefabForProp(prop) {
    if (prop.prefab) {
        return prop.prefab;
    }
    if (prop.kind === 'cart') {
        return 'cart-pullout';
    }
    if (prop.kind === 'well') {
        return 'well-node';
    }
    if (prop.kind === 'signpost') {
        return 'signpost-node';
    }
    if (prop.kind === 'market') {
        return 'market-node';
    }
    return null;
}

function placementRecords(layout) {
    return [
        ...(layout.bridges || []).map((bridge) => ({
            id: bridge.id,
            kind: 'bridge',
            prefab: bridge.prefab || 'bridge-crossing',
            node: bridge.node,
        })),
        ...(layout.cottage_sites || []).map((site) => ({
            id: site.id,
            kind: 'cottage',
            prefab: site.prefab || 'cottage',
            node: site.door,
        })),
        ...(layout.props || []).map((prop) => ({
            id: prop.id,
            kind: prop.kind,
            prefab: prefabForProp(prop),
            node: prop.node,
        })),
        ...(layout.npc_slots || []).map((slot) => ({
            id: slot.id,
            kind: 'npc',
            prefab: slot.prefab || 'npc-standing-slot',
            node: slot.node,
        })),
    ];
}

function propFootprintCells(layout, prop, grid) {
    const anchor = nodePoint(layout, prop.node);
    return new Set(
        assetFootprintPoints(prop.kind, anchor, prop.scale || 0.74, Boolean(prop.flip)).map((sample) =>
            cellKey(pointToCell(grid, sample)),
        ),
    );
}

function visualWaterExclusionCells(grid, visualWaterExclusions = []) {
    const cells = new Set();
    for (const exclusion of visualWaterExclusions) {
        const exclusionCells = Array.isArray(exclusion.rect)
            ? cellsFromRect(grid, exclusion.rect)
            : Array.isArray(exclusion.polygon)
              ? cellsFromPolygon(grid, exclusion.polygon)
              : new Set();
        for (const key of exclusionCells) {
            cells.add(key);
        }
    }
    return cells;
}

function buildGridTerrainModel(layout, { grid = defaultIsoGrid, visualWaterExclusions = [], prefabCatalog = builtinPrefabCatalog } = {}) {
    const pathCellsById = new Map();
    for (const pathDef of layout.paths || []) {
        pathCellsById.set(pathDef.id, lineCells(grid, nodePoint(layout, pathDef.from), nodePoint(layout, pathDef.to)));
    }
    const roadCells = unionCells(...pathCellsById.values());

    const waterCellsById = new Map();
    const renderedWaterCellsById = new Map();
    for (const waterway of layout.waterways || []) {
        const segmentCells = waterwaySegments(waterway).map((segment) => lineCells(grid, segment.a, segment.b));
        const coreCells = unionCells(...segmentCells);
        const widthRadius = Math.max(0, Math.round((waterway.width || 6) / 8));
        waterCellsById.set(waterway.id, coreCells);
        renderedWaterCellsById.set(
            waterway.id,
            expandedCells(coreCells, grid, Math.max(widthRadius, grid.rendered_water_margin_cells || 0)),
        );
    }
    const waterCells = unionCells(...waterCellsById.values());
    const renderedWaterCells = unionCells(...renderedWaterCellsById.values(), visualWaterExclusionCells(grid, visualWaterExclusions));

    const bridgeCells = new Set();
    for (const bridge of layout.bridges || []) {
        const center = pointToCell(grid, nodePoint(layout, bridge.node));
        for (const key of expandedCells(new Set([cellKey(center)]), grid, 1)) {
            bridgeCells.add(key);
        }
        const pathCells = pathCellsById.get(bridge.path);
        const renderedCells = renderedWaterCellsById.get(bridge.waterway);
        if (pathCells && renderedCells) {
            const crossingCells = new Set([...pathCells].filter((key) => renderedCells.has(key)));
            for (const key of expandedCells(crossingCells, grid, 1)) {
                bridgeCells.add(key);
            }
        }
    }

    const walkableCells = unionCells(roadCells, bridgeCells);
    const roadComponents = connectedComponents(walkableCells, grid);
    const waterComponents = [...waterCellsById.values()].flatMap((cells) => connectedComponents(cells, grid));
    const invalidFreeformPlacements = placementRecords(layout).filter(
        (record) => !record.prefab || !prefabCatalog[record.prefab] || !layout.nodes?.[record.node],
    );

    const errors = [];
    if (roadCells.size && roadComponents.length !== 1) {
        errors.push(`grid road cells split into ${roadComponents.length} components`);
    }
    for (const [waterwayId, cells] of waterCellsById.entries()) {
        const componentCount = connectedComponents(cells, grid).length;
        if (componentCount !== 1) {
            errors.push(`waterway '${waterwayId}' grid cells split into ${componentCount} components`);
        }
    }
    for (const record of invalidFreeformPlacements) {
        errors.push(`placement '${record.id}' is not resolved through a known prefab and grid node`);
    }

    for (const bridge of layout.bridges || []) {
        const waterway = (layout.waterways || []).find((candidate) => candidate.id === bridge.waterway);
        const pathCells = pathCellsById.get(bridge.path);
        const renderedCells = renderedWaterCellsById.get(bridge.waterway);
        if (!waterway || !pathCells || !renderedCells) {
            continue;
        }
        const center = nodePoint(layout, bridge.node);
        const progress = waterwayProgressAtPoint(center, waterway);
        if (progress.progress <= 0.08 || progress.progress >= 0.92) {
            errors.push(`bridge '${bridge.id}' does not have continuous water on both sides`);
        }
        const overlap = [...pathCells].filter((key) => renderedCells.has(key));
        if (!overlap.length) {
            errors.push(`bridge '${bridge.id}' path has no rendered-water crossing cells`);
        }
        const uncovered = overlap.filter((key) => !bridgeCells.has(key));
        if (uncovered.length > 2) {
            errors.push(`bridge '${bridge.id}' does not cover its path/water crossing cells`);
        }
    }

    const bridgePathIds = new Set((layout.bridges || []).map((bridge) => bridge.path));
    for (const [pathId, pathCells] of pathCellsById.entries()) {
        const waterOverlap = [...pathCells].some((key) => waterCells.has(key) && !bridgeCells.has(key));
        if (waterOverlap && !bridgePathIds.has(pathId)) {
            errors.push(`path '${pathId}' crosses grid water without a bridge prefab`);
        }
    }

    const renderedWaterCollisionFailures = [];
    const propFootprints = [];
    for (const prop of layout.props || []) {
        if (!layout.nodes?.[prop.node]) {
            continue;
        }
        const footprintCells = propFootprintCells(layout, prop, grid);
        propFootprints.push({ prop, cells: footprintCells });
        const blockingWater = prop.kind === 'cart' ? renderedWaterCells : waterCells;
        const wetCell = [...footprintCells].find((key) => blockingWater.has(key));
        if (wetCell) {
            renderedWaterCollisionFailures.push({ id: prop.id, cell: wetCell });
            errors.push(`prop '${prop.id}' grid footprint intersects rendered water at ${wetCell}`);
        }
    }

    for (const slot of layout.npc_slots || []) {
        if (!layout.nodes?.[slot.node]) {
            continue;
        }
        const slotCell = cellKey(pointToCell(grid, nodePoint(layout, slot.node)));
        const blockingProp = propFootprints.find(({ prop, cells }) => prop.kind === 'cart' && cells.has(slotCell));
        if (blockingProp) {
            errors.push(`npc slot '${slot.id}' intersects prop '${blockingProp.prop.id}' footprint at ${slotCell}`);
        }
    }

    const prefabPortConnections =
        (layout.bridges || []).length * 4 +
        (layout.cottage_sites || []).length +
        (layout.props || []).length +
        (layout.npc_slots || []).length;

    return {
        errors,
        summary: {
            grid_cols: grid.cols,
            grid_rows: grid.rows,
            grid_cell_count: grid.cols * grid.rows,
            road_cell_count: roadCells.size,
            water_cell_count: waterCells.size,
            rendered_water_cell_count: renderedWaterCells.size,
            bridge_cell_count: bridgeCells.size,
            road_components: roadComponents.length,
            water_components: waterComponents.length,
            prefab_port_connections: prefabPortConnections,
            invalid_freeform_placements: invalidFreeformPlacements.length,
            rendered_water_collision_failures: renderedWaterCollisionFailures.length,
        },
    };
}

function assetFootprintPoints(kind, anchor, scale = 1, flip = false) {
    if (kind !== 'cart') {
        return [anchor];
    }
    const baseScale = scale || 0.74;
    const xs = [-11.5, -8, -4, 0, 4, 8, 11.5].map((offset) => offset * baseScale);
    const ys = [-34, -27, -20, -13, -6, -1].map((offset) => offset * baseScale);
    const samples = [];
    for (const x of xs) {
        for (const y of ys) {
            samples.push({ x: anchor.x + x, y: anchor.y + y });
        }
    }
    const handleDirection = flip ? 1 : -1;
    for (const reach of [8, 14, 20]) {
        samples.push({ x: anchor.x + handleDirection * reach * baseScale, y: anchor.y - 2 * baseScale });
    }
    return samples;
}

function rectAround(candidate, width, height) {
    return [
        round(clamp(candidate.x - width / 2, 0, 100 - width), 1),
        round(clamp(candidate.y - height / 2, 0, 100 - height), 1),
        round(width, 1),
        round(height, 1),
    ];
}

function uniqueOrError(values, label, errors) {
    const seen = new Set();
    for (const value of values) {
        if (!value) {
            errors.push(`${label} contains a missing id`);
        } else if (seen.has(value)) {
            errors.push(`${label} contains duplicate id '${value}'`);
        }
        seen.add(value);
    }
}

function reachablePathNodes(layout) {
    const graph = new Map();
    for (const nodeId of Object.keys(layout.nodes || {})) {
        graph.set(nodeId, new Set());
    }
    for (const pathDef of layout.paths || []) {
        if (!graph.has(pathDef.from) || !graph.has(pathDef.to)) {
            continue;
        }
        graph.get(pathDef.from).add(pathDef.to);
        graph.get(pathDef.to).add(pathDef.from);
    }
    const reachable = new Set();
    const queue = layout.entry && graph.has(layout.entry) ? [layout.entry] : [];
    while (queue.length) {
        const current = queue.shift();
        if (reachable.has(current)) {
            continue;
        }
        reachable.add(current);
        for (const next of graph.get(current) || []) {
            if (!reachable.has(next)) {
                queue.push(next);
            }
        }
    }
    return { graph, reachable };
}

function branchDegrees(layout) {
    const { graph } = reachablePathNodes(layout);
    return [...graph.values()].map((edges) => edges.size).filter((degree) => degree > 0).sort((a, b) => a - b);
}

export function topologySignature(layout) {
    const payload = {
        grid: layout.grid ? gridSpec(layout.grid) : null,
        nodes: Object.entries(layout.nodes || {})
            .filter(([nodeId]) => (layout.paths || []).some((pathDef) => pathDef.from === nodeId || pathDef.to === nodeId))
            .map(([nodeId, raw]) => [nodeId, raw.map((value) => round(value, 1))])
            .sort(([a], [b]) => a.localeCompare(b)),
        paths: (layout.paths || [])
            .map((pathDef) => [pathDef.id, pathDef.from, pathDef.to])
            .sort(([a], [b]) => a.localeCompare(b)),
        water: (layout.waterways || []).map((waterway) => [
            waterway.kind || 'water',
            waterway.points.map((raw) => raw.map((value) => round(value, 1))),
        ]),
        bridges: (layout.bridges || []).map((bridge) => [bridge.path, bridge.waterway, bridge.node]).sort(),
        cottages: (layout.cottage_sites || []).map((site) => [site.asset, site.door, site.body_at]).sort(),
        degrees: branchDegrees(layout),
    };
    return hashHex(payload).slice(0, 20);
}

export function validateOutdoorLayout(
    layout,
    { grid = defaultIsoGrid, visualWaterExclusions = [], prefabCatalog = builtinPrefabCatalog } = {},
) {
    const errors = [];
    if (!layout || typeof layout !== 'object') {
        throw new Error('layout must be an object');
    }
    const resolvedGrid = validateGridSpec(layout.grid || grid);
    if (!layout.id || !layout.name) {
        errors.push('layout must have id and name');
    }
    if (!layout.nodes || typeof layout.nodes !== 'object') {
        errors.push('layout must define nodes');
    }
    if (!Array.isArray(layout.paths) || layout.paths.length === 0) {
        errors.push('layout must define at least one path');
    }
    if (!layout.entry || !layout.nodes?.[layout.entry]) {
        errors.push(`entry node '${layout.entry}' is missing`);
    }

    uniqueOrError((layout.paths || []).map((pathDef) => pathDef.id), 'paths', errors);
    uniqueOrError((layout.waterways || []).map((waterway) => waterway.id), 'waterways', errors);
    uniqueOrError((layout.bridges || []).map((bridge) => bridge.id), 'bridges', errors);
    uniqueOrError((layout.cottage_sites || []).map((site) => site.id), 'cottage sites', errors);
    uniqueOrError((layout.props || []).map((prop) => prop.id), 'props', errors);
    uniqueOrError((layout.npc_slots || []).map((slot) => slot.id), 'npc slots', errors);
    for (const record of placementRecords(layout)) {
        if (!record.prefab) {
            errors.push(`placement '${record.id}' must resolve through a prefab`);
        } else if (!prefabCatalog[record.prefab]) {
            errors.push(`placement '${record.id}' references missing prefab '${record.prefab}'`);
        }
    }

    for (const [nodeId, raw] of Object.entries(layout.nodes || {})) {
        try {
            const candidate = point(raw, `node '${nodeId}'`);
            if (!pointInBounds(candidate)) {
                errors.push(`node '${nodeId}' is out of bounds`);
            }
        } catch (error) {
            errors.push(error.message);
        }
    }

    for (const pathDef of layout.paths || []) {
        if (!layout.nodes?.[pathDef.from]) {
            errors.push(`path '${pathDef.id}' references missing from node '${pathDef.from}'`);
            continue;
        }
        if (!layout.nodes?.[pathDef.to]) {
            errors.push(`path '${pathDef.id}' references missing to node '${pathDef.to}'`);
            continue;
        }
        const segment = pathSegment(layout, pathDef.id);
        if (segmentLength(segment) < 3) {
            errors.push(`path '${pathDef.id}' is too short`);
        }
    }

    const { reachable } = reachablePathNodes(layout);
    for (const pathDef of layout.paths || []) {
        if (!reachable.has(pathDef.from) || !reachable.has(pathDef.to)) {
            errors.push(`path '${pathDef.id}' is disconnected from entry '${layout.entry}'`);
        }
    }
    for (const exit of layout.exits || []) {
        if (!layout.nodes?.[exit.node]) {
            errors.push(`exit '${exit.id}' references missing node '${exit.node}'`);
        } else if (!reachable.has(exit.node)) {
            errors.push(`exit '${exit.id}' is unreachable from entry '${layout.entry}'`);
        }
        if (!exit.command || !exit.label || !Number.isFinite(exit.location_id)) {
            errors.push(`exit '${exit.id}' needs command, label, and numeric location_id`);
        }
    }

    for (const waterway of layout.waterways || []) {
        if (!Array.isArray(waterway.points) || waterway.points.length < 2) {
            errors.push(`waterway '${waterway.id}' needs at least two points`);
            continue;
        }
        for (const [index, raw] of waterway.points.entries()) {
            const candidate = point(raw, `waterway '${waterway.id}' point ${index}`);
            if (!pointInBounds(candidate)) {
                errors.push(`waterway '${waterway.id}' point ${index} is out of bounds`);
            }
        }
    }

    for (const bridge of layout.bridges || []) {
        const waterway = (layout.waterways || []).find((candidate) => candidate.id === bridge.waterway);
        if (!waterway) {
            errors.push(`bridge '${bridge.id}' references missing waterway '${bridge.waterway}'`);
            continue;
        }
        if (!layout.paths?.some((pathDef) => pathDef.id === bridge.path)) {
            errors.push(`bridge '${bridge.id}' references missing path '${bridge.path}'`);
            continue;
        }
        if (!layout.nodes?.[bridge.node]) {
            errors.push(`bridge '${bridge.id}' references missing node '${bridge.node}'`);
            continue;
        }
        const center = nodePoint(layout, bridge.node);
        const path = pathSegment(layout, bridge.path);
        const pathDistance = distancePointToSegment(center, path.a, path.b);
        const waterDistance = distanceToWaterway(center, waterway);
        const crossingDistance = Math.min(
            ...waterwaySegments(waterway).map((waterSegment) => segmentDistance(path, waterSegment)),
        );
        const bridgeTolerance = Math.max(4, (waterway.width || 6) * 0.75);
        if (pathDistance > 5) {
            errors.push(`bridge '${bridge.id}' is ${round(pathDistance, 2)} from its path`);
        }
        if (waterDistance > bridgeTolerance) {
            errors.push(`bridge '${bridge.id}' is ${round(waterDistance, 2)} from its waterway`);
        }
        if (crossingDistance > bridgeTolerance) {
            errors.push(`bridge '${bridge.id}' path does not cross waterway '${waterway.id}'`);
        }
    }

    for (const pathDef of layout.paths || []) {
        const path = pathSegment(layout, pathDef.id);
        for (const waterway of layout.waterways || []) {
            const crosses = waterwaySegments(waterway).some((waterSegment) =>
                segmentsIntersect(path.a, path.b, waterSegment.a, waterSegment.b),
            );
            if (!crosses) {
                continue;
            }
            const covered = (layout.bridges || []).some((bridge) => bridge.path === pathDef.id && bridge.waterway === waterway.id);
            if (!covered) {
                errors.push(`path '${pathDef.id}' crosses waterway '${waterway.id}' without a bridge`);
            }
        }
    }

    for (const site of layout.cottage_sites || []) {
        if (!layout.nodes?.[site.door]) {
            errors.push(`cottage '${site.id}' references missing door node '${site.door}'`);
            continue;
        }
        const door = nodePoint(layout, site.door);
        const body = point(site.body_at, `cottage '${site.id}' body_at`);
        const chimneyOpening = site.chimney_opening
            ? point(site.chimney_opening, `cottage '${site.id}' chimney_opening`)
            : null;
        if (nearestPathDistance(layout, door) > 4) {
            errors.push(`cottage '${site.id}' door is not on a walkable path`);
        }
        if (pointInWater(layout, door)) {
            errors.push(`cottage '${site.id}' door is in water`);
        }
        if (pointInWater(layout, body)) {
            errors.push(`cottage '${site.id}' body is in water`);
        }
        if (distance(door, body) > 18) {
            errors.push(`cottage '${site.id}' door is too far from cottage body`);
        }
        if (!chimneyOpening) {
            errors.push(`cottage '${site.id}' is missing chimney_opening`);
        } else {
            if (!pointInBounds(chimneyOpening)) {
                errors.push(`cottage '${site.id}' chimney opening is out of bounds`);
            }
            if (pointInWater(layout, chimneyOpening)) {
                errors.push(`cottage '${site.id}' chimney opening is in water`);
            }
            if (chimneyOpening.y >= body.y) {
                errors.push(`cottage '${site.id}' chimney opening must be above the cottage body anchor`);
            }
            if (distance(chimneyOpening, body) > 32) {
                errors.push(`cottage '${site.id}' chimney opening is too far from the cottage body`);
            }
        }
    }

    for (const prop of layout.props || []) {
        if (!layout.nodes?.[prop.node]) {
            errors.push(`prop '${prop.id}' references missing node '${prop.node}'`);
            continue;
        }
        const candidate = nodePoint(layout, prop.node);
        const footprintPoints = assetFootprintPoints(prop.kind, candidate, prop.scale || 0.74, Boolean(prop.flip));
        const waterTest =
            prop.kind === 'cart'
                ? (_layout, sample) => pointInVisualWater(_layout, sample, visualWaterExclusions)
                : pointInWater;
        const wetPoint = footprintPoints.find((sample) => waterTest(layout, sample));
        if (wetPoint) {
            errors.push(`prop '${prop.id}' footprint is in water near ${round(wetPoint.x, 1)},${round(wetPoint.y, 1)}`);
        }
        if (nearestPathDistance(layout, candidate) > 12) {
            errors.push(`prop '${prop.id}' is too far from a path`);
        }
    }

    for (const slot of layout.npc_slots || []) {
        if (!layout.nodes?.[slot.node]) {
            errors.push(`npc slot '${slot.id}' references missing node '${slot.node}'`);
            continue;
        }
        const candidate = nodePoint(layout, slot.node);
        if (pointInWater(layout, candidate)) {
            errors.push(`npc slot '${slot.id}' is in water`);
        }
        if (nearestPathDistance(layout, candidate) > 8) {
            errors.push(`npc slot '${slot.id}' is not reachable from a path`);
        }
    }

    let gridTerrain = null;
    try {
        gridTerrain = buildGridTerrainModel(layout, {
            grid: resolvedGrid,
            visualWaterExclusions,
            prefabCatalog,
        });
        errors.push(...gridTerrain.errors);
    } catch (error) {
        errors.push(error.message);
    }

    if (errors.length) {
        throw new Error(`layout '${layout.id || 'unknown'}' invalid: ${errors.join('; ')}`);
    }

    return {
        ok: true,
        topology_signature: topologySignature(layout),
        node_count: Object.keys(layout.nodes || {}).length,
        path_count: layout.paths.length,
        waterway_count: (layout.waterways || []).length,
        bridge_count: (layout.bridges || []).length,
        exit_count: (layout.exits || []).length,
        reachable_node_count: reachable.size,
        branch_degrees: branchDegrees(layout),
        grid: gridTerrain?.summary,
    };
}

function findSourceScene(sceneIndex, sourceSlug) {
    const scene = sceneIndex.scenes?.find((candidate) => candidate.slug === sourceSlug);
    if (!scene) {
        throw new Error(`source scene '${sourceSlug}' not found`);
    }
    return scene;
}

function validateRecipe(recipe, sceneIndex) {
    if (!recipe || typeof recipe !== 'object') {
        throw new Error('village layout recipe must be a JSON object');
    }
    if (!recipe.source_slug) {
        throw new Error('village layout recipe is missing source_slug');
    }
    if (!Array.isArray(recipe.layouts)) {
        throw new Error('village layout recipe is missing layouts array');
    }
    const visualWaterExclusions = recipe.visual_water_exclusions || [];
    const prefabCatalog = prefabCatalogForRecipe(recipe);
    const terrainProfiles = terrainProfilesForRecipe(recipe);
    const grid = validateGridSpec(recipe.grid || defaultIsoGrid);
    validateVisualWaterExclusions(visualWaterExclusions);
    validatePrefabCatalog(prefabCatalog);
    validateTerrainProfiles(terrainProfiles);
    const requiredLayoutCount = recipe.required_layout_count || 10;
    if (recipe.layouts.length !== requiredLayoutCount) {
        throw new Error(`village layout recipe must declare exactly ${requiredLayoutCount} layouts, got ${recipe.layouts.length}`);
    }
    const assets = new Set((sceneIndex.assets || []).map((asset) => asset.id));
    for (const assetId of requiredAssetIds) {
        if (!assets.has(assetId)) {
            throw new Error(`required Kilteevan atom asset '${assetId}' is missing`);
        }
    }
    const layoutIds = new Set();
    const topologySignatures = new Set();
    const assignedTerrainProfiles = new Set();
    const terrainProfileSignatures = new Set();
    for (const layout of recipe.layouts) {
        if (layoutIds.has(layout.id)) {
            throw new Error(`layout recipe has duplicate id '${layout.id}'`);
        }
        layoutIds.add(layout.id);
        const terrainProfile = terrainProfileForLayout(recipe, layout);
        if (assignedTerrainProfiles.has(terrainProfile.id)) {
            throw new Error(`layout recipe has duplicate terrain profile '${terrainProfile.id}'`);
        }
        assignedTerrainProfiles.add(terrainProfile.id);
        const profileSignature = terrainProfileConfigSignature(terrainProfile);
        if (terrainProfileSignatures.has(profileSignature)) {
            throw new Error(`layout recipe has duplicate terrain profile signature '${profileSignature}'`);
        }
        terrainProfileSignatures.add(profileSignature);
        const validation = validateOutdoorLayout(layout, { grid, visualWaterExclusions, prefabCatalog });
        if (topologySignatures.has(validation.topology_signature)) {
            throw new Error(`layout recipe has duplicate topology signature '${validation.topology_signature}'`);
        }
        topologySignatures.add(validation.topology_signature);
    }
}

function makeLayoutSlug(recipe, layout, index) {
    const prefix = recipe.output_slug_prefix || `${recipe.source_slug}-layout`;
    return `${prefix}-${String(index + 1).padStart(2, '0')}-${slugify(layout.id)}`;
}

function createLayerBuilder() {
    const layers = [];
    const ids = new Set();
    const usedZ = new Set();
    const zCursors = {
        calibration: -1000,
        ground: -900,
        terrain_underpaint: -700,
        water: -500,
        terrain: -350,
        road: -220,
        bridge: -120,
        contact: -60,
        building: 10,
        wall: 24,
        prop: 34,
        foliage: 50,
        foreground: 70,
        smoke: 84,
        overlay: 1000,
    };
    function nextZ(group) {
        const key = group in zCursors ? group : 'prop';
        while (usedZ.has(zCursors[key])) {
            zCursors[key] += 1;
        }
        const z = zCursors[key];
        usedZ.add(z);
        zCursors[key] += 1;
        return z;
    }
    function uniqueId(id) {
        let candidate = id;
        let suffix = 2;
        while (ids.has(candidate)) {
            candidate = `${id}-${suffix}`;
            suffix += 1;
        }
        ids.add(candidate);
        return candidate;
    }
    function add({ id, asset, x, y, zGroup = 'prop', scale = 1, opacity = 1, flip = false, animation, labels }) {
        const layer = {
            id: uniqueId(id),
            asset,
            x: round(clamp(x, 0, 100), 1),
            y: round(clamp(y, 0, 100), 1),
            z: nextZ(zGroup),
            scale: round(clamp(scale, 0.05, 1.6), 3),
        };
        if (opacity !== 1) {
            layer.opacity = round(clamp(opacity, 0.05, 1), 3);
        }
        if (flip) {
            layer.flip = true;
        }
        if (animation) {
            layer.animation = animation;
        }
        if (labels?.length) {
            layer.labels = labels;
        }
        layers.push(layer);
        return layer;
    }
    return { layers, add };
}

function distanceToNearestWater(layout, candidate) {
    const waterways = layout.waterways || [];
    if (!waterways.length) {
        return Number.POSITIVE_INFINITY;
    }
    return Math.min(...waterways.map((waterway) => distanceToWaterway(candidate, waterway)));
}

function rankedGridSamples(grid, layout, profile, salt, predicate) {
    const samples = [];
    for (let col = 0; col < grid.cols; col += 1) {
        for (let row = 0; row < grid.rows; row += 1) {
            const center = cellCenter(grid, { col, row });
            if (predicate && !predicate(center, { col, row })) {
                continue;
            }
            samples.push({
                center,
                cell: { col, row },
                score: unitFromSeed(`${layout.id}:${profile.id}:${salt}:${col}:${row}`, 0),
            });
        }
    }
    return samples.sort((a, b) => b.score - a.score);
}

function jitterPoint(candidate, seed, index, amountX = 2.4, amountY = 1.8) {
    return {
        x: candidate.x + (unitFromSeed(seed, index * 2) - 0.5) * amountX,
        y: candidate.y + (unitFromSeed(seed, index * 2 + 1) - 0.5) * amountY,
    };
}

function terrainAtomScale(asset, seed, index, base = 1) {
    if (asset === 'kilteevan-ground-patch') {
        return base * (0.46 + unitFromSeed(seed, index + 33) * 0.24);
    }
    if (asset.includes('grass') || asset.includes('flower') || asset.includes('bramble')) {
        return base * (0.16 + unitFromSeed(seed, index + 33) * 0.18);
    }
    return base * (0.24 + unitFromSeed(seed, index + 33) * 0.18);
}

function addGeneratedTerrainBackground(builder, layout, profile, grid) {
    const metrics = {
        terrain_underpaint_layer_count: 0,
        generated_ground_patch_count: 0,
        generated_path_underpaint_count: 0,
        generated_bank_patch_count: 0,
        generated_vegetation_patch_count: 0,
        generated_mud_patch_count: 0,
        shared_ground_base_opacity: round(profile.base_opacity, 3),
    };
    const addTerrain = (layer, counter) => {
        builder.add(layer);
        metrics.terrain_underpaint_layer_count += 1;
        if (counter) {
            metrics[counter] += 1;
        }
    };

    builder.add({
        id: 'terrain-ground-calibration',
        asset: 'kilteevan-ground-base',
        x: 50,
        y: 50,
        zGroup: 'calibration',
        scale: 1,
        opacity: profile.base_opacity,
    });

    const groundSamples = rankedGridSamples(
        grid,
        layout,
        profile,
        'ground',
        (candidate) => distanceToNearestWater(layout, candidate) > 3.5 && nearestPathDistance(layout, candidate) > 2.2,
    ).slice(0, profile.ground_patch_count);
    for (const [index, sample] of groundSamples.entries()) {
        const asset = terrainAtoms[(index + hashByte(profile.id, index)) % terrainAtoms.length];
        const at = jitterPoint(sample.center, `${profile.id}:ground`, index, 3.2, 2.2);
        addTerrain(
            {
                id: `terrain-ground-${index}`,
                asset,
                x: at.x,
                y: at.y,
                zGroup: 'ground',
                scale: terrainAtomScale(asset, profile.id, index, 1.15),
                opacity: asset === 'kilteevan-ground-patch' ? 0.54 : 0.36,
                flip: index % 2 === 1,
            },
            'generated_ground_patch_count',
        );
    }

    for (const [pathIndex, pathDef] of (layout.paths || []).entries()) {
        const segment = pathSegment(layout, pathDef.id);
        const len = segmentLength(segment);
        const sampleCount = Math.max(2, Math.ceil(len / 12));
        for (let sampleIndex = 1; sampleIndex < sampleCount; sampleIndex += 1) {
            const t = sampleIndex / sampleCount;
            const sample = lerp(segment.a, segment.b, t);
            const asset = mudAtoms[(pathIndex + sampleIndex) % mudAtoms.length];
            const at = jitterPoint(sample, `${profile.id}:${pathDef.id}:path`, sampleIndex, 2.4, 1.6);
            addTerrain(
                {
                    id: `terrain-path-${pathDef.id}-${sampleIndex}`,
                    asset,
                    x: at.x,
                    y: at.y,
                    zGroup: 'terrain_underpaint',
                    scale: clamp((len / 74) * profile.path_width_scale, 0.24, 0.58),
                    opacity: 0.22 + profile.puddle_density * 0.18,
                    flip: pathIndex % 2 === 1,
                },
                'generated_path_underpaint_count',
            );
            if (unitFromSeed(`${profile.id}:${pathDef.id}:puddle`, sampleIndex) < profile.puddle_density) {
                addTerrain(
                    {
                        id: `terrain-puddle-${pathDef.id}-${sampleIndex}`,
                        asset: waterAtoms[(pathIndex + sampleIndex) % waterAtoms.length],
                        x: at.x + 0.7,
                        y: at.y + 0.3,
                        zGroup: 'terrain_underpaint',
                        scale: clamp((len / 180) * profile.path_width_scale, 0.12, 0.28),
                        opacity: 0.2 + profile.puddle_density * 0.22,
                        flip: sampleIndex % 2 === 0,
                    },
                    'generated_mud_patch_count',
                );
            }
        }
    }

    for (const [waterwayIndex, waterway] of (layout.waterways || []).entries()) {
        for (const [segmentIndex, segment] of waterwaySegments(waterway).entries()) {
            const len = segmentLength(segment);
            const sampleCount = Math.max(2, Math.ceil(len / 16));
            const dx = segment.b.x - segment.a.x;
            const dy = segment.b.y - segment.a.y;
            const segmentLen = Math.max(0.0001, len);
            const normal = { x: -dy / segmentLen, y: dx / segmentLen };
            for (let sampleIndex = 1; sampleIndex < sampleCount; sampleIndex += 1) {
                const t = sampleIndex / sampleCount;
                const sample = lerp(segment.a, segment.b, t);
                for (const side of [-1, 1]) {
                    if (metrics.generated_bank_patch_count >= profile.bank_patch_count) {
                        continue;
                    }
                    const bankOffset = (waterway.width || 6) * 0.35 * profile.water_bank_width * side;
                    const at = jitterPoint(
                        {
                            x: sample.x + normal.x * bankOffset,
                            y: sample.y + normal.y * bankOffset,
                        },
                        `${profile.id}:${waterway.id}:bank:${side}`,
                        segmentIndex * 11 + sampleIndex,
                        1.8,
                        1.2,
                    );
                    const asset = side < 0 ? 'kilteevan-kit-m2-mud-chip-a' : 'kilteevan-kit-m2-grass-tuft-a';
                    addTerrain(
                        {
                            id: `terrain-bank-${waterway.id}-${segmentIndex}-${sampleIndex}-${side < 0 ? 'a' : 'b'}`,
                            asset,
                            x: at.x,
                            y: at.y,
                            zGroup: 'terrain',
                            scale: clamp(((waterway.width || 6) / 30) * profile.water_bank_width, 0.12, 0.32),
                            opacity: side < 0 ? 0.24 : 0.28,
                            flip: side > 0,
                        },
                        'generated_bank_patch_count',
                    );
                }
            }
        }
    }

    const vegetationSamples = rankedGridSamples(
        grid,
        layout,
        profile,
        'vegetation',
        (candidate) => distanceToNearestWater(layout, candidate) > 2.4 && nearestPathDistance(layout, candidate) > 5.4,
    ).slice(0, profile.vegetation_patch_count);
    for (const [index, sample] of vegetationSamples.entries()) {
        const asset = foliageAtoms[(index + hashByte(profile.id, index + 41)) % foliageAtoms.length];
        const at = jitterPoint(sample.center, `${profile.id}:vegetation`, index, 3.8, 2.5);
        addTerrain(
            {
                id: `terrain-vegetation-${index}`,
                asset,
                x: at.x,
                y: at.y,
                zGroup: 'terrain',
                scale: terrainAtomScale(asset, profile.id, index, 1.05),
                opacity: 0.34 + unitFromSeed(profile.id, index + 55) * 0.2,
                flip: index % 2 === 0,
            },
            'generated_vegetation_patch_count',
        );
    }

    const mudSamples = rankedGridSamples(
        grid,
        layout,
        profile,
        'mud',
        (candidate) => distanceToNearestWater(layout, candidate) > 2 && nearestPathDistance(layout, candidate) <= 8,
    ).slice(0, profile.mud_patch_count);
    for (const [index, sample] of mudSamples.entries()) {
        const asset = mudAtoms[index % mudAtoms.length];
        const at = jitterPoint(sample.center, `${profile.id}:mud`, index, 3.1, 1.8);
        addTerrain(
            {
                id: `terrain-mud-${index}`,
                asset,
                x: at.x,
                y: at.y,
                zGroup: 'terrain_underpaint',
                scale: terrainAtomScale(asset, profile.id, index, profile.path_width_scale),
                opacity: 0.24 + profile.puddle_density * 0.18,
                flip: index % 2 === 1,
            },
            'generated_mud_patch_count',
        );
    }

    return metrics;
}

function addWaterways(builder, layout) {
    for (const waterway of layout.waterways || []) {
        const points = waterway.points.map((raw, index) => point(raw, `waterway '${waterway.id}' point ${index}`));
        const center = points.reduce((acc, candidate) => ({ x: acc.x + candidate.x, y: acc.y + candidate.y }), { x: 0, y: 0 });
        center.x /= points.length;
        center.y /= points.length;
        const length = waterwaySegments(waterway).reduce((sum, segment) => sum + segmentLength(segment), 0);
        builder.add({
            id: `${waterway.id}-stream-body`,
            asset: 'kilteevan-stream',
            x: center.x,
            y: center.y,
            zGroup: 'water',
            scale: clamp(length / 58, 0.72, 1.28),
            opacity: waterway.kind === 'river' ? 0.5 : 0.42,
            flip: unitFromSeed(waterway.id, 1) > 0.5,
        });
        for (const [index, segment] of waterwaySegments(waterway).entries()) {
            const mid = midpoint(segment.a, segment.b);
            const segmentLen = segmentLength(segment);
            builder.add({
                id: `${waterway.id}-ribbon-${index}`,
                asset: 'kilteevan-stream',
                x: mid.x,
                y: mid.y,
                zGroup: 'water',
                scale: clamp(segmentLen / 60, 0.38, 0.82),
                opacity: waterway.kind === 'river' ? 0.34 : 0.26,
                flip: index % 2 === 1,
            });
            const sampleCount = Math.max(2, Math.ceil(segmentLen / 18));
            for (let sampleIndex = 1; sampleIndex < sampleCount; sampleIndex += 1) {
                const t = sampleIndex / sampleCount;
                const sample = lerp(segment.a, segment.b, t);
                builder.add({
                    id: `${waterway.id}-continuity-${index}-${sampleIndex}`,
                    asset: waterAtoms[(index + sampleIndex) % waterAtoms.length],
                    x: sample.x + (unitFromSeed(waterway.id, index * 17 + sampleIndex) - 0.5) * 1.8,
                    y: sample.y + (unitFromSeed(waterway.id, index * 19 + sampleIndex) - 0.5) * 1.2,
                    zGroup: 'water',
                    scale: clamp((waterway.width || 6) / 24, 0.16, 0.38),
                    opacity: 0.28,
                    flip: (index + sampleIndex) % 2 === 0,
                });
            }
            builder.add({
                id: `${waterway.id}-water-chip-${index}`,
                asset: waterAtoms[index % waterAtoms.length],
                x: mid.x + (unitFromSeed(waterway.id, index + 2) - 0.5) * 2,
                y: mid.y + (unitFromSeed(waterway.id, index + 7) - 0.5) * 2,
                zGroup: 'water',
                scale: clamp((waterway.width || 6) / 22, 0.18, 0.42),
                opacity: 0.34,
                flip: index % 2 === 1,
            });
            builder.add({
                id: `${waterway.id}-mud-bank-${index}`,
                asset: 'kilteevan-kit-m2-mud-chip-a',
                x: mid.x + (unitFromSeed(waterway.id, index + 11) - 0.5) * 4,
                y: mid.y + 3 + (unitFromSeed(waterway.id, index + 13) - 0.5) * 2,
                zGroup: 'terrain',
                scale: clamp((waterway.width || 6) / 34, 0.12, 0.3),
                opacity: 0.18,
                flip: index % 2 === 0,
            });
        }
    }
}

function addPaths(builder, layout) {
    for (const [index, pathDef] of layout.paths.entries()) {
        const segment = pathSegment(layout, pathDef.id);
        const len = segmentLength(segment);
        const mid = midpoint(segment.a, segment.b);
        const isBridgePath = (layout.bridges || []).some((bridge) => bridge.path === pathDef.id);
        builder.add({
            id: `${pathDef.id}-mud-lane`,
            asset: 'kilteevan-muddy-lane',
            x: mid.x,
            y: mid.y,
            zGroup: 'road',
            scale: clamp(len / 42, 0.38, 1.16),
            opacity: isBridgePath ? 0.2 : 0.28,
            flip: segment.a.x > segment.b.x,
        });
        const atom = isBridgePath
            ? 'kilteevan-kit-m2-road-stub-a'
            : index % 4 === 1
              ? 'kilteevan-kit-m2-road-bend-a'
              : index % 5 === 2
                ? 'kilteevan-kit-m2-road-fork-a'
                : 'kilteevan-kit-m2-road-straight-a';
        builder.add({
            id: `${pathDef.id}-road-atom`,
            asset: atom,
            x: mid.x + (unitFromSeed(pathDef.id, 1) - 0.5) * 2,
            y: mid.y + (unitFromSeed(pathDef.id, 2) - 0.5) * 2,
            zGroup: 'road',
            scale: clamp(len / 70, 0.24, 0.58),
            opacity: isBridgePath ? 0.3 : 0.48,
            flip: index % 2 === 1,
        });
        for (const [sampleIndex, t] of [0.32, 0.68].entries()) {
            const sample = lerp(segment.a, segment.b, t);
            builder.add({
                id: `${pathDef.id}-mud-chip-${sampleIndex}`,
                asset: 'kilteevan-kit-m2-mud-chip-a',
                x: sample.x + (unitFromSeed(pathDef.id, sampleIndex + 3) - 0.5) * 3,
                y: sample.y + (unitFromSeed(pathDef.id, sampleIndex + 5) - 0.5) * 2,
                zGroup: 'terrain',
                scale: clamp(len / 180, 0.12, 0.28),
                opacity: 0.2,
                flip: sampleIndex % 2 === 1,
            });
        }
        if (!isBridgePath && index % 2 === 0) {
            const puddle = lerp(segment.a, segment.b, 0.52);
            builder.add({
                id: `${pathDef.id}-puddle`,
                asset: waterAtoms[(index + 1) % waterAtoms.length],
                x: puddle.x + (unitFromSeed(pathDef.id, 9) - 0.5) * 2,
                y: puddle.y + (unitFromSeed(pathDef.id, 10) - 0.5) * 1.5,
                zGroup: 'road',
                scale: clamp(len / 160, 0.16, 0.32),
                opacity: 0.34,
                flip: index % 3 === 0,
            });
        }
    }
}

function addBridges(builder, layout) {
    for (const [index, bridge] of (layout.bridges || []).entries()) {
        const center = nodePoint(layout, bridge.node);
        builder.add({
            id: `${bridge.id}-span`,
            asset: 'kilteevan-bridge',
            x: center.x,
            y: center.y,
            zGroup: 'bridge',
            scale: bridge.scale || 1,
            opacity: 0.98,
            flip: index % 2 === 1,
        });
        builder.add({
            id: `${bridge.id}-planks`,
            asset: 'kilteevan-kit-m2-wood-planks-a',
            x: center.x + 0.2,
            y: center.y + 0.8,
            zGroup: 'bridge',
            scale: clamp((bridge.scale || 1) * 0.26, 0.2, 0.36),
            opacity: 0.55,
            flip: index % 2 === 0,
        });
        builder.add({
            id: `${bridge.id}-bank-stones`,
            asset: 'kilteevan-kit-m2-wall-straight-a',
            x: center.x - 6,
            y: center.y + 4,
            zGroup: 'wall',
            scale: 0.22,
            opacity: 0.62,
            flip: true,
        });
    }
}

function addCottages(builder, layout) {
    for (const site of layout.cottage_sites || []) {
        const body = point(site.body_at, `cottage '${site.id}' body_at`);
        const door = nodePoint(layout, site.door);
        const chimneyOpening = point(site.chimney_opening, `cottage '${site.id}' chimney_opening`);
        const rightFacing = site.asset.includes('right');
        builder.add({
            id: `${site.id}-body`,
            asset: site.asset,
            x: body.x,
            y: body.y,
            zGroup: 'building',
            scale: site.scale || 1,
        });
        builder.add({
            id: `${site.id}-roof-detail`,
            asset: rightFacing ? 'kilteevan-kit-m2-roof-cottage-a' : 'kilteevan-kit-m2-roof-corner-a',
            x: body.x + (rightFacing ? 2.4 : -1.8),
            y: body.y - 12.5,
            zGroup: 'building',
            scale: 0.36 * (site.scale || 1),
            opacity: 0.44,
            flip: rightFacing,
        });
        builder.add({
            id: `${site.id}-door-detail`,
            asset: 'kilteevan-kit-m2-cottage-door-a',
            x: door.x,
            y: door.y + 1.2,
            zGroup: 'building',
            scale: 0.2 * (site.scale || 1),
            opacity: 0.46,
            flip: rightFacing,
        });
        builder.add({
            id: `${site.id}-window-detail`,
            asset: 'kilteevan-kit-m2-cottage-window-a',
            x: body.x + (rightFacing ? -5 : 5),
            y: body.y + 4,
            zGroup: 'building',
            scale: 0.18 * (site.scale || 1),
            opacity: 0.42,
            flip: rightFacing,
        });
        builder.add({
            id: `${site.id}-smoke`,
            asset: 'kilteevan-smoke',
            x: chimneyOpening.x,
            y: chimneyOpening.y,
            zGroup: 'smoke',
            scale: 0.64 * (site.scale || 1),
            opacity: 0.52,
            flip: rightFacing,
            animation: {
                mode: 'drift',
                amplitude_x: rightFacing ? -0.8 : 0.8,
                amplitude_y: -1.8,
                alpha: 0.04,
                period_ms: 6100 + Math.round(unitFromSeed(site.id, 9) * 1400),
                phase: round(unitFromSeed(site.id, 10), 2),
            },
        });
        builder.add({
            id: `${site.id}-smoke-wisp`,
            asset: 'kilteevan-kit-m2-smoke-wisp-a',
            x: chimneyOpening.x,
            y: chimneyOpening.y,
            zGroup: 'smoke',
            scale: 0.28 * (site.scale || 1),
            opacity: 0.48,
            flip: rightFacing,
            animation: {
                mode: 'drift',
                amplitude_x: rightFacing ? -0.6 : 0.6,
                amplitude_y: -1.3,
                alpha: 0.035,
                period_ms: 5800 + Math.round(unitFromSeed(site.id, 11) * 1300),
                phase: round(unitFromSeed(site.id, 12), 2),
            },
        });
    }
}

function addProps(builder, layout) {
    for (const prop of layout.props || []) {
        const at = nodePoint(layout, prop.node);
        if (prop.kind === 'well') {
            builder.add({ id: `${prop.id}-well`, asset: 'kilteevan-well', x: at.x, y: at.y, zGroup: 'prop', scale: 0.66 });
            builder.add({
                id: `${prop.id}-well-rim`,
                asset: 'kilteevan-kit-m2-well-rim-a',
                x: at.x,
                y: at.y + 1,
                zGroup: 'prop',
                scale: 0.25,
                opacity: 0.68,
            });
        } else if (prop.kind === 'cart') {
            const flip = prop.flip ?? unitFromSeed(prop.id, 1) > 0.5;
            const scale = prop.scale || 0.74;
            builder.add({
                id: `${prop.id}-cart`,
                asset: 'kilteevan-cart',
                x: at.x,
                y: at.y,
                zGroup: 'prop',
                scale,
                flip,
            });
            builder.add({
                id: `${prop.id}-cart-wheel-a`,
                asset: 'kilteevan-kit-m2-cart-wheel-a',
                x: at.x + (flip ? 2.5 : -2.5),
                y: at.y + 1.4,
                zGroup: 'prop',
                scale: 0.2,
                opacity: 0.62,
            });
            builder.add({
                id: `${prop.id}-cart-wheel-b`,
                asset: 'kilteevan-kit-m2-cart-wheel-a',
                x: at.x + (flip ? -2.4 : 2.4),
                y: at.y + 1.8,
                zGroup: 'prop',
                scale: 0.18,
                opacity: 0.52,
            });
        } else if (prop.kind === 'signpost') {
            builder.add({
                id: `${prop.id}-signpost`,
                asset: 'kilteevan-signpost',
                x: at.x,
                y: at.y,
                zGroup: 'prop',
                scale: 0.58,
                labels: [{ text: 'Crossroads', anchor: [52, 42], rotation: -2 }],
            });
            builder.add({
                id: `${prop.id}-signpost-detail`,
                asset: 'kilteevan-kit-m2-signpost-a',
                x: at.x + 0.4,
                y: at.y + 1.1,
                zGroup: 'prop',
                scale: 0.28,
                opacity: 0.72,
            });
        } else if (prop.kind === 'market') {
            builder.add({
                id: `${prop.id}-planks`,
                asset: 'kilteevan-kit-m2-wood-planks-a',
                x: at.x,
                y: at.y,
                zGroup: 'prop',
                scale: 0.34,
                opacity: 0.78,
            });
            builder.add({
                id: `${prop.id}-basket-shadow`,
                asset: 'kilteevan-kit-m2-cart-wheel-a',
                x: at.x + 3,
                y: at.y + 1,
                zGroup: 'prop',
                scale: 0.16,
                opacity: 0.44,
            });
        }
    }
}

function addWalls(builder, layout) {
    for (const wall of layout.walls || []) {
        const points = wall.points.map((raw, index) => point(raw, `wall '${wall.id}' point ${index}`));
        for (let index = 0; index < points.length - 1; index += 1) {
            const a = points[index];
            const b = points[index + 1];
            const mid = midpoint(a, b);
            const len = distance(a, b);
            builder.add({
                id: `${wall.id}-run-${index}`,
                asset: 'kilteevan-kit-m2-wall-straight-a',
                x: mid.x,
                y: mid.y,
                zGroup: 'wall',
                scale: clamp(len / 34, 0.2, 0.38),
                opacity: 0.68,
                flip: a.x > b.x,
            });
            if (index === 0) {
                builder.add({
                    id: `${wall.id}-stones-${index}`,
                    asset: 'kilteevan-kit-wall-stones-a',
                    x: mid.x + 1.2,
                    y: mid.y - 0.6,
                    zGroup: 'wall',
                    scale: 0.42,
                    opacity: 0.26,
                    flip: a.x > b.x,
                });
            }
        }
    }
}

function addFoliage(builder, layout) {
    for (const cluster of layout.foliage || []) {
        const at = point(cluster.at, `foliage '${cluster.id}' at`);
        const count = cluster.count || 4;
        for (let index = 0; index < count; index += 1) {
            const offsetX = (unitFromSeed(cluster.id, index * 2) - 0.5) * 10;
            const offsetY = (unitFromSeed(cluster.id, index * 2 + 1) - 0.5) * 7;
            builder.add({
                id: `${cluster.id}-foliage-${index}`,
                asset: foliageAtoms[index % foliageAtoms.length],
                x: at.x + offsetX,
                y: at.y + offsetY,
                zGroup: 'foliage',
                scale: 0.16 + unitFromSeed(cluster.id, index + 9) * 0.2,
                opacity: 0.48 + unitFromSeed(cluster.id, index + 19) * 0.26,
                flip: index % 2 === 1,
            });
        }
        builder.add({
            id: `${cluster.id}-hedgerow-body`,
            asset: 'kilteevan-hedgerow',
            x: at.x,
            y: at.y + 1.5,
            zGroup: 'foliage',
            scale: 0.45 + Math.min(0.35, count * 0.035),
            opacity: 0.58,
            flip: unitFromSeed(cluster.id, 99) > 0.5,
        });
    }
}

function addFinalOverlays(builder) {
    builder.add({
        id: 'contact-shadows',
        asset: 'kilteevan-contact-shadows',
        x: 50,
        y: 50,
        zGroup: 'contact',
        scale: 1,
        opacity: 0.32,
    });
    builder.add({
        id: 'damp-vignette',
        asset: 'kilteevan-damp-vignette',
        x: 50,
        y: 50,
        zGroup: 'overlay',
        scale: 1,
        opacity: 0.22,
    });
}

function generatedHotspots(layout) {
    const hotspots = [];
    for (const exit of layout.exits || []) {
        const at = nodePoint(layout, exit.node);
        hotspots.push({
            id: exit.id,
            shape: { rect: rectAround(at, 20, 18) },
            label: exit.label,
            action: { travel_to: exit.location_id },
        });
    }
    for (const prop of layout.props || []) {
        if (!prop.inspect) {
            continue;
        }
        const at = nodePoint(layout, prop.node);
        const size = prop.kind === 'cart' ? [18, 15] : [15, 16];
        hotspots.push({
            id: prop.id,
            shape: { rect: rectAround(at, size[0], size[1]) },
            label: prop.kind === 'signpost' ? 'The signpost' : prop.kind === 'cart' ? 'The cart' : prop.kind === 'market' ? 'The market planks' : 'The village well',
            action: { inspect: prop.inspect },
        });
    }
    return hotspots;
}

function generatedSlots(layout) {
    return (layout.npc_slots || []).map((slot) => {
        const at = nodePoint(layout, slot.node);
        return {
            id: slot.id,
            x: round(at.x, 1),
            y: round(at.y, 1),
            scale: round(slot.scale || 1, 3),
        };
    });
}

function generateLayoutScene({ sourceScene, recipe, layout, index, grid }) {
    const builder = createLayerBuilder();
    const terrainProfile = terrainProfileForLayout(recipe, layout);
    const terrainMetrics = addGeneratedTerrainBackground(builder, layout, terrainProfile, grid);
    addWaterways(builder, layout);
    addPaths(builder, layout);
    addBridges(builder, layout);
    addFinalOverlays(builder);
    addCottages(builder, layout);
    addWalls(builder, layout);
    addProps(builder, layout);
    addFoliage(builder, layout);

    return {
        terrainProfile,
        terrainMetrics,
        scene: {
            location_id: (recipe.location_id_base || 15100) + index,
            slug: makeLayoutSlug(recipe, layout, index),
            native_size: clone(sourceScene.native_size || [1280, 720]),
            underlay: sourceScene.underlay,
            plate: sourceScene.plate,
            layers: builder.layers,
            hotspots: generatedHotspots(layout),
            slots: generatedSlots(layout),
        },
    };
}

export function sceneSignature(scene) {
    return hashHex({
        layers: scene.layers.map((layer) => [
            layer.id,
            layer.asset,
            layer.x,
            layer.y,
            layer.z,
            layer.scale,
            layer.opacity ?? 1,
            Boolean(layer.flip),
        ]),
        hotspots: scene.hotspots.map((hotspot) => [hotspot.id, hotspot.shape?.rect, hotspot.action]),
        slots: scene.slots.map((slot) => [slot.id, slot.x, slot.y, slot.scale]),
    }).slice(0, 20);
}

function layerStats(scene, assetsById) {
    const kinds = new Set();
    let kitLayerCount = 0;
    let m2KitLayerCount = 0;
    let terrainLayerCount = 0;
    let groundBaseLayerCount = 0;
    for (const layer of scene.layers) {
        const asset = assetsById.get(layer.asset);
        if (asset?.kind) {
            kinds.add(asset.kind);
        }
        if (layer.asset === 'kilteevan-ground-base') {
            groundBaseLayerCount += 1;
        }
        if (layer.id.startsWith('terrain-') || (asset && ['ground', 'terrain_patch', 'road', 'water'].includes(asset.kind))) {
            terrainLayerCount += 1;
        }
        if (asset?.image?.includes('/atoms/kit/')) {
            kitLayerCount += 1;
            if (asset.image.includes('/atoms/kit/m2-')) {
                m2KitLayerCount += 1;
            }
        }
    }
    return {
        layer_count: scene.layers.length,
        kit_layer_count: kitLayerCount,
        m2_kit_layer_count: m2KitLayerCount,
        terrain_layer_count: terrainLayerCount,
        shared_ground_base_layer_count: groundBaseLayerCount,
        layer_kinds: [...kinds].sort(),
    };
}

function activationHints(layout) {
    return [
        ...(layout.exits || []).map((exit) => ({
            hotspot_id: exit.id,
            kind: 'travel',
            label: exit.label,
            command: exit.command,
            target_location_id: exit.location_id,
        })),
        ...(layout.props || [])
            .filter((prop) => prop.inspect)
            .map((prop) => ({
                hotspot_id: prop.id,
                kind: 'inspect',
                label: prop.kind,
                text: prop.inspect,
            })),
    ];
}

function assertGeneratedPack(pack) {
    const slugs = new Set();
    const locationIds = new Set();
    const signatures = new Set();
    const topologySignatures = new Set();
    const terrainProfiles = new Set();
    const terrainSignatures = new Set();
    for (const layout of pack.summary.layouts) {
        if (slugs.has(layout.slug)) {
            throw new Error(`duplicate generated slug '${layout.slug}'`);
        }
        if (locationIds.has(layout.location_id)) {
            throw new Error(`duplicate generated location_id '${layout.location_id}'`);
        }
        if (signatures.has(layout.scene_signature)) {
            throw new Error(`duplicate generated scene signature '${layout.scene_signature}'`);
        }
        if (topologySignatures.has(layout.topology_signature)) {
            throw new Error(`duplicate topology signature '${layout.topology_signature}'`);
        }
        if (terrainProfiles.has(layout.terrain_profile)) {
            throw new Error(`duplicate terrain profile '${layout.terrain_profile}'`);
        }
        if (terrainSignatures.has(layout.terrain_signature)) {
            throw new Error(`duplicate terrain signature '${layout.terrain_signature}'`);
        }
        slugs.add(layout.slug);
        locationIds.add(layout.location_id);
        signatures.add(layout.scene_signature);
        topologySignatures.add(layout.topology_signature);
        terrainProfiles.add(layout.terrain_profile);
        terrainSignatures.add(layout.terrain_signature);
    }
    for (const scene of pack.scenes) {
        const layerIds = new Set();
        const zValues = new Set();
        for (const layer of scene.layers) {
            if (layerIds.has(layer.id)) {
                throw new Error(`${scene.slug} has duplicate layer id '${layer.id}'`);
            }
            if (zValues.has(layer.z)) {
                throw new Error(`${scene.slug} has duplicate z '${layer.z}'`);
            }
            layerIds.add(layer.id);
            zValues.add(layer.z);
        }
        const hotspotIds = new Set();
        for (const hotspot of scene.hotspots || []) {
            if (hotspotIds.has(hotspot.id)) {
                throw new Error(`${scene.slug} has duplicate hotspot id '${hotspot.id}'`);
            }
            hotspotIds.add(hotspot.id);
        }
        const slotIds = new Set();
        for (const slot of scene.slots || []) {
            if (slotIds.has(slot.id)) {
                throw new Error(`${scene.slug} has duplicate slot id '${slot.id}'`);
            }
            slotIds.add(slot.id);
        }
    }
}

export function generateVillageLayoutPack({
    sceneIndex,
    recipe,
    sceneIndexPath = defaultSceneIndexPath,
    recipePath = defaultRecipePath,
} = {}) {
    validateRecipe(recipe, sceneIndex);
    const sourceScene = findSourceScene(sceneIndex, recipe.source_slug);
    const assetsById = new Map(sceneIndex.assets.map((asset) => [asset.id, asset]));
    const grid = validateGridSpec(recipe.grid || defaultIsoGrid);
    const visualWaterExclusions = recipe.visual_water_exclusions || [];
    const prefabCatalog = prefabCatalogForRecipe(recipe);
    const generated = recipe.layouts.map((layout, index) => generateLayoutScene({ sourceScene, recipe, layout, index, grid }));
    const scenes = generated.map((entry) => entry.scene);
    const summaryLayouts = generated.map(({ scene, terrainProfile, terrainMetrics }, index) => {
        const layout = recipe.layouts[index];
        const validation = validateOutdoorLayout(layout, { grid, visualWaterExclusions, prefabCatalog });
        return {
            index: index + 1,
            id: layout.id,
            name: layout.name,
            slug: scene.slug,
            location_id: scene.location_id,
            description: layout.description,
            scene_signature: sceneSignature(scene),
            terrain_profile: terrainProfile.id,
            terrain_profile_name: terrainProfile.name,
            terrain_signature: terrainSignature(layout, terrainProfile),
            terrain_profile_signature: terrainProfileConfigSignature(terrainProfile),
            ...terrainMetrics,
            topology_signature: validation.topology_signature,
            topology: validation,
            hotspot_count: scene.hotspots.length,
            slot_count: scene.slots.length,
            activation_hints: activationHints(layout),
            ...layerStats(scene, assetsById),
        };
    });
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
            layout_count: scenes.length,
            art_direction: recipe.art_direction,
            grid,
            terrain_profile_count: Object.keys(recipe.terrain_profiles || {}).length,
            prefab_catalog_ids: Object.keys(prefabCatalog).sort(),
            visual_water_exclusion_count: visualWaterExclusions.length,
            ai_asset_strategy: recipe.ai_asset_strategy,
            layouts: summaryLayouts,
        },
    };
    assertGeneratedPack(pack);
    return pack;
}

export async function loadVillageLayoutInputs({
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
    console.log(`Generated ${summary.layout_count} topology-aware village layout(s).`);
    for (const layout of summary.layouts) {
        console.log(
            `${String(layout.index).padStart(2, '0')}. ${layout.slug} ` +
                `layers=${layout.layer_count} kit=${layout.kit_layer_count} ` +
                `terrain=${layout.terrain_profile} underpaint=${layout.terrain_underpaint_layer_count} ` +
                `paths=${layout.topology.path_count} water=${layout.topology.waterway_count} ` +
                `bridges=${layout.topology.bridge_count} topology=${layout.topology_signature}`,
        );
    }
}

async function main() {
    const args = parseArgs(process.argv.slice(2));
    const inputs = await loadVillageLayoutInputs(args);
    const pack = generateVillageLayoutPack(inputs);
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
