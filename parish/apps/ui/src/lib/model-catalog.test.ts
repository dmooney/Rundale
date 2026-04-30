import { describe, expect, it } from 'vitest';

import { MODEL_CATALOG, detectModelTrigger, filterModels } from './model-catalog';

describe('model catalog', () => {
	it('exposes Anthropic, Ollama, and OpenRouter providers', () => {
		const providers = new Set(MODEL_CATALOG.map((m) => m.provider));
		expect(providers.has('Anthropic')).toBe(true);
		expect(providers.has('Ollama')).toBe(true);
		expect(providers.has('OpenRouter')).toBe(true);
	});

	it('returns the full list for an empty query', () => {
		expect(filterModels('')).toEqual(MODEL_CATALOG);
		expect(filterModels('   ')).toEqual(MODEL_CATALOG);
	});

	it('filters by substring in model name (case-insensitive)', () => {
		const matches = filterModels('OPUS');
		expect(matches.length).toBeGreaterThan(0);
		expect(matches.every((m) => m.name.toLowerCase().includes('opus'))).toBe(true);
	});

	it('filters by provider label', () => {
		const matches = filterModels('groq');
		expect(matches.length).toBeGreaterThan(0);
		expect(matches.every((m) => m.provider === 'Groq')).toBe(true);
	});

	it('returns an empty array when nothing matches', () => {
		expect(filterModels('definitely-not-a-model-xyz')).toEqual([]);
	});
});

describe('detectModelTrigger', () => {
	it('matches `/model ` with empty query', () => {
		expect(detectModelTrigger('/model ')).toEqual({ prefix: '/model', query: '' });
	});

	it('matches `/model claude` with the partial query', () => {
		expect(detectModelTrigger('/model claude')).toEqual({
			prefix: '/model',
			query: 'claude'
		});
	});

	it('matches per-category `/model.dialogue gpt`', () => {
		expect(detectModelTrigger('/model.dialogue gpt')).toEqual({
			prefix: '/model.dialogue',
			query: 'gpt'
		});
	});

	it('matches all four category subcommands', () => {
		for (const cat of ['dialogue', 'simulation', 'intent', 'reaction']) {
			expect(detectModelTrigger(`/model.${cat} `)).toEqual({
				prefix: `/model.${cat}`,
				query: ''
			});
		}
	});

	it('rejects unknown categories', () => {
		expect(detectModelTrigger('/model.bogus foo')).toBeNull();
	});

	it('rejects text without a trailing space after the command', () => {
		expect(detectModelTrigger('/model')).toBeNull();
		expect(detectModelTrigger('/model.dialogue')).toBeNull();
	});

	it('rejects unrelated commands', () => {
		expect(detectModelTrigger('/provider llama3')).toBeNull();
		expect(detectModelTrigger('go to pub')).toBeNull();
	});

	it('is case-insensitive on the prefix and normalises to lowercase', () => {
		expect(detectModelTrigger('/MODEL claude')).toEqual({
			prefix: '/model',
			query: 'claude'
		});
		expect(detectModelTrigger('/Model.Dialogue gpt')).toEqual({
			prefix: '/model.dialogue',
			query: 'gpt'
		});
	});
});
