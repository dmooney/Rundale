// Shared paths and helpers for the in-session lab scripts.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const lab = path.resolve(
	path.dirname(fileURLToPath(import.meta.url)),
	'..',
);
// Raw data: rendered requests, replies, judge batches and verdicts. Git ignores output/.
export const data = path.join(lab, 'output', 'insession');

export const readJson = (file) => JSON.parse(fs.readFileSync(file, 'utf8'));
export const writeJson = (file, value) => {
	fs.mkdirSync(path.dirname(file), { recursive: true });
	fs.writeFileSync(file, JSON.stringify(value, null, 1) + '\n');
};

// The lab's transform (promptfooconfig.yaml): the spoken text of a reply that
// is the v1 output JSON, or "MALFORMED: <raw>" for anything else.
export function spoken(raw) {
	try {
		const value = JSON.parse(raw);
		const dialogue = value && value.dialogue;
		if (typeof dialogue === 'string' && dialogue.trim()) return dialogue;
	} catch {}
	return 'MALFORMED: ' + raw;
}

// A small seeded generator, so a batch layout can be rebuilt from its label.
export function seeded(seed) {
	let state = 0;
	for (const ch of String(seed))
		state = (Math.imul(state, 31) + ch.charCodeAt(0)) >>> 0;
	return () => {
		state = (state + 0x6d2b79f5) >>> 0;
		let t = state;
		t = Math.imul(t ^ (t >>> 15), t | 1);
		t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
		return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
	};
}

export function shuffle(list, random) {
	const out = [...list];
	for (let i = out.length - 1; i > 0; i--) {
		const j = Math.floor(random() * (i + 1));
		[out[i], out[j]] = [out[j], out[i]];
	}
	return out;
}

export const mean = (xs) =>
	xs.length
		? Math.round((xs.reduce((a, b) => a + b, 0) / xs.length) * 10) / 10
		: null;
