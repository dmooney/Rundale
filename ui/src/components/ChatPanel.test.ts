import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import { textLog, streamingActive, loadingSpinner, loadingPhrase, loadingColor } from '../stores/game';
import ChatPanel from './ChatPanel.svelte';

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
		expect(container.querySelector('.loading-spinner')).toBeTruthy();
		expect(getByText('Consulting the sheep...')).toBeTruthy();
	});

	it('applies spinner colour from loadingColor store', () => {
		loadingSpinner.set('✜');
		loadingPhrase.set('Pondering the craic...');
		loadingColor.set([255, 200, 87]);
		streamingActive.set(true);
		const { container } = render(ChatPanel);
		const spinner = container.querySelector('.loading-spinner') as HTMLElement;
		expect(spinner.style.color).toBe('rgb(255, 200, 87)');
	});

	it('shows blinking cursor on streaming entry', () => {
		textLog.set([{ source: 'Seán', content: 'Dia dhuit…', streaming: true }]);
		const { container } = render(ChatPanel);
		expect(container.querySelector('.cursor')).toBeTruthy();
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
});
