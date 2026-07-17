import { copyFile, mkdir, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { deflateSync } from 'node:zlib';

const here = dirname(fileURLToPath(import.meta.url));
const uiRoot = join(here, '..');
const sourceRoot = join(uiRoot, 'static', 'notebook-ui');
const generatedRoot = join(sourceRoot, 'generated');
const outRoot = join(uiRoot, 'static', 'rundale', 'notebook-ui');

function crc32(buf) {
	let c = 0xffffffff;
	for (let i = 0; i < buf.length; i += 1) {
		c ^= buf[i];
		for (let k = 0; k < 8; k += 1) {
			c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
		}
	}
	return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
	const typeBuf = Buffer.from(type);
	const len = Buffer.alloc(4);
	len.writeUInt32BE(data.length, 0);
	const crc = Buffer.alloc(4);
	crc.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 0);
	return Buffer.concat([len, typeBuf, data, crc]);
}

function png(width, height, rgba) {
	const signature = Buffer.from([
		0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
	]);
	const ihdr = Buffer.alloc(13);
	ihdr.writeUInt32BE(width, 0);
	ihdr.writeUInt32BE(height, 4);
	ihdr[8] = 8;
	ihdr[9] = 6;
	const stride = width * 4;
	const raw = Buffer.alloc((stride + 1) * height);
	for (let y = 0; y < height; y += 1) {
		raw[y * (stride + 1)] = 0;
		rgba.copy(raw, y * (stride + 1) + 1, y * stride, y * stride + stride);
	}
	return Buffer.concat([
		signature,
		chunk('IHDR', ihdr),
		chunk('IDAT', deflateSync(raw, { level: 9 })),
		chunk('IEND', Buffer.alloc(0)),
	]);
}

class Bitmap {
	constructor(width, height) {
		this.width = width;
		this.height = height;
		this.data = Buffer.alloc(width * height * 4);
	}

	blendPixel(x, y, color) {
		x = Math.round(x);
		y = Math.round(y);
		if (x < 0 || y < 0 || x >= this.width || y >= this.height) return;
		const i = (y * this.width + x) * 4;
		const a = color[3] / 255;
		const ia = 1 - a;
		this.data[i] = Math.round(color[0] * a + this.data[i] * ia);
		this.data[i + 1] = Math.round(color[1] * a + this.data[i + 1] * ia);
		this.data[i + 2] = Math.round(color[2] * a + this.data[i + 2] * ia);
		this.data[i + 3] = Math.min(
			255,
			Math.round(color[3] + this.data[i + 3] * ia),
		);
	}

	fill(color) {
		for (let i = 0; i < this.data.length; i += 4) {
			this.data[i] = color[0];
			this.data[i + 1] = color[1];
			this.data[i + 2] = color[2];
			this.data[i + 3] = color[3];
		}
	}

	dot(x, y, radius, color) {
		const r2 = radius * radius;
		for (
			let yy = Math.floor(y - radius);
			yy <= Math.ceil(y + radius);
			yy += 1
		) {
			for (
				let xx = Math.floor(x - radius);
				xx <= Math.ceil(x + radius);
				xx += 1
			) {
				const dx = xx - x;
				const dy = yy - y;
				if (dx * dx + dy * dy <= r2) this.blendPixel(xx, yy, color);
			}
		}
	}

	line(x1, y1, x2, y2, color = INK, width = 2) {
		const steps = Math.max(Math.abs(x2 - x1), Math.abs(y2 - y1), 1);
		for (let i = 0; i <= steps; i += 1) {
			const t = i / steps;
			this.dot(x1 + (x2 - x1) * t, y1 + (y2 - y1) * t, width / 2, color);
		}
	}

	ellipse(
		cx,
		cy,
		rx,
		ry,
		color = INK,
		width = 2,
		start = 0,
		end = Math.PI * 2,
	) {
		const steps = Math.max(80, Math.ceil((rx + ry) * 1.7));
		let last = null;
		for (let i = 0; i <= steps; i += 1) {
			const t = start + ((end - start) * i) / steps;
			const p = [cx + Math.cos(t) * rx, cy + Math.sin(t) * ry];
			if (last) this.line(last[0], last[1], p[0], p[1], color, width);
			last = p;
		}
	}

	fillEllipse(cx, cy, rx, ry, color) {
		for (let y = Math.floor(cy - ry); y <= Math.ceil(cy + ry); y += 1) {
			for (let x = Math.floor(cx - rx); x <= Math.ceil(cx + rx); x += 1) {
				const dx = (x - cx) / rx;
				const dy = (y - cy) / ry;
				if (dx * dx + dy * dy <= 1) this.blendPixel(x, y, color);
			}
		}
	}

	polygon(points, color, width = 2) {
		for (let i = 0; i < points.length; i += 1) {
			const a = points[i];
			const b = points[(i + 1) % points.length];
			this.line(a[0], a[1], b[0], b[1], color, width);
		}
	}

	fillPolygon(points, color) {
		const minY = Math.floor(Math.min(...points.map((p) => p[1])));
		const maxY = Math.ceil(Math.max(...points.map((p) => p[1])));
		for (let y = minY; y <= maxY; y += 1) {
			const xs = [];
			for (let i = 0; i < points.length; i += 1) {
				const a = points[i];
				const b = points[(i + 1) % points.length];
				if ((a[1] <= y && b[1] > y) || (b[1] <= y && a[1] > y)) {
					xs.push(a[0] + ((y - a[1]) * (b[0] - a[0])) / (b[1] - a[1]));
				}
			}
			xs.sort((a, b) => a - b);
			for (let i = 0; i < xs.length - 1; i += 2) {
				for (let x = Math.floor(xs[i]); x <= Math.ceil(xs[i + 1]); x += 1) {
					this.blendPixel(x, y, color);
				}
			}
		}
	}

	roughPaperRect(x, y, w, h) {
		const points = [
			[x + 7, y + 1],
			[x + w - 10, y + 3],
			[x + w - 2, y + 10],
			[x + w - 6, y + h - 7],
			[x + 8, y + h - 3],
			[x + 2, y + h - 11],
		];
		this.fillPolygon(points, PAPER);
		this.polygon(points, INK_SOFT, 3);
		this.polygon(
			points.map(([px, py]) => [px + 2, py + 2]),
			INK_FAINT,
			1,
		);
	}

	save(path) {
		return writeFile(path, png(this.width, this.height, this.data));
	}
}

const INK = [48, 35, 22, 235];
const INK_SOFT = [80, 57, 34, 210];
const INK_FAINT = [118, 87, 49, 125];
const PAPER = [230, 204, 151, 235];
const WASH = [130, 111, 65, 96];
const GREEN = [103, 126, 83, 230];
const BLUE = [71, 91, 105, 225];

function iconTalk() {
	const b = new Bitmap(128, 128);
	b.ellipse(60, 52, 38, 27, INK, 5);
	b.line(42, 74, 28, 94, INK, 5);
	b.line(42, 74, 57, 78, INK, 5);
	for (const x of [46, 60, 74]) b.fillEllipse(x, 52, 4, 4, INK);
	return b;
}

function iconAsk() {
	const b = new Bitmap(128, 128);
	b.ellipse(64, 64, 43, 43, INK, 5);
	b.ellipse(63, 50, 17, 18, INK, 5, Math.PI * 1.05, Math.PI * 2.15);
	b.line(65, 68, 65, 78, INK, 5);
	b.fillEllipse(65, 94, 4, 5, INK);
	return b;
}

function iconHelp() {
	const b = new Bitmap(128, 128);
	for (let i = 0; i < 5; i += 1) {
		const x = 42 + i * 10;
		b.line(x, 58, x - 4, 34 + (i % 2) * 3, INK, 5);
	}
	b.line(35, 58, 42, 96, INK, 6);
	b.line(83, 56, 78, 98, INK, 6);
	b.ellipse(61, 76, 28, 32, INK, 5, Math.PI * 0.05, Math.PI * 1.0);
	return b;
}

function iconObserve() {
	const b = new Bitmap(128, 128);
	b.ellipse(64, 64, 44, 24, INK, 5);
	b.fillEllipse(64, 64, 12, 12, BLUE);
	b.fillEllipse(64, 64, 6, 6, INK);
	b.line(24, 64, 12, 60, INK_FAINT, 2);
	b.line(104, 64, 116, 60, INK_FAINT, 2);
	return b;
}

function iconLeave() {
	const b = new Bitmap(128, 128);
	b.line(36, 27, 36, 101, INK, 5);
	b.line(36, 27, 76, 35, INK, 5);
	b.line(76, 35, 76, 93, INK, 5);
	b.line(36, 101, 76, 93, INK, 5);
	b.line(67, 64, 106, 64, INK, 5);
	b.line(91, 49, 106, 64, INK, 5);
	b.line(91, 79, 106, 64, INK, 5);
	return b;
}

function iconMap() {
	const b = new Bitmap(128, 128);
	const cols = [
		[
			[22, 34],
			[52, 22],
			[52, 92],
			[22, 106],
		],
		[
			[52, 22],
			[83, 36],
			[83, 106],
			[52, 92],
		],
		[
			[83, 36],
			[108, 24],
			[108, 92],
			[83, 106],
		],
	];
	for (const p of cols) {
		b.fillPolygon(p, [229, 206, 157, 120]);
		b.polygon(p, INK, 3);
	}
	b.line(34, 68, 56, 54, INK_FAINT, 2);
	b.line(56, 54, 76, 75, INK_FAINT, 2);
	b.line(76, 75, 101, 60, INK_FAINT, 2);
	return b;
}

function iconTime() {
	const b = new Bitmap(128, 128);
	b.line(42, 22, 86, 22, INK, 5);
	b.line(42, 106, 86, 106, INK, 5);
	b.line(50, 28, 78, 56, INK, 4);
	b.line(78, 28, 50, 56, INK, 4);
	b.line(50, 100, 78, 72, INK, 4);
	b.line(78, 100, 50, 72, INK, 4);
	b.fillEllipse(64, 65, 8, 6, WASH);
	return b;
}

function sendStamp() {
	const b = new Bitmap(128, 128);
	b.ellipse(64, 64, 40, 40, [122, 47, 36, 220], 6);
	b.ellipse(64, 64, 31, 31, [122, 47, 36, 165], 2);
	b.line(43, 70, 77, 39, INK, 4);
	b.line(77, 39, 90, 34, INK, 4);
	b.line(77, 39, 83, 54, INK, 3);
	b.line(46, 72, 72, 76, INK, 3);
	return b;
}

function paperExitLabel() {
	const b = new Bitmap(260, 82);
	b.roughPaperRect(8, 8, 244, 64);
	b.line(26, 40, 7, 40, INK_SOFT, 3);
	b.line(7, 40, 18, 31, INK_SOFT, 3);
	b.line(7, 40, 18, 50, INK_SOFT, 3);
	return b;
}

function inputLine() {
	const b = new Bitmap(760, 52);
	for (let x = 20; x < 720; x += 14) {
		const y = 32 + Math.sin(x / 34) * 1.6;
		b.line(x, y, x + 9, y + Math.sin(x / 21), INK_FAINT, 2);
	}
	return b;
}

function selectionRing() {
	const b = new Bitmap(160, 92);
	b.ellipse(80, 48, 54, 25, [255, 246, 214, 230], 6);
	b.ellipse(80, 48, 60, 29, [48, 35, 22, 85], 2);
	return b;
}

function playerMarker() {
	const b = new Bitmap(128, 180);
	b.fillEllipse(64, 35, 15, 18, [235, 214, 170, 245]);
	b.ellipse(64, 35, 15, 18, INK, 3);
	b.fillPolygon(
		[
			[47, 55],
			[81, 55],
			[93, 135],
			[35, 135],
		],
		BLUE,
	);
	b.polygon(
		[
			[47, 55],
			[81, 55],
			[93, 135],
			[35, 135],
		],
		INK,
		3,
	);
	b.line(52, 135, 45, 170, INK, 5);
	b.line(76, 135, 83, 170, INK, 5);
	return b;
}

function npcMarker(color = GREEN) {
	const b = new Bitmap(120, 170);
	b.fillEllipse(60, 34, 14, 17, [222, 200, 160, 245]);
	b.ellipse(60, 34, 14, 17, INK, 3);
	b.fillPolygon(
		[
			[45, 54],
			[75, 54],
			[84, 130],
			[36, 130],
		],
		color,
	);
	b.polygon(
		[
			[45, 54],
			[75, 54],
			[84, 130],
			[36, 130],
		],
		INK,
		3,
	);
	b.line(50, 130, 45, 160, INK, 4);
	b.line(70, 130, 75, 160, INK, 4);
	return b;
}

function bindingRings() {
	const b = new Bitmap(90, 620);
	for (let y = 34; y < 590; y += 35) {
		b.ellipse(45, y, 24, 8, INK, 4);
		b.line(20, y, 5, y, INK, 4);
	}
	return b;
}

function portraitFrame() {
	const b = new Bitmap(190, 210);
	b.roughPaperRect(8, 7, 174, 196);
	b.ellipse(95, 82, 51, 58, INK_SOFT, 3);
	b.line(48, 155, 142, 155, INK_FAINT, 2);
	return b;
}

function portrait(seed) {
	const b = new Bitmap(180, 190);
	const cx = 90;
	const hair = seed % 2 === 0 ? [48, 36, 26, 220] : [81, 60, 42, 220];
	b.ellipse(cx, 74, 38, 46, hair, 8, Math.PI * 0.85, Math.PI * 2.25);
	b.fillEllipse(cx, 82, 28, 35, [229, 205, 166, 235]);
	b.ellipse(cx, 82, 28, 35, INK, 3);
	b.line(cx - 9, 83, cx - 2, 83, INK, 2);
	b.line(cx + 9, 83, cx + 16, 83, INK, 2);
	b.line(cx + 3, 86, cx, 99, INK_FAINT, 2);
	b.ellipse(cx, 109, 12, 5, INK_FAINT, 2, 0, Math.PI);
	b.line(cx - 18, 120, cx - 45, 165, INK, 3);
	b.line(cx + 18, 120, cx + 45, 165, INK, 3);
	b.line(cx - 45, 165, cx + 45, 165, INK, 3);
	return b;
}

const copies = {
	'top-ribbon.png': join(generatedRoot, 'top-banner-v1.png'),
	'spiral-notebook-page.png': join(generatedRoot, 'notebook-page-v1.png'),
	'side-tab-notes.png': join(generatedRoot, 'tab-v1-notes.png'),
	'side-tab-people.png': join(generatedRoot, 'tab-v1-people.png'),
	'side-tab-places.png': join(generatedRoot, 'tab-v1-places.png'),
	'side-tab-rumours.png': join(generatedRoot, 'tab-v1-rumours.png'),
	'side-tab-journal.png': join(generatedRoot, 'tab-v1-journal.png'),
	'action-stamp-frame-a.png': join(generatedRoot, 'action-card-v1-a.png'),
	'action-stamp-frame-b.png': join(generatedRoot, 'action-card-v1-b.png'),
	'action-stamp-frame-c.png': join(generatedRoot, 'action-card-v1-c.png'),
	'intent-parchment-strip.png': join(generatedRoot, 'intent-slip-v1.png'),
	'active-intents-card.png': join(generatedRoot, 'active-intents-card-v1.png'),
	'map-card.png': join(generatedRoot, 'small-card-v1-a.png'),
	'time-card.png': join(generatedRoot, 'small-card-v1-b.png'),
	'nearby-portrait-strip.png': join(generatedRoot, 'people-rail-v1.png'),
	'scene-crossroads.png': join(sourceRoot, 'scene-crossroads.png'),
};

const generated = {
	'action-icon-talk.png': iconTalk(),
	'action-icon-ask.png': iconAsk(),
	'action-icon-help.png': iconHelp(),
	'action-icon-observe.png': iconObserve(),
	'action-icon-leave.png': iconLeave(),
	'map-icon.png': iconMap(),
	'time-icon.png': iconTime(),
	'ink-stamp-send.png': sendStamp(),
	'paper-exit-label.png': paperExitLabel(),
	'handwritten-input-line.png': inputLine(),
	'npc-selection-ring.png': selectionRing(),
	'player-marker.png': playerMarker(),
	'npc-marker-1.png': npcMarker(GREEN),
	'npc-marker-2.png': npcMarker([118, 86, 61, 230]),
	'npc-marker-3.png': npcMarker([90, 98, 118, 230]),
	'notebook-binding-rings.png': bindingRings(),
	'nearby-portrait-card-frame.png': portraitFrame(),
	'portrait-placeholder-1.png': portrait(1),
	'portrait-placeholder-2.png': portrait(2),
	'portrait-placeholder-3.png': portrait(3),
	'portrait-placeholder-4.png': portrait(4),
};

const manifest = {
	name: 'rundale-illustrated-notebook-ui',
	version: 1,
	source:
		'Generated/coded bitmap runtime kit. Concept art is visual reference only.',
	assets: {
		scenePlate: 'scene-crossroads.png',
		topRibbon: 'top-ribbon.png',
		spiralNotebookPage: 'spiral-notebook-page.png',
		notebookBindingRings: 'notebook-binding-rings.png',
		sideTabs: [
			'side-tab-notes.png',
			'side-tab-people.png',
			'side-tab-places.png',
			'side-tab-rumours.png',
			'side-tab-journal.png',
		],
		intentParchmentStrip: 'intent-parchment-strip.png',
		handwrittenInputLine: 'handwritten-input-line.png',
		inkStampSend: 'ink-stamp-send.png',
		actionStampFrames: [
			'action-stamp-frame-a.png',
			'action-stamp-frame-b.png',
			'action-stamp-frame-c.png',
		],
		actionIcons: {
			talk: 'action-icon-talk.png',
			ask: 'action-icon-ask.png',
			help: 'action-icon-help.png',
			observe: 'action-icon-observe.png',
			leave: 'action-icon-leave.png',
		},
		nearbyPortraitStrip: 'nearby-portrait-strip.png',
		nearbyPortraitCardFrame: 'nearby-portrait-card-frame.png',
		portraits: [
			'portrait-placeholder-1.png',
			'portrait-placeholder-2.png',
			'portrait-placeholder-3.png',
			'portrait-placeholder-4.png',
		],
		activeIntentsCard: 'active-intents-card.png',
		mapCard: 'map-card.png',
		timeCard: 'time-card.png',
		mapIcon: 'map-icon.png',
		timeIcon: 'time-icon.png',
		paperExitLabel: 'paper-exit-label.png',
		npcSelectionRing: 'npc-selection-ring.png',
		playerMarker: 'player-marker.png',
		npcMarkers: ['npc-marker-1.png', 'npc-marker-2.png', 'npc-marker-3.png'],
	},
};

const visualScenes = {
	version: 1,
	scenes: [
		{
			location_ids: [1, 15],
			plate_asset: '/rundale/notebook-ui/scene-crossroads.png',
			written_visual_summary:
				'Rural Ireland in 1820 after rain: a parish crossroads with whitewashed cottages, a chapel lane, a shop road, a stone bridge, wet grass, hedgerows, muddy tracks, and quiet villagers.',
			camera_hint: 'wide elevated oblique illustrated storybook game scene',
			background_generation_source:
				'Generated from the written prompt in docs/graphics-v2/illustrated-parish-scene-no-ui-prompt.md; no source imagery is used by this runtime plate.',
			depth_bands: [
				{ name: 'far', min_depth: 0, max_depth: 0.35, marker_scale: 0.5 },
				{ name: 'mid', min_depth: 0.35, max_depth: 0.7, marker_scale: 0.72 },
				{ name: 'near', min_depth: 0.7, max_depth: 1, marker_scale: 0.95 },
			],
			anchors: {
				player: { x: 0.48, y: 0.55, depth: 0.72 },
				npcs: [
					{ id: 'nearby-1', x: 0.51, y: 0.55, depth: 0.72 },
					{ id: 'nearby-2', x: 0.43, y: 0.48, depth: 0.58 },
					{ id: 'nearby-3', x: 0.68, y: 0.58, depth: 0.66 },
					{ id: 'nearby-4', x: 0.33, y: 0.69, depth: 0.82 },
				],
				exits: [
					{ id: 'chapel', label: 'Chapel Lane', x: 0.16, y: 0.15, depth: 0.18 },
					{ id: 'shop', label: 'Shop Road', x: 0.68, y: 0.43, depth: 0.46 },
					{ id: 'bridge', label: 'Bridge', x: 0.77, y: 0.58, depth: 0.64 },
				],
			},
		},
	],
};

const readme = `# Rundale Illustrated Notebook Runtime UI Assets

This directory is the production runtime asset kit for the PixiJS illustrated
notebook play screen.

## Source And Provenance

- The parchment frame crops are copied from the existing generated sheet at
  \`/notebook-ui/generated/notebook-ui-sheet-v1-source.png\`. Its manifest
  describes it as blank reusable hand-drawn parchment UI elements with no text,
  portraits, or scene content.
- The missing runtime controls and markers are generated by
  \`parish/apps/ui/scripts/generate-notebook-assets.mjs\` as original raster PNG
  line art: action icons, map/time icons, send stamp, exit label, input line,
  selection ring, player marker, NPC markers, binding rings, portrait card
  frame, and portrait placeholders.
- The scene plate is copied from \`/notebook-ui/scene-crossroads.png\`, which
  follows the written background prompt in
  \`docs/graphics-v2/illustrated-parish-scene-no-ui-prompt.md\`.
- \`docs/graphics-v2/illustrated-parish-notebook.png\` is visual reference only.
  No runtime asset in this directory is cut from that concept image.

## Runtime Usage

- \`asset-manifest.json\` is consumed by the Pixi renderer.
- \`visual-scenes.json\` supplies written visual summary, camera hint, plate path,
  scene anchors, and marker depth bands for the notebook scene.
- Svelte may size the canvas host and provide hidden accessibility inputs, but
  the first viewport notebook UI is intended to be rendered from these bitmap
  assets in Pixi.
`;

await mkdir(outRoot, { recursive: true });
await Promise.all(
	Object.entries(copies).map(([name, src]) =>
		copyFile(src, join(outRoot, name)),
	),
);
for (const [name, bitmap] of Object.entries(generated)) {
	await bitmap.save(join(outRoot, name));
}
await writeFile(
	join(outRoot, 'asset-manifest.json'),
	`${JSON.stringify(manifest, null, '\t')}\n`,
);
await writeFile(
	join(outRoot, 'visual-scenes.json'),
	`${JSON.stringify(visualScenes, null, '\t')}\n`,
);
await writeFile(join(outRoot, 'asset-readme.md'), readme);

console.log(
	`Generated ${Object.keys(copies).length + Object.keys(generated).length} notebook assets in ${outRoot}`,
);

await import('./build-notebook-person-art.mjs');
