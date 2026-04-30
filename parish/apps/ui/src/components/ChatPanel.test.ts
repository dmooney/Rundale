import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { textLog, streamingActive, loadingSpinner, loadingPhrase, loadingColor } from '../stores/game';
import ChatPanel from './ChatPanel.svelte';

// Mock the IPC layer
vi.mock('$lib/ipc', () => ({
	reactToMessage: vi.fn(() => Promise.resolve())
}));

describe('ChatPanel', () => {
	beforeEach(() => {
		textLog.set([]);
		streamingActive.set(false);
		loadingSpinner.set('');
		loadingPhrase.set('');
		loadingColor.set([72, 199, 142]);
	});

	it('renders empty chat panel', () => {
		const { container } = render(ChatPanel);
		expect(container.querySelector('.chat-panel')).toBeTruthy();
	});

	it('renders text log entries', () => {
		textLog.set([
			{ source: 'player', content: 'Hello there' },
			{ source: 'system', content: 'You arrive at the pub.' }
		]);
		const { getByText } = render(ChatPanel);
		expect(getByText('Hello there')).toBeTruthy();
		expect(getByText('You arrive at the pub.')).toBeTruthy();
	});

	it('shows loading phrase when streaming is active with no streaming entry', () => {
		loadingSpinner.set('✛');
		loadingPhrase.set('Consulting the sheep...');
		streamingActive.set(true);
		const { container, getByText } = render(ChatPanel);
		expect(container.querySelector('.loading-row')).toBeTruthy();
		expect(container.querySelector('.triquetra-spinner')).toBeTruthy();
		expect(getByText('Consulting the sheep...')).toBeTruthy();
	});

	it('applies spinner colour from loadingColor store', () => {
		loadingSpinner.set('✜');
		loadingPhrase.set('Pondering the craic...');
		loadingColor.set([255, 200, 87]);
		streamingActive.set(true);
		const { container } = render(ChatPanel);
		const phrase = container.querySelector('.loading-phrase') as HTMLElement;
		expect(phrase.style.color).toBe('rgb(255, 200, 87)');
	});

	it('does not render a streaming cursor', () => {
		textLog.set([{ source: 'Seán', content: 'Dia dhuit…', streaming: true }]);
		const { container } = render(ChatPanel);
		expect(container.querySelector('.cursor')).toBeFalsy();
	});

	it('animates the latest streamed chunk when metadata is present', () => {
		textLog.set([{
			source: 'Seán',
			content: 'Dia dhuit ',
			streaming: true,
			latest_chunk: 'dhuit ',
			stream_chunk_id: 2
		}]);
		const { container } = render(ChatPanel);
		const latestChunk = container.querySelector('.stream-chunk');
		expect(latestChunk).toBeTruthy();
		expect(latestChunk?.textContent).toBe('dhuit');
		expect(container.querySelector('.content')?.textContent).toBe('Dia dhuit ');
	});

	it('player source shows You label', () => {
		textLog.set([{ source: 'player', content: 'Go north' }]);
		const { getByText } = render(ChatPanel);
		expect(getByText('You')).toBeTruthy();
	});

	it('npc source shows name label', () => {
		textLog.set([{ source: 'Máire', content: 'Conas atá tú?' }]);
		const { getByText } = render(ChatPanel);
		expect(getByText('Máire')).toBeTruthy();
	});

	it('player bubble is right-aligned', () => {
		textLog.set([{ source: 'player', content: 'Hello' }]);
		const { container } = render(ChatPanel);
		expect(container.querySelector('.bubble-row.player')).toBeTruthy();
	});

	it('npc bubble is left-aligned', () => {
		textLog.set([{ source: 'Seán', content: 'Dia dhuit' }]);
		const { container } = render(ChatPanel);
		expect(container.querySelector('.bubble-row.npc')).toBeTruthy();
	});

	it('system messages have no bubble', () => {
		textLog.set([{ source: 'system', content: 'You look around.' }]);
		const { container } = render(ChatPanel);
		expect(container.querySelector('.bubble-row')).toBeFalsy();
		expect(container.querySelector('.entry.system')).toBeTruthy();
	});

	describe('emote rendering', () => {
		it('renders *action* text with emote class', () => {
			textLog.set([{ source: 'player', content: '*waves*' }]);
			const { container } = render(ChatPanel);
			const emote = container.querySelector('.emote');
			expect(emote).toBeTruthy();
			expect(emote?.textContent).toBe('waves');
		});

		it('renders mixed text and emotes', () => {
			textLog.set([{ source: 'Padraig', content: 'Hello *smiles warmly* how are ye?' }]);
			const { container } = render(ChatPanel);
			const emotes = container.querySelectorAll('.emote');
			expect(emotes.length).toBe(1);
			expect(emotes[0].textContent).toBe('smiles warmly');
			// Normal text should also be present
			const content = container.querySelector('.content');
			expect(content?.textContent).toContain('Hello');
			expect(content?.textContent).toContain('how are ye?');
		});

		it('renders text without asterisks normally', () => {
			textLog.set([{ source: 'player', content: 'Just plain text' }]);
			const { container } = render(ChatPanel);
			expect(container.querySelector('.emote')).toBeFalsy();
			expect(container.querySelector('.content')?.textContent).toContain('Just plain text');
		});

		it('renders unmatched asterisks as normal text', () => {
			textLog.set([{ source: 'player', content: 'I think *this is incomplete' }]);
			const { container } = render(ChatPanel);
			expect(container.querySelector('.emote')).toBeFalsy();
		});

		it('renders emotes in system messages too', () => {
			textLog.set([{ source: 'system', content: 'You *tip your hat* to the barman.' }]);
			const { container } = render(ChatPanel);
			const emote = container.querySelector('.emote');
			expect(emote).toBeTruthy();
			expect(emote?.textContent).toBe('tip your hat');
		});
	});

	describe('emoji reactions', () => {
		it('shows reaction picker on NPC message hover', async () => {
			textLog.set([{ id: 'msg-1', source: 'Padraig', content: 'Good morning!' }]);
			const { container } = render(ChatPanel);

			const anchor = container.querySelector('.bubble-row.npc .bubble-anchor') as HTMLElement;
			expect(anchor).toBeTruthy();

			await fireEvent.mouseEnter(anchor);
			expect(container.querySelector('[data-testid="reaction-picker"]')).toBeTruthy();
		});

		it('hides reaction picker on mouseleave', async () => {
			textLog.set([{ id: 'msg-1', source: 'Padraig', content: 'Good morning!' }]);
			const { container } = render(ChatPanel);

			const anchor = container.querySelector('.bubble-row.npc .bubble-anchor') as HTMLElement;
			await fireEvent.mouseEnter(anchor);
			expect(container.querySelector('[data-testid="reaction-picker"]')).toBeTruthy();

			await fireEvent.mouseLeave(anchor);
			expect(container.querySelector('[data-testid="reaction-picker"]')).toBeFalsy();
		});

		it('does not show reaction picker on player messages', async () => {
			textLog.set([{ id: 'msg-1', source: 'player', content: 'Hello' }]);
			const { container } = render(ChatPanel);

			const anchor = container.querySelector('.bubble-row.player .bubble-anchor') as HTMLElement;
			await fireEvent.mouseEnter(anchor);
			expect(container.querySelector('[data-testid="reaction-picker"]')).toBeFalsy();
		});

		it('does not show reaction picker on streaming messages', async () => {
			textLog.set([{ id: 'msg-1', source: 'Padraig', content: 'Hello...', streaming: true }]);
			const { container } = render(ChatPanel);

			const anchor = container.querySelector('.bubble-row.npc .bubble-anchor') as HTMLElement;
			await fireEvent.mouseEnter(anchor);
			expect(container.querySelector('[data-testid="reaction-picker"]')).toBeFalsy();
		});

		it('does not show reaction picker on messages without id', async () => {
			textLog.set([{ source: 'Padraig', content: 'Hello' }]);
			const { container } = render(ChatPanel);

			const anchor = container.querySelector('.bubble-row.npc .bubble-anchor') as HTMLElement;
			await fireEvent.mouseEnter(anchor);
			expect(container.querySelector('[data-testid="reaction-picker"]')).toBeFalsy();
		});

		it('clicking reaction adds to entry reactions and calls IPC', async () => {
			const { reactToMessage } = await import('$lib/ipc');
			textLog.set([{ id: 'msg-1', source: 'Padraig', content: 'The rent was raised.' }]);
			const { container } = render(ChatPanel);

			// Hover to show picker
			const anchor = container.querySelector('.bubble-row.npc .bubble-anchor') as HTMLElement;
			await fireEvent.mouseEnter(anchor);

			// Click the first reaction button (😊)
			const buttons = container.querySelectorAll('.reaction-btn');
			expect(buttons.length).toBe(12);
			await fireEvent.click(buttons[0]);

			// Reaction bar should appear with the badge
			expect(container.querySelector('[data-testid="reaction-bar"]')).toBeTruthy();
			const badge = container.querySelector('.reaction-badge');
			expect(badge?.textContent).toContain('😊');

			// IPC should be called
			expect(reactToMessage).toHaveBeenCalledWith('Padraig', 'The rent was raised.', '😊');
		});

		it('renders reaction bar when entry has reactions', () => {
			textLog.set([{
				id: 'msg-1',
				source: 'Padraig',
				content: 'Hello',
				reactions: [
					{ emoji: '😊', source: 'player' },
					{ emoji: '😂', source: 'Siobhan' }
				]
			}]);
			const { container } = render(ChatPanel);

			const bar = container.querySelector('[data-testid="reaction-bar"]');
			expect(bar).toBeTruthy();

			const badges = container.querySelectorAll('.reaction-badge');
			expect(badges.length).toBe(2);

			// NPC reactions show source name
			const source = container.querySelector('.reaction-source');
			expect(source?.textContent).toBe('Siobhan');
		});

		it('player reactions do not show source label', () => {
			textLog.set([{
				id: 'msg-1',
				source: 'Padraig',
				content: 'Hello',
				reactions: [{ emoji: '😊', source: 'player' }]
			}]);
			const { container } = render(ChatPanel);

			expect(container.querySelector('.reaction-source')).toBeFalsy();
		});
	});
});
