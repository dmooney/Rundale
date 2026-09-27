import { readFile } from 'node:fs/promises';
import { deflateSync, inflateSync } from 'node:zlib';

function crc32(buffer) {
	let value = 0xffffffff;
	for (const byte of buffer) {
		value ^= byte;
		for (let bit = 0; bit < 8; bit += 1) {
			value = value & 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
		}
	}
	return (value ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
	const typeBuffer = Buffer.from(type);
	const length = Buffer.alloc(4);
	length.writeUInt32BE(data.length);
	const crc = Buffer.alloc(4);
	crc.writeUInt32BE(crc32(Buffer.concat([typeBuffer, data])));
	return Buffer.concat([length, typeBuffer, data, crc]);
}

export function createImage(width, height, rgba = [0, 0, 0, 0]) {
	const data = Buffer.alloc(width * height * 4);
	for (let offset = 0; offset < data.length; offset += 4) {
		data.set(rgba, offset);
	}
	return { width, height, data };
}

export function getPixel(image, x, y) {
	if (x < 0 || y < 0 || x >= image.width || y >= image.height) {
		return [0, 0, 0, 0];
	}
	const offset = (y * image.width + x) * 4;
	return [...image.data.subarray(offset, offset + 4)];
}

export function setPixel(image, x, y, pixel) {
	if (x < 0 || y < 0 || x >= image.width || y >= image.height) return;
	image.data.set(pixel, (y * image.width + x) * 4);
}

export function encodePng(image) {
	const signature = Buffer.from('89504e470d0a1a0a', 'hex');
	const header = Buffer.alloc(13);
	header.writeUInt32BE(image.width, 0);
	header.writeUInt32BE(image.height, 4);
	header[8] = 8;
	header[9] = 6;
	const stride = image.width * 4;
	const raw = Buffer.alloc((stride + 1) * image.height);
	for (let y = 0; y < image.height; y += 1) {
		image.data.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
	}
	return Buffer.concat([
		signature,
		chunk('IHDR', header),
		chunk('IDAT', deflateSync(raw, { level: 9 })),
		chunk('IEND', Buffer.alloc(0)),
	]);
}

function unfilter(filter, input, previous, bytesPerPixel) {
	const output = Buffer.from(input);
	for (let index = 0; index < output.length; index += 1) {
		const left = index >= bytesPerPixel ? output[index - bytesPerPixel] : 0;
		const up = previous?.[index] ?? 0;
		const upLeft =
			index >= bytesPerPixel ? (previous?.[index - bytesPerPixel] ?? 0) : 0;
		let addend = 0;
		if (filter === 1) addend = left;
		else if (filter === 2) addend = up;
		else if (filter === 3) addend = Math.floor((left + up) / 2);
		else if (filter === 4) {
			const estimate = left + up - upLeft;
			const distances = [
				Math.abs(estimate - left),
				Math.abs(estimate - up),
				Math.abs(estimate - upLeft),
			];
			addend =
				distances[0] <= distances[1] && distances[0] <= distances[2]
					? left
					: distances[1] <= distances[2]
						? up
						: upLeft;
		} else if (filter !== 0) {
			throw new Error(`Unsupported PNG filter ${filter}`);
		}
		output[index] = (output[index] + addend) & 0xff;
	}
	return output;
}

export function decodePngBytes(buffer, label = 'PNG') {
	if (buffer.subarray(0, 8).toString('hex') !== '89504e470d0a1a0a') {
		throw new Error(`${label} is not a PNG file`);
	}
	let offset = 8;
	let width;
	let height;
	let bitDepth;
	let colorType;
	let interlace;
	const compressed = [];
	while (offset + 12 <= buffer.length) {
		const length = buffer.readUInt32BE(offset);
		if (offset + 12 + length > buffer.length) {
			throw new Error(`${label} has a truncated PNG chunk`);
		}
		const type = buffer.subarray(offset + 4, offset + 8).toString('ascii');
		const data = buffer.subarray(offset + 8, offset + 8 + length);
		const expectedCrc = buffer.readUInt32BE(offset + 8 + length);
		const actualCrc = crc32(Buffer.concat([Buffer.from(type, 'ascii'), data]));
		if (actualCrc !== expectedCrc) {
			throw new Error(`${label} has an invalid ${type} chunk CRC`);
		}
		offset += 12 + length;
		if (type === 'IHDR') {
			width = data.readUInt32BE(0);
			height = data.readUInt32BE(4);
			bitDepth = data[8];
			colorType = data[9];
			interlace = data[12];
		} else if (type === 'IDAT') compressed.push(data);
		else if (type === 'IEND') break;
	}
	if (
		!width ||
		!height ||
		bitDepth !== 8 ||
		![2, 6].includes(colorType) ||
		interlace !== 0
	) {
		throw new Error(
			`${label} must be a non-interlaced 8-bit RGB/RGBA PNG; got ${width ?? 0}x${height ?? 0}, bitDepth=${bitDepth}, colorType=${colorType}, interlace=${interlace}`,
		);
	}
	const bytesPerPixel = colorType === 6 ? 4 : 3;
	const stride = width * bytesPerPixel;
	const inflated = inflateSync(Buffer.concat(compressed));
	if (inflated.length !== (stride + 1) * height)
		throw new Error(`${label} has invalid scanline data`);
	const image = createImage(width, height);
	let previous;
	for (let y = 0; y < height; y += 1) {
		const start = y * (stride + 1);
		const line = unfilter(
			inflated[start],
			inflated.subarray(start + 1, start + 1 + stride),
			previous,
			bytesPerPixel,
		);
		previous = line;
		for (let x = 0; x < width; x += 1) {
			const source = x * bytesPerPixel;
			setPixel(image, x, y, [
				line[source],
				line[source + 1],
				line[source + 2],
				colorType === 6 ? line[source + 3] : 255,
			]);
		}
	}
	return image;
}

export async function decodePng(path) {
	return decodePngBytes(await readFile(path), path);
}

export function alphaStats(image) {
	let visible = 0;
	let transparent = 0;
	for (let offset = 3; offset < image.data.length; offset += 4) {
		if (image.data[offset] > 16) visible += 1;
		if (image.data[offset] < 239) transparent += 1;
	}
	return { visible, transparent, total: image.width * image.height };
}

function samplePremultiplied(image, x, y) {
	const x0 = Math.floor(x);
	const y0 = Math.floor(y);
	const tx = x - x0;
	const ty = y - y0;
	const samples = [
		[getPixel(image, x0, y0), (1 - tx) * (1 - ty)],
		[getPixel(image, x0 + 1, y0), tx * (1 - ty)],
		[getPixel(image, x0, y0 + 1), (1 - tx) * ty],
		[getPixel(image, x0 + 1, y0 + 1), tx * ty],
	];
	let red = 0;
	let green = 0;
	let blue = 0;
	let alpha = 0;
	for (const [pixel, weight] of samples) {
		const weightedAlpha = pixel[3] * weight;
		red += pixel[0] * weightedAlpha;
		green += pixel[1] * weightedAlpha;
		blue += pixel[2] * weightedAlpha;
		alpha += weightedAlpha;
	}
	return alpha === 0
		? [0, 0, 0, 0]
		: [
				Math.round(red / alpha),
				Math.round(green / alpha),
				Math.round(blue / alpha),
				Math.round(alpha),
			];
}

export function resizeContain(image, width, height) {
	if (image.width === width && image.height === height)
		return { ...image, data: Buffer.from(image.data) };
	const output = createImage(width, height);
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
				sourceX < -0.5 ||
				sourceY < -0.5 ||
				sourceX > image.width - 0.5 ||
				sourceY > image.height - 0.5
			)
				continue;
			setPixel(output, x, y, samplePremultiplied(image, sourceX, sourceY));
		}
	}
	return output;
}

export function compositeOpaque(base, overlay, left, top) {
	for (let y = 0; y < overlay.height; y += 1) {
		for (let x = 0; x < overlay.width; x += 1) {
			const source = getPixel(overlay, x, y);
			const target = getPixel(base, left + x, top + y);
			const alpha = source[3] / 255;
			setPixel(base, left + x, top + y, [
				Math.round(source[0] * alpha + target[0] * (1 - alpha)),
				Math.round(source[1] * alpha + target[1] * (1 - alpha)),
				Math.round(source[2] * alpha + target[2] * (1 - alpha)),
				255,
			]);
		}
	}
}

export function fillRect(image, left, top, width, height, color) {
	for (let y = top; y < top + height; y += 1) {
		for (let x = left; x < left + width; x += 1) setPixel(image, x, y, color);
	}
}
