import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { deflateSync, inflateSync } from 'node:zlib';

const here = dirname(fileURLToPath(import.meta.url));
const uiRoot = join(here, '..');
const artRoot = join(uiRoot, 'art', 'notebook-person-art');
const configPath = join(artRoot, 'approved-cast-v1.json');
const runtimeRoot = join(uiRoot, 'static', 'rundale', 'notebook-ui');
const peopleRoot = join(runtimeRoot, 'people');
const manifestPath = join(runtimeRoot, 'asset-manifest.json');
const assetReadmePath = join(runtimeRoot, 'asset-readme.md');
const provenancePath = join(runtimeRoot, 'person-art-provenance.md');

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

function encodePng(image) {
	const signature = Buffer.from([
		0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
	]);
	const ihdr = Buffer.alloc(13);
	ihdr.writeUInt32BE(image.width, 0);
	ihdr.writeUInt32BE(image.height, 4);
	ihdr[8] = 8;
	ihdr[9] = 6;
	const stride = image.width * 4;
	const raw = Buffer.alloc((stride + 1) * image.height);
	for (let y = 0; y < image.height; y += 1) {
		raw[y * (stride + 1)] = 0;
		image.data.copy(raw, y * (stride + 1) + 1, y * stride, y * stride + stride);
	}
	return Buffer.concat([
		signature,
		chunk('IHDR', ihdr),
		chunk('IDAT', deflateSync(raw, { level: 9 })),
		chunk('IEND', Buffer.alloc(0)),
	]);
}

function unfilterScanline(filter, line, prev, bpp) {
	const out = Buffer.from(line);
	for (let i = 0; i < out.length; i += 1) {
		const left = i >= bpp ? out[i - bpp] : 0;
		const up = prev ? prev[i] : 0;
		const upLeft = prev && i >= bpp ? prev[i - bpp] : 0;
		let add = 0;
		if (filter === 1) add = left;
		else if (filter === 2) add = up;
		else if (filter === 3) add = Math.floor((left + up) / 2);
		else if (filter === 4) {
			const p = left + up - upLeft;
			const pa = Math.abs(p - left);
			const pb = Math.abs(p - up);
			const pc = Math.abs(p - upLeft);
			add = pa <= pb && pa <= pc ? left : pb <= pc ? up : upLeft;
		} else if (filter !== 0) {
			throw new Error(`Unsupported PNG filter ${filter}`);
		}
		out[i] = (out[i] + add) & 0xff;
	}
	return out;
}

async function decodePng(path) {
	const buf = await readFile(path);
	const sig = buf.subarray(0, 8);
	if (sig.toString('hex') !== '89504e470d0a1a0a') {
		throw new Error(`${path} is not a PNG file`);
	}
	let offset = 8;
	let width = 0;
	let height = 0;
	let bitDepth = 0;
	let colorType = 0;
	let interlace = 0;
	const idat = [];
	while (offset < buf.length) {
		const len = buf.readUInt32BE(offset);
		const type = buf.subarray(offset + 4, offset + 8).toString('ascii');
		const data = buf.subarray(offset + 8, offset + 8 + len);
		offset += 12 + len;
		if (type === 'IHDR') {
			width = data.readUInt32BE(0);
			height = data.readUInt32BE(4);
			bitDepth = data[8];
			colorType = data[9];
			interlace = data[12];
		} else if (type === 'IDAT') {
			idat.push(data);
		} else if (type === 'IEND') {
			break;
		}
	}
	if (
		bitDepth !== 8 ||
		(colorType !== 2 && colorType !== 6) ||
		interlace !== 0
	) {
		throw new Error(
			`${path} must be a non-interlaced 8-bit RGB/RGBA PNG; got bitDepth=${bitDepth}, colorType=${colorType}, interlace=${interlace}`,
		);
	}
	const bpp = colorType === 6 ? 4 : 3;
	const inflated = inflateSync(Buffer.concat(idat));
	const stride = width * bpp;
	const out = Buffer.alloc(width * height * 4);
	let prev = null;
	for (let y = 0; y < height; y += 1) {
		const start = y * (stride + 1);
		const filter = inflated[start];
		const scanline = inflated.subarray(start + 1, start + 1 + stride);
		const line = unfilterScanline(filter, scanline, prev, bpp);
		prev = line;
		for (let x = 0; x < width; x += 1) {
			const src = x * bpp;
			const dst = (y * width + x) * 4;
			out[dst] = line[src];
			out[dst + 1] = line[src + 1];
			out[dst + 2] = line[src + 2];
			out[dst + 3] = colorType === 6 ? line[src + 3] : 255;
		}
	}
	return { width, height, data: out };
}

function createImage(width, height, rgba) {
	const data = Buffer.alloc(width * height * 4);
	for (let i = 0; i < data.length; i += 4) {
		data[i] = rgba[0];
		data[i + 1] = rgba[1];
		data[i + 2] = rgba[2];
		data[i + 3] = rgba[3];
	}
	return { width, height, data };
}

function getPixel(image, x, y) {
	const clampedX = Math.max(0, Math.min(image.width - 1, x));
	const clampedY = Math.max(0, Math.min(image.height - 1, y));
	const i = (clampedY * image.width + clampedX) * 4;
	return [
		image.data[i],
		image.data[i + 1],
		image.data[i + 2],
		image.data[i + 3],
	];
}

function setPixel(image, x, y, pixel) {
	if (x < 0 || y < 0 || x >= image.width || y >= image.height) return;
	const i = (y * image.width + x) * 4;
	image.data[i] = pixel[0];
	image.data[i + 1] = pixel[1];
	image.data[i + 2] = pixel[2];
	image.data[i + 3] = pixel[3];
}

function crop(image, rect) {
	const out = createImage(rect.width, rect.height, [0, 0, 0, 0]);
	for (let y = 0; y < rect.height; y += 1) {
		for (let x = 0; x < rect.width; x += 1) {
			setPixel(out, x, y, getPixel(image, rect.x + x, rect.y + y));
		}
	}
	return out;
}

function inkBounds(image) {
	let minX = image.width;
	let minY = image.height;
	let maxX = -1;
	let maxY = -1;
	for (let y = 0; y < image.height; y += 1) {
		for (let x = 0; x < image.width; x += 1) {
			const [r, g, b, a] = getPixel(image, x, y);
			const luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
			if (a > 32 && luma < 188) {
				minX = Math.min(minX, x);
				minY = Math.min(minY, y);
				maxX = Math.max(maxX, x);
				maxY = Math.max(maxY, y);
			}
		}
	}
	if (maxX < minX || maxY < minY) return null;
	return { minX, minY, maxX, maxY };
}

function cropInkWithPadding(image, padding) {
	const bounds = inkBounds(image);
	if (!bounds) return image;
	return crop(image, {
		x: Math.max(0, bounds.minX - padding.left),
		y: Math.max(0, bounds.minY - padding.top),
		width:
			Math.min(image.width - 1, bounds.maxX + padding.right) -
			Math.max(0, bounds.minX - padding.left) +
			1,
		height:
			Math.min(image.height - 1, bounds.maxY + padding.bottom) -
			Math.max(0, bounds.minY - padding.top) +
			1,
	});
}

function sampleBilinearPremul(image, x, y) {
	const x0 = Math.floor(x);
	const y0 = Math.floor(y);
	const x1 = x0 + 1;
	const y1 = y0 + 1;
	const tx = x - x0;
	const ty = y - y0;
	const weights = [
		[getPixel(image, x0, y0), (1 - tx) * (1 - ty)],
		[getPixel(image, x1, y0), tx * (1 - ty)],
		[getPixel(image, x0, y1), (1 - tx) * ty],
		[getPixel(image, x1, y1), tx * ty],
	];
	let r = 0;
	let g = 0;
	let b = 0;
	let a = 0;
	for (const [pixel, weight] of weights) {
		const alpha = pixel[3] * weight;
		r += pixel[0] * alpha;
		g += pixel[1] * alpha;
		b += pixel[2] * alpha;
		a += alpha;
	}
	if (a <= 0) return [0, 0, 0, 0];
	return [
		Math.round(r / a),
		Math.round(g / a),
		Math.round(b / a),
		Math.round(a),
	];
}

function resizeCover(image, width, height, background) {
	const out = createImage(width, height, background);
	const scale = Math.max(width / image.width, height / image.height);
	const scaledWidth = image.width * scale;
	const scaledHeight = image.height * scale;
	const offsetX = (width - scaledWidth) / 2;
	const offsetY = (height - scaledHeight) / 2;
	for (let y = 0; y < height; y += 1) {
		for (let x = 0; x < width; x += 1) {
			const sourceX = (x - offsetX + 0.5) / scale - 0.5;
			const sourceY = (y - offsetY + 0.5) / scale - 0.5;
			setPixel(out, x, y, sampleBilinearPremul(image, sourceX, sourceY));
		}
	}
	return out;
}

function resizeContain(image, width, height, background) {
	const out = createImage(width, height, background);
	const scale = Math.min(width / image.width, height / image.height);
	const scaledWidth = image.width * scale;
	const scaledHeight = image.height * scale;
	const offsetX = (width - scaledWidth) / 2;
	const offsetY = (height - scaledHeight) / 2;
	for (let y = 0; y < height; y += 1) {
		for (let x = 0; x < width; x += 1) {
			const sourceX = (x - offsetX + 0.5) / scale - 0.5;
			const sourceY = (y - offsetY + 0.5) / scale - 0.5;
			if (
				sourceX < 0 ||
				sourceY < 0 ||
				sourceX > image.width - 1 ||
				sourceY > image.height - 1
			) {
				continue;
			}
			setPixel(out, x, y, sampleBilinearPremul(image, sourceX, sourceY));
		}
	}
	return out;
}

function hexToRgb(hex) {
	const clean = hex.replace(/^#/, '');
	return [
		Number.parseInt(clean.slice(0, 2), 16),
		Number.parseInt(clean.slice(2, 4), 16),
		Number.parseInt(clean.slice(4, 6), 16),
	];
}

function applyChromaKey(image, hex) {
	const [kr, kg, kb] = hexToRgb(hex);
	const out = createImage(image.width, image.height, [0, 0, 0, 0]);
	for (let y = 0; y < image.height; y += 1) {
		for (let x = 0; x < image.width; x += 1) {
			const [r, g, b, a] = getPixel(image, x, y);
			const dist = Math.hypot(r - kr, g - kg, b - kb);
			let alpha = a;
			if (dist <= 28) alpha = 0;
			else if (dist < 140) alpha = Math.round(a * ((dist - 28) / 112));
			let rr = r;
			let gg = g;
			let bb = b;
			if (alpha < 255 && kr > 200 && kb > 200 && kg < 80) {
				rr = Math.min(rr, Math.max(gg + 72, 96));
				bb = Math.min(bb, Math.max(gg + 72, 96));
			}
			setPixel(out, x, y, [rr, gg, bb, alpha]);
		}
	}
	return out;
}

function cellRect(image, sheet, cell) {
	if (cell.column < 0 || cell.column >= sheet.columns) {
		throw new Error(`Invalid cell column ${cell.column} for ${sheet.path}`);
	}
	if (cell.row < 0 || cell.row >= sheet.rows) {
		throw new Error(`Invalid cell row ${cell.row} for ${sheet.path}`);
	}
	const x1 = Math.round((image.width * cell.column) / sheet.columns);
	const rowBounds = sheet.row_bounds?.[cell.row];
	const y1 = rowBounds
		? Math.round(rowBounds.y)
		: Math.round((image.height * cell.row) / sheet.rows);
	const x2 = Math.round((image.width * (cell.column + 1)) / sheet.columns);
	const y2 = rowBounds
		? Math.round(rowBounds.y + rowBounds.height)
		: Math.round((image.height * (cell.row + 1)) / sheet.rows);
	if (y1 < 0 || y2 > image.height || y2 <= y1) {
		throw new Error(
			`Invalid row_bounds for row ${cell.row} in ${sheet.path}: y=${y1}, y2=${y2}, image height=${image.height}`,
		);
	}
	return { x: x1, y: y1, width: x2 - x1, height: y2 - y1 };
}

function assertApproved(label, value) {
	if (value.approval_status !== 'approved') {
		throw new Error(`${label} is not approved; got ${value.approval_status}`);
	}
}

function alphaStats(image) {
	let opaque = 0;
	let transparent = 0;
	for (let i = 3; i < image.data.length; i += 4) {
		if (image.data[i] > 16) opaque += 1;
		if (image.data[i] < 16) transparent += 1;
	}
	return { opaque, transparent, total: image.width * image.height };
}

function validatePortrait(image, name) {
	const { opaque, total } = alphaStats(image);
	if (opaque < total * 0.9) {
		throw new Error(`${name} portrait is unexpectedly transparent or blank`);
	}
}

function validateMarker(image, name) {
	const { opaque, transparent, total } = alphaStats(image);
	if (opaque < total * 0.08) {
		throw new Error(`${name} marker is too sparse or blank`);
	}
	if (transparent < total * 0.3) {
		throw new Error(`${name} marker key removal left too little transparency`);
	}
	for (const [x, y] of [
		[0, 0],
		[image.width - 1, 0],
		[0, image.height - 1],
		[image.width - 1, image.height - 1],
	]) {
		if (getPixel(image, x, y)[3] > 4) {
			throw new Error(`${name} marker corner is not transparent`);
		}
	}
}

function composite(base, image, x, y) {
	for (let yy = 0; yy < image.height; yy += 1) {
		for (let xx = 0; xx < image.width; xx += 1) {
			const dstX = x + xx;
			const dstY = y + yy;
			if (dstX < 0 || dstY < 0 || dstX >= base.width || dstY >= base.height) {
				continue;
			}
			const src = getPixel(image, xx, yy);
			const dst = getPixel(base, dstX, dstY);
			const a = src[3] / 255;
			const ia = 1 - a;
			setPixel(base, dstX, dstY, [
				Math.round(src[0] * a + dst[0] * ia),
				Math.round(src[1] * a + dst[1] * ia),
				Math.round(src[2] * a + dst[2] * ia),
				255,
			]);
		}
	}
}

function fillRect(image, x, y, width, height, color) {
	for (let yy = y; yy < y + height; yy += 1) {
		for (let xx = x; xx < x + width; xx += 1) {
			setPixel(image, xx, yy, color);
		}
	}
}

function drawContactSheet(assets) {
	const cols = 4;
	const cellW = 276;
	const cellH = 250;
	const pad = 24;
	const sheet = createImage(
		cols * cellW + pad * 2,
		2 * cellH + pad * 2,
		[237, 218, 176, 255],
	);
	for (let y = 0; y <= 2; y += 1) {
		fillRect(sheet, pad, pad + y * cellH, cols * cellW, 2, [87, 62, 38, 130]);
	}
	for (let x = 0; x <= cols; x += 1) {
		fillRect(sheet, pad + x * cellW, pad, 2, 2 * cellH, [87, 62, 38, 130]);
	}
	assets.forEach((asset, index) => {
		const col = index % cols;
		const row = Math.floor(index / cols);
		const x = pad + col * cellW;
		const y = pad + row * cellH;
		const portrait = resizeContain(
			asset.portraitImage,
			126,
			134,
			[237, 218, 176, 255],
		);
		const marker = resizeCover(asset.markerImage, 96, 136, [0, 0, 0, 0]);
		composite(sheet, portrait, x + 28, y + 40);
		composite(sheet, marker, x + 160, y + 36);
	});
	return sheet;
}

function contactSheetHtml(config, people) {
	const rows = people
		.map(
			(person) => `<figure>
	<img src="${person.portrait}" alt="${person.display_name} portrait">
	<img src="${person.marker}" alt="${person.display_name} marker">
	<figcaption>${person.display_name}</figcaption>
</figure>`,
		)
		.join('\n');
	return `<!doctype html>
<meta charset="utf-8">
<title>Rundale Notebook Person Art Contact Sheet</title>
<style>
body { margin: 24px; background: #ead8af; color: #2f2316; font-family: Georgia, serif; }
h1 { font-size: 24px; font-weight: 400; }
.sheet { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 18px; max-width: 1100px; }
figure { margin: 0; padding: 14px; border: 1px solid rgba(47, 35, 22, 0.35); background: rgba(255, 248, 222, 0.45); }
img { max-width: 46%; height: 140px; object-fit: contain; vertical-align: middle; image-rendering: auto; }
figcaption { margin-top: 8px; font-size: 15px; }
</style>
<h1>Rundale Notebook Person Art Contact Sheet</h1>
<p>Config: ${config.issue ? `issue #${config.issue}` : 'approved cast'}; all entries approved.</p>
<div class="sheet">
${rows}
</div>
`;
}

function relativePromptPath(path) {
	return join('parish/apps/ui/art/notebook-person-art', path).replaceAll(
		'\\',
		'/',
	);
}

function provenanceMarkdown(config, people) {
	const rows = people
		.map(
			(person) =>
				`| ${person.display_name} | ${person.portrait} | ${person.marker} | ${person.approval_status} | ${person.review_notes} |`,
		)
		.join('\n');
	return `# Notebook Person Art Provenance

Generated by \`parish/apps/ui/scripts/build-notebook-person-art.mjs\` from
\`parish/apps/ui/art/notebook-person-art/approved-cast-v1.json\`.

## Sources

- Portrait source sheet: \`${relativePromptPath(config.source_sheets.portraits.path)}\`
- Portrait prompt: \`${relativePromptPath(config.source_sheets.portraits.prompt)}\`
- Marker source sheet: \`${relativePromptPath(config.source_sheets.markers.path)}\`
- Marker prompt: \`${relativePromptPath(config.source_sheets.markers.prompt)}\`
- Marker chroma key: \`${config.source_sheets.markers.key_color}\`

Both source sheets and every runtime entry must be marked \`approved\` in the
config or the pipeline exits without writing shipping assets.

## Runtime Assets

| Person | Portrait | Marker | Approval | Review notes |
| --- | --- | --- | --- | --- |
${rows}

Contact sheet: \`${config.contact_sheet}\`
HTML contact sheet: \`person-art-contact-sheet.html\`
`;
}

function assetReadme() {
	return `# Rundale Illustrated Notebook Runtime UI Assets

This directory is the production runtime asset kit for the PixiJS illustrated
notebook play screen.

## Source And Provenance

- The parchment frame crops are copied from the existing generated sheet at
  \`/notebook-ui/generated/notebook-ui-sheet-v1-source.png\`. Its manifest
  describes it as blank reusable hand-drawn parchment UI elements with no text,
  portraits, or scene content.
- Reusable runtime controls are generated by
  \`parish/apps/ui/scripts/generate-notebook-assets.mjs\` as original raster PNG
  line art: action icons, map/time icons, send stamp, exit label, input line,
  selection ring, player marker, binding rings, and portrait card frame.
- Approved person portraits and NPC markers are assembled by
  \`parish/apps/ui/scripts/build-notebook-person-art.mjs\` from reviewed source
  sheets and \`parish/apps/ui/art/notebook-person-art/approved-cast-v1.json\`.
  The pipeline refuses unapproved source sheets or person entries.
- The scene plate is copied from \`/notebook-ui/scene-crossroads.png\`, which
  follows the written background prompt in
  \`docs/graphics-v2/illustrated-parish-scene-no-ui-prompt.md\`.
- \`docs/graphics-v2/illustrated-parish-notebook.png\` is visual reference only.
  No runtime asset in this directory is cut from that concept image.

## Runtime Usage

- \`asset-manifest.json\` is consumed by the Pixi renderer.
- \`visual-scenes.json\` supplies written visual summary, camera hint, plate path,
  scene anchors, and marker depth bands for the notebook scene.
- \`person-art-provenance.md\` documents portrait/marker prompt, source, approval,
  and fallback provenance.
- \`person-art-contact-sheet.png\` and \`person-art-contact-sheet.html\` show the
  final approved portrait + marker set.
- Svelte may size the canvas host and provide hidden accessibility inputs, but
  the first viewport notebook UI is intended to be rendered from these bitmap
  assets in Pixi.
`;
}

async function writeRuntimeAsset(path, image) {
	await writeFile(path, encodePng(image));
}

async function main() {
	const config = JSON.parse(await readFile(configPath, 'utf8'));
	assertApproved('portrait source sheet', config.source_sheets.portraits);
	assertApproved('marker source sheet', config.source_sheets.markers);
	assertApproved('fallback person art', config.fallback);
	for (const person of config.people) {
		assertApproved(person.display_name, person);
	}

	await mkdir(peopleRoot, { recursive: true });
	const portraitSheet = await decodePng(
		join(artRoot, config.source_sheets.portraits.path),
	);
	const markerSheet = await decodePng(
		join(artRoot, config.source_sheets.markers.path),
	);
	const runtimePeople = [...config.people, config.fallback];
	const contactAssets = [];

	for (const person of runtimePeople) {
		const portraitCell = crop(
			portraitSheet,
			cellRect(
				portraitSheet,
				config.source_sheets.portraits,
				person.portrait_cell,
			),
		);
		const portraitContent = cropInkWithPadding(portraitCell, {
			left: 86,
			right: 86,
			top: 96,
			bottom: 118,
		});
		const portrait = resizeContain(
			portraitContent,
			config.portrait_size.width,
			config.portrait_size.height,
			[237, 218, 176, 255],
		);
		validatePortrait(portrait, person.display_name);
		await writeRuntimeAsset(join(runtimeRoot, person.portrait), portrait);

		const markerCell = crop(
			markerSheet,
			cellRect(markerSheet, config.source_sheets.markers, person.marker_cell),
		);
		const keyed = applyChromaKey(
			markerCell,
			config.source_sheets.markers.key_color,
		);
		const marker = resizeCover(
			keyed,
			config.marker_size.width,
			config.marker_size.height,
			[0, 0, 0, 0],
		);
		validateMarker(marker, person.display_name);
		await writeRuntimeAsset(join(runtimeRoot, person.marker), marker);
		contactAssets.push({
			person,
			portraitImage: portrait,
			markerImage: marker,
		});
	}

	const contactSheet = drawContactSheet(contactAssets);
	await writeRuntimeAsset(
		join(runtimeRoot, config.contact_sheet),
		contactSheet,
	);
	await writeFile(
		join(runtimeRoot, 'person-art-contact-sheet.html'),
		contactSheetHtml(config, runtimePeople),
	);

	const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
	const people = config.people.map((person) => ({
		real_name: person.real_name,
		display_name: person.display_name,
		npc_id: person.npc_id,
		portrait: person.portrait,
		marker: person.marker,
		approval_status: person.approval_status,
		source_config:
			'parish/apps/ui/art/notebook-person-art/approved-cast-v1.json',
		review_notes: person.review_notes,
	}));
	manifest.version = Math.max(Number(manifest.version ?? 1), 2);
	manifest.source =
		'Generated bitmap runtime kit plus approved notebook person art. Concept art is visual reference only.';
	manifest.assets.portraits = runtimePeople.map((person) => person.portrait);
	manifest.assets.npcMarkers = runtimePeople.map((person) => person.marker);
	manifest.assets.personArt = {
		source_config:
			'parish/apps/ui/art/notebook-person-art/approved-cast-v1.json',
		portrait_prompt: relativePromptPath(config.source_sheets.portraits.prompt),
		marker_prompt: relativePromptPath(config.source_sheets.markers.prompt),
		contact_sheet: config.contact_sheet,
		contact_sheet_html: 'person-art-contact-sheet.html',
		fallback: {
			display_name: config.fallback.display_name,
			portrait: config.fallback.portrait,
			marker: config.fallback.marker,
			approval_status: config.fallback.approval_status,
			review_notes: config.fallback.review_notes,
		},
		people,
	};
	await writeFile(manifestPath, `${JSON.stringify(manifest, null, '\t')}\n`);
	await writeFile(provenancePath, provenanceMarkdown(config, runtimePeople));
	await writeFile(assetReadmePath, assetReadme());

	console.log(
		`Built ${runtimePeople.length} approved notebook person portrait/marker pairs in ${runtimeRoot}`,
	);
}

await main();
