import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import ModelDropdown from './ModelDropdown.svelte';
import type { ModelSuggestion } from '$lib/model-catalog';

const models: ModelSuggestion[] = [
	{ name: 'claude-sonnet-4-6', provider: 'Anthropic' },
	{ name: 'gpt-4o', provider: 'OpenAI' },
	{ name: 'llama3:8b', provider: 'Ollama' },
];

describe('ModelDropdown', () => {
	it('renders a listbox with the correct aria-label', () => {
		const { getByRole } = render(ModelDropdown, {
			props: {
				models,
				selectedIndex: 0,
				onSelect: vi.fn(),
				onHighlight: vi.fn(),
			},
		});
		const list = getByRole('listbox');
		expect(list.getAttribute('aria-label')).toBe('Model suggestions');
	});

	it('renders one option per model', () => {
		const { getAllByRole } = render(ModelDropdown, {
			props: {
				models,
				selectedIndex: 0,
				onSelect: vi.fn(),
				onHighlight: vi.fn(),
			},
		});
		expect(getAllByRole('option')).toHaveLength(models.length);
	});

	it('sets aria-selected=true only on the item at selectedIndex', () => {
		const { getAllByRole } = render(ModelDropdown, {
			props: {
				models,
				selectedIndex: 2,
				onSelect: vi.fn(),
				onHighlight: vi.fn(),
			},
		});
		const options = getAllByRole('option');
		expect(options[0].getAttribute('aria-selected')).toBe('false');
		expect(options[1].getAttribute('aria-selected')).toBe('false');
		expect(options[2].getAttribute('aria-selected')).toBe('true');
	});

	it('assigns id="model-option-{i}" to each item', () => {
		const { getAllByRole } = render(ModelDropdown, {
			props: {
				models,
				selectedIndex: 0,
				onSelect: vi.fn(),
				onHighlight: vi.fn(),
			},
		});
		getAllByRole('option').forEach((opt, i) => {
			expect(opt.id).toBe(`model-option-${i}`);
		});
	});

	it('renders both model name and provider for each entry', () => {
		const { getByText } = render(ModelDropdown, {
			props: {
				models,
				selectedIndex: 0,
				onSelect: vi.fn(),
				onHighlight: vi.fn(),
			},
		});
		for (const m of models) {
			expect(getByText(m.name)).toBeTruthy();
			expect(getByText(m.provider)).toBeTruthy();
		}
	});

	it('renders an empty list when models is empty', () => {
		const { queryAllByRole } = render(ModelDropdown, {
			props: {
				models: [],
				selectedIndex: 0,
				onSelect: vi.fn(),
				onHighlight: vi.fn(),
			},
		});
		expect(queryAllByRole('option')).toHaveLength(0);
	});
});
