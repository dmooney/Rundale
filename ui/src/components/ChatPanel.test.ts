import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import { textLog, streamingActive } from '../stores/game';
import ChatPanel from './ChatPanel.svelte';

describe('ChatPanel', () => {
	beforeEach(() => {
		textLog.set([]);
		streamingActive.set(false);
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

	it('shows spinner when streaming is active with no streaming entry', () => {
		streamingActive.set(true);
		const { container } = render(ChatPanel);
		expect(container.querySelector('.spinner')).toBeTruthy();
	});

	it('shows blinking cursor on streaming entry', () => {
		textLog.set([{ source: 'Seán', content: 'Dia dhuit…', streaming: true }]);
		const { container } = render(ChatPanel);
		expect(container.querySelector('.cursor')).toBeTruthy();
	});

	it('player source shows You label', () => {
		textLog.set([{ source: 'player', content: 'Go north' }]);
		const { getByText } = render(ChatPanel);
		expect(getByText('You:')).toBeTruthy();
	});

	it('npc source shows name label', () => {
		textLog.set([{ source: 'Máire', content: 'Conas atá tú?' }]);
		const { getByText } = render(ChatPanel);
		expect(getByText('Máire:')).toBeTruthy();
	});
});
