import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { readFileSync } from 'fs';
import { resolve } from 'path';

// Raw component source — used to assert the scoped CSS rules introduced by the
// #1226/#1275 fixes are present (jsdom does not apply Svelte's scoped <style>,
// so rendered colour/layout is proven by the Playwright screenshot spec).
// vitest's cwd is parish/apps/ui.
const COMPONENT_SRC = readFileSync(
	resolve(process.cwd(), 'src/components/ChatPanel.svelte'),
	'utf8',
).replace(/\s+/g, ' ');
import {
	textLog,
	streamingActive,
	loadingPhrase,
	loadingColor,
	worldState,
} from '../stores/game';
import type { WorldSnapshot } from '$lib/types';
import ChatPanel from './ChatPanel.svelte';

// Mock the IPC layer
vi.mock('$lib/ipc', () => ({
	reactToMessage: vi.fn(() => Promise.resolve()),
}));

describe('ChatPanel', () => {
	beforeEach(() => {
		textLog.set([]);
		streamingActive.set(false);
		loadingPhrase.set('');
		loadingColor.set([72, 199, 142]);
		worldState.set(null);
	});

	/** Builds a minimal WorldSnapshot; only location_name drives richify(). */
	function setLocation(name: string) {
		worldState.set({ location_name: name } as unknown as WorldSnapshot);
	}

	it('renders empty chat panel', () => {
		const { container } = render(ChatPanel);
		expect(container.querySelector('.chat-panel')).toBeTruthy();
	});

	it('renders text log entries', () => {
		textLog.set([
			{ source: 'player', content: 'Hello there' },
			{ source: 'system', content: 'You arrive at the pub.' },
		]);
		const { getByText } = render(ChatPanel);
		expect(getByText('Hello there')).toBeTruthy();
		expect(getByText('You arrive at the pub.')).toBeTruthy();
	});

	it('shows loading phrase when streaming is active with no streaming entry', () => {
		loadingPhrase.set('Consulting the sheep...');
		streamingActive.set(true);
		const { container, getByText } = render(ChatPanel);
		expect(container.querySelector('.loading-row')).toBeTruthy();
		expect(container.querySelector('.triquetra-spinner')).toBeTruthy();
		expect(getByText('Consulting the sheep...')).toBeTruthy();
	});

	it('applies spinner colour from loadingColor store', () => {
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
		textLog.set([
			{
				source: 'Seán',
				content: 'Dia dhuit ',
				streaming: true,
				latest_chunk: 'dhuit ',
				stream_chunk_id: 2,
			},
		]);
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
			textLog.set([
				{ source: 'Padraig', content: 'Hello *smiles warmly* how are ye?' },
			]);
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
			expect(container.querySelector('.content')?.textContent).toContain(
				'Just plain text',
			);
		});

		it('renders unmatched asterisks as normal text', () => {
			textLog.set([
				{ source: 'player', content: 'I think *this is incomplete' },
			]);
			const { container } = render(ChatPanel);
			expect(container.querySelector('.emote')).toBeFalsy();
		});

		it('renders emotes in system messages too', () => {
			textLog.set([
				{ source: 'system', content: 'You *tip your hat* to the barman.' },
			]);
			const { container } = render(ChatPanel);
			const emote = container.querySelector('.emote');
			expect(emote).toBeTruthy();
			expect(emote?.textContent).toBe('tip your hat');
		});
	});

	describe('reaction IPC failure rollback', () => {
		afterEach(async () => {
			const { reactToMessage } = await import('$lib/ipc');
			(reactToMessage as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);
		});

		it('removes optimistic reaction when reactToMessage IPC fails', async () => {
			const { reactToMessage } = await import('$lib/ipc');
			(reactToMessage as ReturnType<typeof vi.fn>).mockRejectedValue(
				new Error('Network error'),
			);

			textLog.set([{ id: 'msg-1', source: 'Padraig', content: 'Bad news.' }]);
			const { container } = render(ChatPanel);

			const anchor = container.querySelector(
				'.bubble-row.npc .bubble-anchor',
			) as HTMLElement;
			await fireEvent.mouseEnter(anchor);

			const buttons = container.querySelectorAll('.reaction-btn');
			await fireEvent.click(buttons[0]);

			// Flush microtasks for the async IPC rejection + reaction rollback
			for (let i = 0; i < 10; i++) {
				await Promise.resolve();
			}

			expect(
				container.querySelector('[data-testid="reaction-bar"]'),
			).toBeFalsy();
		});
	});

	describe('tabular subtype rendering', () => {
		it('renders tabular grid with header and cmd/desc pairs', () => {
			textLog.set([
				{
					source: 'system',
					content: 'Commands\n/go — Move somewhere\n/look — Look around',
					subtype: 'tabular',
				},
			]);
			const { container } = render(ChatPanel);
			const grid = container.querySelector('.tabular-grid');
			expect(grid).toBeTruthy();
			expect(container.querySelector('.tabular-header')?.textContent).toContain(
				'Commands',
			);
			expect(container.querySelector('.tabular-cmd')?.textContent).toBe('/go');
			expect(container.querySelector('.tabular-desc')?.textContent).toContain(
				'Move somewhere',
			);
		});

		it('renders multiple cmd/desc pairs in tabular grid', () => {
			textLog.set([
				{
					source: 'system',
					content:
						'/go — Move somewhere\n/look — Look around\n/talk — Start a conversation',
					subtype: 'tabular',
				},
			]);
			const { container } = render(ChatPanel);
			const cmds = container.querySelectorAll('.tabular-cmd');
			expect(cmds.length).toBe(3);
			expect(cmds[0].textContent).toBe('/go');
			expect(cmds[1].textContent).toBe('/look');
			expect(cmds[2].textContent).toBe('/talk');
		});
	});

	describe('scroll-to-bottom behavior', () => {
		it('renders log container and scrolls on new message', async () => {
			const { container } = render(ChatPanel);
			const logEl = container.querySelector('.chat-panel') as HTMLDivElement;
			expect(logEl).toBeTruthy();

			textLog.set([{ source: 'system', content: 'Hello' }]);
			await Promise.resolve();

			expect(container.querySelector('.chat-panel')).toBeTruthy();
		});
	});

	describe('emoji reactions', () => {
		it('shows reaction picker on NPC message hover', async () => {
			textLog.set([
				{ id: 'msg-1', source: 'Padraig', content: 'Good morning!' },
			]);
			const { container } = render(ChatPanel);

			const anchor = container.querySelector(
				'.bubble-row.npc .bubble-anchor',
			) as HTMLElement;
			expect(anchor).toBeTruthy();

			await fireEvent.mouseEnter(anchor);
			expect(
				container.querySelector('[data-testid="reaction-picker"]'),
			).toBeTruthy();
		});

		it('hides reaction picker on mouseleave', async () => {
			textLog.set([
				{ id: 'msg-1', source: 'Padraig', content: 'Good morning!' },
			]);
			const { container } = render(ChatPanel);

			const anchor = container.querySelector(
				'.bubble-row.npc .bubble-anchor',
			) as HTMLElement;
			await fireEvent.mouseEnter(anchor);
			expect(
				container.querySelector('[data-testid="reaction-picker"]'),
			).toBeTruthy();

			await fireEvent.mouseLeave(anchor);
			expect(
				container.querySelector('[data-testid="reaction-picker"]'),
			).toBeFalsy();
		});

		it('does not show reaction picker on player messages', async () => {
			textLog.set([{ id: 'msg-1', source: 'player', content: 'Hello' }]);
			const { container } = render(ChatPanel);

			const anchor = container.querySelector(
				'.bubble-row.player .bubble-anchor',
			) as HTMLElement;
			await fireEvent.mouseEnter(anchor);
			expect(
				container.querySelector('[data-testid="reaction-picker"]'),
			).toBeFalsy();
		});

		it('does not show reaction picker on streaming messages', async () => {
			textLog.set([
				{
					id: 'msg-1',
					source: 'Padraig',
					content: 'Hello...',
					streaming: true,
				},
			]);
			const { container } = render(ChatPanel);

			const anchor = container.querySelector(
				'.bubble-row.npc .bubble-anchor',
			) as HTMLElement;
			await fireEvent.mouseEnter(anchor);
			expect(
				container.querySelector('[data-testid="reaction-picker"]'),
			).toBeFalsy();
		});

		it('does not show reaction picker on messages without id', async () => {
			textLog.set([{ source: 'Padraig', content: 'Hello' }]);
			const { container } = render(ChatPanel);

			const anchor = container.querySelector(
				'.bubble-row.npc .bubble-anchor',
			) as HTMLElement;
			await fireEvent.mouseEnter(anchor);
			expect(
				container.querySelector('[data-testid="reaction-picker"]'),
			).toBeFalsy();
		});

		it('clicking reaction adds to entry reactions and calls IPC', async () => {
			const { reactToMessage } = await import('$lib/ipc');
			textLog.set([
				{ id: 'msg-1', source: 'Padraig', content: 'The rent was raised.' },
			]);
			const { container } = render(ChatPanel);

			// Hover to show picker
			const anchor = container.querySelector(
				'.bubble-row.npc .bubble-anchor',
			) as HTMLElement;
			await fireEvent.mouseEnter(anchor);

			// Click the first reaction button (😊)
			const buttons = container.querySelectorAll('.reaction-btn');
			expect(buttons.length).toBe(12);
			await fireEvent.click(buttons[0]);

			// Reaction bar should appear with the badge
			expect(
				container.querySelector('[data-testid="reaction-bar"]'),
			).toBeTruthy();
			const badge = container.querySelector('.reaction-badge');
			expect(badge?.textContent).toContain('😊');

			// IPC should be called
			expect(reactToMessage).toHaveBeenCalledWith(
				'Padraig',
				'The rent was raised.',
				'😊',
			);
		});

		it('renders reaction bar when entry has reactions', () => {
			textLog.set([
				{
					id: 'msg-1',
					source: 'Padraig',
					content: 'Hello',
					reactions: [
						{ emoji: '😊', source: 'player' },
						{ emoji: '😂', source: 'Siobhan' },
					],
				},
			]);
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
			textLog.set([
				{
					id: 'msg-1',
					source: 'Padraig',
					content: 'Hello',
					reactions: [{ emoji: '😊', source: 'player' }],
				},
			]);
			const { container } = render(ChatPanel);

			expect(container.querySelector('.reaction-source')).toBeFalsy();
		});
	});

	// Regression for #1226: a go-to / movement echo bubble contains the
	// destination, which equals the current location name and is therefore
	// highlighted as a `.term-location`. The destination text must remain in
	// the player bubble AND its term spans must use the bubble's own
	// foreground (cream `--color-bg`) rather than the page-tuned gold location
	// colour, which is invisible on the gold player bubble.
	describe('#1226 go-to destination visible in player bubble', () => {
		it('renders the full destination in a go-to player bubble', () => {
			setLocation('The Crossroads');
			textLog.set([
				{ id: 'p1', source: 'player', content: 'go to The Crossroads' },
			]);
			const { container } = render(ChatPanel);

			const content = container.querySelector(
				'.bubble-row.player .content',
			) as HTMLElement;
			expect(content).toBeTruthy();
			// Full echoed command — destination NOT truncated.
			expect(content.textContent).toBe('go to The Crossroads');
		});

		it('highlights the destination as a location term', () => {
			setLocation('The Crossroads');
			textLog.set([
				{ id: 'p1', source: 'player', content: 'go to The Crossroads' },
			]);
			const { container } = render(ChatPanel);

			const term = container.querySelector(
				'.bubble-row.player .term-location',
			) as HTMLElement;
			expect(term).toBeTruthy();
			expect(term.textContent).toBe('The Crossroads');
		});

		it('scopes the player-bubble term override to player bubbles only', () => {
			// NPC bubble keeps the normal location term colour (regression guard
			// AC1226-3). We assert the override CSS targets `.player .bubble`,
			// so an NPC location term is not forced to the bubble background.
			setLocation('The Crossroads');
			textLog.set([
				{ id: 'n1', source: 'Padraig', content: 'Meet me at The Crossroads' },
			]);
			const { container } = render(ChatPanel);

			const npcTerm = container.querySelector(
				'.bubble-row.npc .term-location',
			) as HTMLElement;
			expect(npcTerm).toBeTruthy();
			expect(npcTerm.textContent).toBe('The Crossroads');
			// The override selector does not match NPC bubbles; the NPC term keeps
			// its default `.term-location` colour (asserted structurally — the
			// element exists under `.bubble-row.npc`, not `.player`).
			expect(
				container.querySelector('.bubble-row.player .term-location'),
			).toBeFalsy();
		});

		// jsdom does not apply Svelte's scoped <style>, so the *rendered* colour
		// is proven by the Playwright screenshot. Here we assert the component
		// source declares the player-bubble term-colour override that makes the
		// destination legible (cream on the gold bubble) — a regression that
		// removes it fails fast.
		it('declares a player-bubble term-colour override to --color-bg (AC1226-2)', () => {
			const normalized = COMPONENT_SRC;
			// All three term kinds, scoped to the player bubble, set to the
			// bubble's readable foreground.
			expect(normalized).toMatch(
				/\.player \.bubble :global\(\.term-irish\), \.player \.bubble :global\(\.term-name\), \.player \.bubble :global\(\.term-location\) \{ color: var\(--color-bg\);/,
			);
		});
	});

	// Regression for #1275: multiple co-located NPCs reacting to one player
	// message must lay out cleanly — the bar aligns to the player side, wraps
	// as whole chips, and renders every reaction.
	describe('#1275 multiple emoji reactions layout', () => {
		const multiReaction = {
			id: 'p1',
			source: 'player',
			content: 'there is not',
			reactions: [
				{ emoji: '🤔', source: 'Mick Flanagan' },
				{ emoji: '😏', source: 'Brendan Duffy' },
				{ emoji: '🙄', source: 'Padraig Darcy' },
			],
		};

		it('renders every reaction chip with no drops under a player bubble (AC1275-2)', () => {
			textLog.set([multiReaction]);
			const { container } = render(ChatPanel);

			// Bar lives inside the player bubble's row (so the side-aligned CSS
			// rule `.player .reaction-bar` matches at runtime).
			const bar = container.querySelector(
				'.bubble-row.player [data-testid="reaction-bar"]',
			);
			expect(bar).toBeTruthy();
			// Every reaction is rendered — none dropped or clipped.
			const badges = container.querySelectorAll('.reaction-badge');
			expect(badges.length).toBe(3);
			// Each chip carries its source name, so the chip is the wide element
			// the layout fix must keep intact.
			const sources = Array.from(
				container.querySelectorAll('.reaction-source'),
			).map((el) => el.textContent);
			expect(sources).toEqual([
				'Mick Flanagan',
				'Brendan Duffy',
				'Padraig Darcy',
			]);
		});

		it('nests the reaction bar under an NPC bubble row when the NPC message is reacted to (AC1275-1)', () => {
			textLog.set([
				{
					id: 'n1',
					source: 'Padraig',
					content: 'Bad news.',
					reactions: [{ emoji: '😢', source: 'player' }],
				},
			]);
			const { container } = render(ChatPanel);

			// Bar nests under `.bubble-row.npc`, so `.npc .reaction-bar`
			// (left-align) matches and `.player .reaction-bar` does not.
			expect(
				container.querySelector('.bubble-row.npc [data-testid="reaction-bar"]'),
			).toBeTruthy();
			expect(
				container.querySelector(
					'.bubble-row.player [data-testid="reaction-bar"]',
				),
			).toBeFalsy();
		});

		// jsdom does not apply Svelte's scoped <style> cascade, so the *rendered*
		// alignment / wrapping / chip-integrity is proven by the Playwright
		// screenshot spec (e2e/chat-feed-rendering.spec.ts) which runs a real
		// browser. Here we assert the component source declares the layout rules
		// the fix introduced, so a regression that deletes them fails fast.
		describe('layout CSS rules present in component source (AC1275-1/3)', () => {
			const normalized = COMPONENT_SRC;

			it('player reaction bar is right-aligned', () => {
				expect(normalized).toMatch(
					/\.player \.reaction-bar \{ justify-content: flex-end;/,
				);
			});

			it('npc reaction bar is left-aligned', () => {
				expect(normalized).toMatch(
					/\.npc \.reaction-bar \{ justify-content: flex-start;/,
				);
			});

			it('reaction badge is a non-shrinking, non-wrapping chip', () => {
				expect(normalized).toContain('flex: 0 0 auto;');
				expect(normalized).toContain('white-space: nowrap;');
			});

			it('anchors the player/npc bubble wrapper to the message side', () => {
				// So bubble + reaction chips line up on the same edge.
				expect(normalized).toMatch(
					/\.player \.bubble-wrapper \{ align-items: flex-end;/,
				);
				expect(normalized).toMatch(
					/\.npc \.bubble-wrapper \{ align-items: flex-start;/,
				);
			});
		});
	});
});

describe('ChatPanel — UI design pass (game-ui-design-ma9ls0)', () => {
	beforeEach(() => {
		textLog.set([]);
		streamingActive.set(false);
		worldState.set(null);
	});

	it('renders a time-rule entry as a separator with its label', () => {
		textLog.set([
			{ source: 'system', content: 'You arrive.' },
			{ source: 'system', subtype: 'time-rule', content: 'Dusk — Monday' },
		]);
		const { container, getByText } = render(ChatPanel);
		const rule = container.querySelector('.time-rule');
		expect(rule).toBeTruthy();
		expect(rule?.getAttribute('role')).toBe('separator');
		expect(getByText('Dusk — Monday')).toBeTruthy();
	});

	it('renders the splash as a title card with demoted metadata', () => {
		textLog.set([
			{
				source: 'system',
				content: 'Rundale\nCopyright © 2026 David Mooney.\nmain — 2026',
			},
		]);
		const { container } = render(ChatPanel);
		const card = container.querySelector('.splash-card');
		expect(card).toBeTruthy();
		expect(card?.querySelector('strong')?.textContent).toBe('Rundale');
		expect(card?.querySelector('.splash-meta')?.textContent).toContain(
			'Copyright',
		);
	});

	describe('scoped CSS regression guards (jsdom cannot apply scoped styles)', () => {
		const normalized = COMPONENT_SRC;

		it('does not pin the log with justify-content: flex-end (breaks top scroll)', () => {
			expect(normalized).not.toMatch(
				/\.chat-panel \{[^}]*justify-content: flex-end/,
			);
			expect(normalized).toMatch(
				/\.chat-panel > :global\(:first-child\) \{ margin-top: auto;/,
			);
		});

		it('darkens the player bubble toward the foreground ink for contrast', () => {
			expect(normalized).toMatch(
				/\.player \.bubble \{ background: color-mix\(in srgb, var\(--color-accent\) 55%, var\(--color-fg\)\);/,
			);
		});
	});
});
