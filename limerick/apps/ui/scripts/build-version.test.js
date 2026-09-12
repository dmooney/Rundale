import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { frontendInputVersion } from '../svelte.config.js';

test('build identity ignores outputs and changes with source, assets, and configuration', () => {
	const root = mkdtempSync(join(tmpdir(), 'limerick-build-version-'));
	const write = (file, text) => writeFileSync(join(root, file), text);
	try {
		for (const directory of [
			'src',
			'static',
			'scripts',
			'dist',
			'.svelte-kit',
			'e2e',
			'node_modules',
		])
			mkdirSync(join(root, directory));
		for (const file of [
			'package.json',
			'package-lock.json',
			'svelte.config.js',
			'vite.config.ts',
			'tsconfig.json',
		])
			write(file, '{}');
		write('src/page.svelte', 'Rundale');
		write('static/scene.json', '{}');
		write('scripts/build.js', 'initial script');
		const original = frontendInputVersion(root);
		for (const directory of ['dist', '.svelte-kit', 'e2e', 'node_modules'])
			write(`${directory}/result.json`, 'noise');
		assert.equal(frontendInputVersion(root), original);
		for (const file of [
			'src/page.svelte',
			'static/scene.json',
			'scripts/build.js',
			'package.json',
			'vite.config.ts',
			'tsconfig.json',
			'svelte.config.js',
			'package-lock.json',
		]) {
			const before = frontendInputVersion(root);
			write(file, 'changed input');
			assert.notEqual(frontendInputVersion(root), before, file);
			assert.equal(frontendInputVersion(root), frontendInputVersion(root));
		}
	} finally {
		rmSync(root, { recursive: true, force: true });
	}
});
