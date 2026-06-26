import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { inflateSync } from 'node:zlib';
import { fileURLToPath } from 'node:url';

const scriptPath = fileURLToPath(import.meta.url);
const appDir = path.resolve(path.dirname(scriptPath), '..');
const repoRoot = path.resolve(appDir, '../../..');
const defaultScenesPath = path.join(repoRoot, 'mods/rundale/scenes.json');
const defaultModDir = path.join(repoRoot, 'mods/rundale');

const alphaEdgeThreshold = 16;
const visibleAlphaThreshold = 0;
const minMeaningfulVisiblePixels = 32;
const minMeaningfulAlphaCoverage = 0.0005;
const pixiStretchedStageKinds = new Set(['ground', 'underlay', 'plate', 'sky']);
const fullStageEffectKinds = new Set(['shadow', 'lighting']);

function paeth(left, up, upLeft) {
    const p = left + up - upLeft;
    const pa = Math.abs(p - left);
    const pb = Math.abs(p - up);
    const pc = Math.abs(p - upLeft);
    if (pa <= pb && pa <= pc) {
        return left;
    }
    return pb <= pc ? up : upLeft;
}

function channelsForColorType(colorType) {
    switch (colorType) {
        case 0:
            return 1;
        case 2:
            return 3;
        case 4:
            return 2;
        case 6:
            return 4;
        default:
            throw new Error(`unsupported PNG color type ${colorType}`);
    }
}

export function parsePng(buffer) {
    const signature = buffer.subarray(0, 8);
    if (!signature.equals(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]))) {
        throw new Error('not a PNG file');
    }

    let offset = 8;
    let ihdr = null;
    const idat = [];
    while (offset < buffer.length) {
        const length = buffer.readUInt32BE(offset);
        const type = buffer.toString('ascii', offset + 4, offset + 8);
        const dataStart = offset + 8;
        const dataEnd = dataStart + length;
        const data = buffer.subarray(dataStart, dataEnd);
        offset = dataEnd + 4;

        if (type === 'IHDR') {
            ihdr = {
                width: data.readUInt32BE(0),
                height: data.readUInt32BE(4),
                bitDepth: data[8],
                colorType: data[9],
                interlace: data[12],
            };
        } else if (type === 'IDAT') {
            idat.push(data);
        } else if (type === 'IEND') {
            break;
        }
    }
    if (!ihdr) {
        throw new Error('PNG missing IHDR');
    }
    if (ihdr.bitDepth !== 8 || ihdr.interlace !== 0) {
        throw new Error(`unsupported PNG encoding bitDepth=${ihdr.bitDepth} interlace=${ihdr.interlace}`);
    }

    const channels = channelsForColorType(ihdr.colorType);
    const bytesPerPixel = channels;
    const stride = ihdr.width * channels;
    const inflated = inflateSync(Buffer.concat(idat));
    const pixels = Buffer.alloc(ihdr.height * stride);

    let source = 0;
    for (let y = 0; y < ihdr.height; y += 1) {
        const filter = inflated[source];
        source += 1;
        const rowStart = y * stride;
        const prevStart = rowStart - stride;
        for (let x = 0; x < stride; x += 1) {
            const raw = inflated[source + x];
            const left = x >= bytesPerPixel ? pixels[rowStart + x - bytesPerPixel] : 0;
            const up = y > 0 ? pixels[prevStart + x] : 0;
            const upLeft = y > 0 && x >= bytesPerPixel ? pixels[prevStart + x - bytesPerPixel] : 0;
            let value;
            if (filter === 0) {
                value = raw;
            } else if (filter === 1) {
                value = raw + left;
            } else if (filter === 2) {
                value = raw + up;
            } else if (filter === 3) {
                value = raw + Math.floor((left + up) / 2);
            } else if (filter === 4) {
                value = raw + paeth(left, up, upLeft);
            } else {
                throw new Error(`unsupported PNG filter ${filter}`);
            }
            pixels[rowStart + x] = value & 0xff;
        }
        source += stride;
    }

    return {
        ...ihdr,
        channels,
        pixels,
    };
}

function alphaAt(image, x, y) {
    const index = (y * image.width + x) * image.channels;
    if (image.colorType === 6) {
        return image.pixels[index + 3];
    }
    if (image.colorType === 4) {
        return image.pixels[index + 1];
    }
    return 255;
}

export function visibleContentSummary(image, threshold = visibleAlphaThreshold) {
    let visiblePixels = 0;
    let minX = image.width;
    let minY = image.height;
    let maxX = -1;
    let maxY = -1;

    for (let y = 0; y < image.height; y += 1) {
        for (let x = 0; x < image.width; x += 1) {
            if (alphaAt(image, x, y) <= threshold) {
                continue;
            }
            visiblePixels += 1;
            minX = Math.min(minX, x);
            minY = Math.min(minY, y);
            maxX = Math.max(maxX, x);
            maxY = Math.max(maxY, y);
        }
    }

    const pixelCount = image.width * image.height;
    if (visiblePixels === 0) {
        return {
            width: image.width,
            height: image.height,
            visiblePixels,
            alphaCoverage: 0,
            bbox: null,
            bboxCoverage: 0,
            bboxAlphaCoverage: 0,
        };
    }

    const bbox = {
        x: minX,
        y: minY,
        width: maxX - minX + 1,
        height: maxY - minY + 1,
    };
    const bboxPixels = bbox.width * bbox.height;

    return {
        width: image.width,
        height: image.height,
        visiblePixels,
        alphaCoverage: visiblePixels / pixelCount,
        bbox,
        bboxCoverage: bboxPixels / pixelCount,
        bboxAlphaCoverage: visiblePixels / bboxPixels,
    };
}

export function alphaEdgeSummary(image, threshold = alphaEdgeThreshold) {
    const edges = {
        top: [],
        right: [],
        bottom: [],
        left: [],
    };
    for (let x = 0; x < image.width; x += 1) {
        edges.top.push(alphaAt(image, x, 0));
        edges.bottom.push(alphaAt(image, x, image.height - 1));
    }
    for (let y = 0; y < image.height; y += 1) {
        edges.left.push(alphaAt(image, 0, y));
        edges.right.push(alphaAt(image, image.width - 1, y));
    }
    return Object.fromEntries(
        Object.entries(edges).map(([edge, values]) => [
            edge,
            {
                max: Math.max(...values),
                strong: values.filter((value) => value > threshold).length,
            },
        ]),
    );
}

function hasStrongEdge(edgeSummary) {
    return Object.values(edgeSummary).some((edge) => edge.strong > 0);
}

function isLocalCompositorAtom(asset) {
    return asset.image.includes('/atoms/local/') || asset.image.includes('/atoms/kit/');
}

function isPixiStretchedStageKind(asset) {
    return pixiStretchedStageKinds.has(asset.kind);
}

function isFullStageEffectLayer(asset, layer, image, stageWidth, stageHeight) {
    if (!fullStageEffectKinds.has(asset.kind)) {
        return false;
    }
    const [anchorX = 50, anchorY = 50] = asset.anchor || [];
    const scale = layer.scale ?? 1;
    const reachesStageAxis = image.width >= stageWidth || image.height >= stageHeight;
    return reachesStageAxis && layer.x === 50 && layer.y === 50 && scale === 1 && anchorX === 50 && anchorY === 50;
}

function isMeaningfulAtom(content) {
    return (
        content.visiblePixels >= minMeaningfulVisiblePixels &&
        content.alphaCoverage >= minMeaningfulAlphaCoverage
    );
}

function blankAtomReason(content) {
    if (content.visiblePixels === 0) {
        return 'blank';
    }
    if (content.visiblePixels < minMeaningfulVisiblePixels) {
        return `near-blank: only ${content.visiblePixels} visible pixels`;
    }
    if (content.alphaCoverage < minMeaningfulAlphaCoverage) {
        return `near-blank: alpha coverage ${formatRatio(content.alphaCoverage)}`;
    }
    return null;
}

function formatRatio(value) {
    return Number(value.toFixed(6));
}

function formatContent(content) {
    return {
        width: content.width,
        height: content.height,
        visiblePixels: content.visiblePixels,
        alphaCoverage: formatRatio(content.alphaCoverage),
        bbox: content.bbox,
        bboxCoverage: formatRatio(content.bboxCoverage),
        bboxAlphaCoverage: formatRatio(content.bboxAlphaCoverage),
    };
}

function shouldCheckEdgeAlpha(asset) {
    return isLocalCompositorAtom(asset) || asset.kind === 'shadow';
}

const defaultSceneAuditConfigs = [
    {
        slug: 'kilteevan-village',
        requiredReusableKitKinds: [],
        minKitLayers: 0,
        minReusedKitAssets: 0,
    },
    {
        slug: 'the-crossroads',
        requiredReusableKitKinds: ['water', 'wall', 'foliage'],
        minKitLayers: 4,
        minReusedKitAssets: 1,
    },
    {
        slug: 'darcys-pub',
        requiredReusableKitKinds: ['vessel', 'wood', 'lighting'],
        minKitLayers: 10,
        minReusedKitAssets: 1,
        allowedFullStageAssetIds: [
            'pub-hearth',
            'pub-back-shelves',
            'pub-bar-counter',
            'pub-door-window',
            'pub-foreground-furniture',
        ],
    },
];

export async function auditSceneAtoms({
    slug,
    requiredReusableKitKinds = [],
    minKitLayers = 4,
    minReusedKitAssets = 1,
    allowedFullStageAssetIds = [],
    scenesPath = defaultScenesPath,
    modDir = defaultModDir,
} = {}) {
    const scenes = JSON.parse(await readFile(scenesPath, 'utf8'));
    const assetsById = new Map(scenes.assets.map((asset) => [asset.id, asset]));
    const scene = scenes.scenes.find((candidate) => candidate.slug === slug);
    if (!scene) {
        return { ok: false, failures: [`missing ${slug} scene`] };
    }

    const failures = [];
    const kitUsage = new Map();
    const kitKindByAsset = new Map();
    const allowedFullStageAssets = new Set(allowedFullStageAssetIds);
    let checkedPngs = 0;
    let kitLayerCount = 0;
    let meaningfulAtoms = 0;
    const atomSummaries = [];
    const blankAtoms = [];
    const suspiciousFullStageAtoms = [];
    const fullStageEffectOverlays = [];

    for (const layer of scene.layers) {
        const asset = assetsById.get(layer.asset);
        if (!asset) {
            failures.push(`${layer.id}: missing asset '${layer.asset}'`);
            continue;
        }
        if (/\.svg(?:$|\?)/i.test(asset.image)) {
            failures.push(`${layer.id}: SVG scene atom is not allowed (${asset.image})`);
            continue;
        }
        if (!/\.png(?:$|\?)/i.test(asset.image)) {
            failures.push(`${layer.id}: scene atom must be a PNG (${asset.image})`);
            continue;
        }

        const image = parsePng(await readFile(path.join(modDir, asset.image)));
        checkedPngs += 1;
        const [stageWidth, stageHeight] = scene.native_size;
        const content = visibleContentSummary(image);
        const formattedContent = formatContent(content);
        const atomSummary = {
            layerId: layer.id,
            assetId: asset.id,
            kind: asset.kind || 'prop',
            image: asset.image,
            ...formattedContent,
        };
        atomSummaries.push(atomSummary);
        if (isMeaningfulAtom(content)) {
            meaningfulAtoms += 1;
        } else {
            const reason = blankAtomReason(content);
            const blankAtom = {
                ...atomSummary,
                reason,
            };
            blankAtoms.push(blankAtom);
            failures.push(`${layer.id}: ${asset.image} is ${reason}`);
        }

        if (isFullStageEffectLayer(asset, layer, image, stageWidth, stageHeight)) {
            const expected = `${stageWidth}x${stageHeight}`;
            const actual = `${image.width}x${image.height}`;
            const overlay = {
                layerId: layer.id,
                assetId: asset.id,
                kind: asset.kind,
                image: asset.image,
                expectedSize: expected,
                actualSize: actual,
                exactNativeSize: image.width === stageWidth && image.height === stageHeight,
            };
            fullStageEffectOverlays.push(overlay);
            if (!overlay.exactNativeSize) {
                const suspiciousAtom = {
                    ...atomSummary,
                    reason: `${asset.kind} full-stage overlay must match native_size ${expected}, got ${actual}`,
                };
                suspiciousFullStageAtoms.push(suspiciousAtom);
                failures.push(`${layer.id}: ${suspiciousAtom.reason}`);
            }
        }

        if (
            !isPixiStretchedStageKind(asset) &&
            !isFullStageEffectLayer(asset, layer, image, stageWidth, stageHeight) &&
            !allowedFullStageAssets.has(asset.id) &&
            (image.width >= stageWidth || image.height >= stageHeight)
        ) {
            const suspiciousAtom = {
                ...atomSummary,
                reason: `non-stretched atom is stage-sized or larger (${image.width}x${image.height})`,
            };
            suspiciousFullStageAtoms.push(suspiciousAtom);
            failures.push(`${layer.id}: ${suspiciousAtom.reason}`);
        }

        if (asset.image.includes('/atoms/kit/')) {
            kitLayerCount += 1;
            const usage = kitUsage.get(layer.asset) || [];
            usage.push(layer);
            kitUsage.set(layer.asset, usage);
            kitKindByAsset.set(layer.asset, asset.kind || 'prop');
            if (image.width >= 360 || image.height >= 240) {
                failures.push(`${layer.id}: kit atom ${asset.image} is too large (${image.width}x${image.height})`);
            }
        }

        if (shouldCheckEdgeAlpha(asset)) {
            const edgeSummary = alphaEdgeSummary(image);
            if (hasStrongEdge(edgeSummary)) {
                failures.push(
                    `${layer.id}: ${asset.image} has strong alpha on PNG edge ${JSON.stringify(edgeSummary)}`,
                );
            }
        }
    }

    const reusedKitAssets = [...kitUsage.values()].filter((layers) => {
        const distinctPositions = new Set(layers.map((layer) => `${layer.x},${layer.y}`));
        return layers.length >= 3 && distinctPositions.size >= 3;
    });
    const reusableKitFamilies = new Set();
    for (const [assetId, layers] of kitUsage.entries()) {
        const distinctPositions = new Set(layers.map((layer) => `${layer.x},${layer.y}`));
        if (layers.length >= 3 && distinctPositions.size >= 3) {
            reusableKitFamilies.add(kitKindByAsset.get(assetId) || 'prop');
        }
    }
    if (kitLayerCount < minKitLayers) {
        failures.push(`${slug}: expected at least ${minKitLayers} kit layers, got ${kitLayerCount}`);
    }
    if (reusedKitAssets.length < minReusedKitAssets) {
        failures.push(`${slug}: expected at least ${minReusedKitAssets} kit asset reused in three positions`);
    }
    for (const requiredKind of requiredReusableKitKinds) {
        if (!reusableKitFamilies.has(requiredKind)) {
            failures.push(`${slug}: expected reusable ${requiredKind} kit family`);
        }
    }

    return {
        ok: failures.length === 0,
        failures,
        summary: {
            slug: scene.slug,
            layers: scene.layers.length,
            kitLayers: kitLayerCount,
            reusedKitAssets: reusedKitAssets.length,
            reusableKitFamilies: reusableKitFamilies.size,
            checkedPngs,
            meaningfulAtoms,
            blankAtoms,
            suspiciousFullStageAtoms,
            fullStageEffectOverlays,
            atoms: atomSummaries,
        },
    };
}

export async function auditConfiguredSceneAtoms({
    scenesPath = defaultScenesPath,
    modDir = defaultModDir,
    configs = defaultSceneAuditConfigs,
} = {}) {
    const results = [];
    for (const config of configs) {
        results.push(await auditSceneAtoms({ ...config, scenesPath, modDir }));
    }
    return {
        ok: results.every((result) => result.ok),
        results,
        failures: results.flatMap((result) => result.failures),
    };
}

export async function auditCrossroadsAtoms(options = {}) {
    return auditSceneAtoms({
        slug: 'the-crossroads',
        requiredReusableKitKinds: ['water', 'wall', 'foliage'],
        minKitLayers: 4,
        minReusedKitAssets: 1,
        ...options,
    });
}

if (process.argv[1] && path.resolve(process.argv[1]) === scriptPath) {
    const result = await auditConfiguredSceneAtoms();
    if (!result.ok) {
        console.error('atom audit failed:');
        for (const failure of result.failures) {
            console.error(`- ${failure}`);
        }
        process.exit(1);
    }
    for (const sceneResult of result.results) {
        const summary = sceneResult.summary;
        console.log(
            `Scene atom audit passed for ${summary.slug}: layers=${summary.layers}, kitLayers=${summary.kitLayers}, reusedKitAssets=${summary.reusedKitAssets}, reusableKitFamilies=${summary.reusableKitFamilies}, checkedPngs=${summary.checkedPngs}, meaningfulAtoms=${summary.meaningfulAtoms}, blankAtoms=${summary.blankAtoms.length}, suspiciousFullStageAtoms=${summary.suspiciousFullStageAtoms.length}, fullStageEffectOverlays=${summary.fullStageEffectOverlays.length}`,
        );
    }
}
