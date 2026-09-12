import adapter from '@sveltejs/adapter-static';
import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { join, posix } from 'node:path';
import { fileURLToPath } from 'node:url';

const inputDirectories = ['src', 'static', 'scripts'];
const inputFiles = [
	'package.json',
	'package-lock.json',
	'svelte.config.js',
	'vite.config.ts',
	'tsconfig.json',
];

// SvelteKit's timestamp default rotates every chunk hash on identical builds.
// Fingerprint only build inputs: outputs and test receipts cannot feed back in.
export function frontendInputVersion(
	root = fileURLToPath(new URL('.', import.meta.url)),
) {
	const hash = createHash('sha256');
	function add(file) {
		const bytes = readFileSync(join(root, file));
		hash.update(`${file.length}:${file}:${bytes.length}:`).update(bytes);
	}
	function visit(directory) {
		const entries = readdirSync(join(root, directory), { withFileTypes: true });
		entries.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
		for (const entry of entries) {
			const child = posix.join(directory, entry.name);
			if (entry.isDirectory()) visit(child);
			else if (entry.isFile()) add(child);
		}
	}
	for (const directory of inputDirectories) visit(directory);
	for (const file of inputFiles) add(file);
	return hash.digest('hex').slice(0, 16);
}

/** @type {import('@sveltejs/kit').Config} */
const config = {
	kit: {
		version: {
			name: frontendInputVersion(),
		},
		adapter: adapter({
			pages: 'dist',
			assets: 'dist',
			fallback: 'index.html',
			precompress: false,
			strict: false,
		}),
	},
};

export default config;
