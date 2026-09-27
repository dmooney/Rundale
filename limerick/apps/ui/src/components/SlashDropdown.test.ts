import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import SlashDropdown from './SlashDropdown.svelte';
import type { SlashCommand } from '$lib/slash-commands';

const commands: SlashCommand[] = [
	{ command: '/help', description: 'Show available commands', hasArgs: false },
	{ command: '/save', description: 'Save game', hasArgs: false },
	{ command: '/load', description: 'Load a saved branch', hasArgs: true },
];

describe('SlashDropdown', () => {
	it('renders a listbox with the correct aria-label', () => {
		const { getByRole } = render(SlashDropdown, {
			props: {
				commands,
				selectedIndex: 0,
				onSelect: vi.fn(),
				onHighlight: vi.fn(),
			},
		});
		const list = getByRole('listbox');
		expect(list.getAttribute('aria-label')).toBe('Slash commands');
	});

	it('renders one option per command', () => {
		const { getAllByRole } = render(SlashDropdown, {
			props: {
				commands,
				selectedIndex: 0,
				onSelect: vi.fn(),
				onHighlight: vi.fn(),
			},
		});
		expect(getAllByRole('option')).toHaveLength(commands.length);
	});

	it('sets aria-selected=true only on the selected index', () => {
		const { getAllByRole } = render(SlashDropdown, {
			props: {
				commands,
				selectedIndex: 1,
				onSelect: vi.fn(),
				onHighlight: vi.fn(),
			},
		});
		const options = getAllByRole('option');
		expect(options[0].getAttribute('aria-selected')).toBe('false');
		expect(options[1].getAttribute('aria-selected')).toBe('true');
		expect(options[2].getAttribute('aria-selected')).toBe('false');
	});

	it('assigns id="slash-option-{i}" to each item', () => {
		const { getAllByRole } = render(SlashDropdown, {
			props: {
				commands,
				selectedIndex: 0,
				onSelect: vi.fn(),
				onHighlight: vi.fn(),
			},
		});
		getAllByRole('option').forEach((opt, i) => {
			expect(opt.id).toBe(`slash-option-${i}`);
		});
	});

	it('renders both command name and description for each entry', () => {
		const { getByText } = render(SlashDropdown, {
			props: {
				commands,
				selectedIndex: 0,
				onSelect: vi.fn(),
				onHighlight: vi.fn(),
			},
		});
		for (const cmd of commands) {
			expect(getByText(cmd.command)).toBeTruthy();
			expect(getByText(cmd.description)).toBeTruthy();
		}
	});

	it('renders an empty listbox when commands is empty', () => {
		const { queryAllByRole } = render(SlashDropdown, {
			props: {
				commands: [],
				selectedIndex: 0,
				onSelect: vi.fn(),
				onHighlight: vi.fn(),
			},
		});
		expect(queryAllByRole('option')).toHaveLength(0);
	});
});
