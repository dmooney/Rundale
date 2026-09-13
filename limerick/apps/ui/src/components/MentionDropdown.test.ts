import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import MentionDropdown from './MentionDropdown.svelte';
import type { NpcInfo } from '$lib/types';

function makeNpc(name: string, introduced: boolean): NpcInfo {
	return {
		npc_id: name.length,
		name,
		real_name: name,
		occupation: `${name}'s occupation`,
		mood: 'neutral',
		introduced,
		mood_emoji: '😐',
	};
}

const npcs: NpcInfo[] = [
	makeNpc('Brigid', true),
	makeNpc('Seamus', false),
	makeNpc('Aoife', true),
];

describe('MentionDropdown', () => {
	it('renders a listbox with the correct aria-label', () => {
		const { getByRole } = render(MentionDropdown, {
			props: {
				npcs,
				selectedIndex: 0,
				onSelect: vi.fn(),
				onHighlight: vi.fn(),
			},
		});
		const list = getByRole('listbox');
		expect(list).toBeTruthy();
		expect(list.getAttribute('aria-label')).toBe('Mention NPC');
	});

	it('renders one option per NPC', () => {
		const { getAllByRole } = render(MentionDropdown, {
			props: {
				npcs,
				selectedIndex: 0,
				onSelect: vi.fn(),
				onHighlight: vi.fn(),
			},
		});
		expect(getAllByRole('option')).toHaveLength(npcs.length);
	});

	it('sets aria-selected=true on the item at selectedIndex and false on others', () => {
		const { getAllByRole } = render(MentionDropdown, {
			props: {
				npcs,
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

	it('assigns id="mention-option-{i}" to each item', () => {
		const { getAllByRole } = render(MentionDropdown, {
			props: {
				npcs,
				selectedIndex: 0,
				onSelect: vi.fn(),
				onHighlight: vi.fn(),
			},
		});
		const options = getAllByRole('option');
		options.forEach((opt, i) => {
			expect(opt.id).toBe(`mention-option-${i}`);
		});
	});

	it('shows occupation detail when introduced is true', () => {
		const { getByText } = render(MentionDropdown, {
			props: {
				npcs,
				selectedIndex: 0,
				onSelect: vi.fn(),
				onHighlight: vi.fn(),
			},
		});
		// Brigid is introduced — occupation should be visible
		expect(getByText("Brigid's occupation")).toBeTruthy();
	});

	it('hides occupation detail when introduced is false', () => {
		const { queryByText } = render(MentionDropdown, {
			props: {
				npcs,
				selectedIndex: 0,
				onSelect: vi.fn(),
				onHighlight: vi.fn(),
			},
		});
		// Seamus is not introduced — occupation must not appear
		expect(queryByText("Seamus's occupation")).toBeNull();
	});

	it('renders each NPC name', () => {
		const { getByText } = render(MentionDropdown, {
			props: {
				npcs,
				selectedIndex: 0,
				onSelect: vi.fn(),
				onHighlight: vi.fn(),
			},
		});
		for (const npc of npcs) {
			expect(getByText(npc.name)).toBeTruthy();
		}
	});
});
