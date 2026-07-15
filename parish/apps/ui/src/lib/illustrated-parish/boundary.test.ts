import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
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

const ACTIVE_VISUAL_SOURCES = [
	...runtimeSources(resolve(process.cwd(), 'src/lib/illustrated-parish')),
	...runtimeSources(
		resolve(process.cwd(), 'src/components/illustrated-notebook'),
	),
	resolve(process.cwd(), 'src/routes/+page.svelte'),
];

describe('fresh illustrated parish provenance boundary', () => {
	it('does not route through the rejected visual stack or asset kit', () => {
		for (const source of ACTIVE_VISUAL_SOURCES) {
			const text = readFileSync(source, 'utf8');
			expect(text).not.toMatch(
				/illustrated-notebook\/(?:renderer|layout|assets|types|interactions)|(?:components|\.\.)\/notebook\/|(?:\/rundale|static\/rundale)\/notebook-ui\//,
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
