import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

function runtimeSources(directory: string): string[] {
	return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
		const path = resolve(directory, entry.name);
		if (entry.isDirectory()) return runtimeSources(path);
		if (
			!/\.(?:svelte|ts)$/.test(entry.name) ||
			entry.name.endsWith('.test.ts')
		) {
			return [];
		}
		return [path];
	});
}

const PERSON_ART_RUNTIME_SOURCE = resolve(
	process.cwd(),
	'src/lib/notebook/person-art.ts',
);

const ACTIVE_VISUAL_SOURCES = [
	...runtimeSources(resolve(process.cwd(), 'src/lib/illustrated-parish')),
	...runtimeSources(
		resolve(process.cwd(), 'src/components/illustrated-notebook'),
	),
	PERSON_ART_RUNTIME_SOURCE,
	resolve(process.cwd(), 'src/routes/+page.svelte'),
];

describe('fresh illustrated parish provenance boundary', () => {
	it('does not ship the rejected asset kit or its dead renderer', () => {
		for (const path of [
			'static/notebook-ui',
			'scripts/generate-notebook-assets.mjs',
		]) {
			expect(existsSync(resolve(process.cwd(), path)), path).toBe(false);
		}
		const rejectedComponents = resolve(
			process.cwd(),
			'src/components/notebook',
		);
		expect(
			existsSync(rejectedComponents) ? readdirSync(rejectedComponents) : [],
		).toEqual([]);
		expect(
			readdirSync(
				resolve(process.cwd(), 'src/lib/illustrated-notebook'),
			).sort(),
		).toEqual(['command.test.ts', 'command.ts']);
		expect(
			readdirSync(resolve(process.cwd(), 'static/rundale/notebook-ui')).sort(),
		).toEqual([
			'asset-manifest.json',
			'asset-readme.md',
			'people',
			'person-art-contact-sheet.html',
			'person-art-contact-sheet.png',
			'person-art-provenance.md',
		]);
	});

	it('does not route through the rejected visual stack or broaden the approved person-art root', () => {
		for (const source of ACTIVE_VISUAL_SOURCES) {
			const text = readFileSync(source, 'utf8');
			expect(text).not.toMatch(
				/illustrated-notebook\/(?:renderer|layout|assets|types|interactions)|(?:components|\.\.)\/notebook\//,
			);
			expect(text.match(/\/rundale\/notebook-ui\//g) ?? []).toEqual(
				source === PERSON_ART_RUNTIME_SOURCE ? ['/rundale/notebook-ui/'] : [],
			);
		}
	});

	it('preserves the user-approved sewn notebook page exactly', () => {
		const page = readFileSync(
			resolve(
				process.cwd(),
				'static/rundale/illustrated-notebook-v2/sewn-notebook-page.png',
			),
		);
		expect(createHash('sha256').update(page).digest('hex')).toBe(
			'26aac148d97fdd47d7be7456ab99bb3dcc59bded4942bad34b428e3be9069445',
		);
	});
});
