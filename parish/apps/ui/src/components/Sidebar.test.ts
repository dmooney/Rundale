import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import { languageHints, nameHints, uiConfig } from '../stores/game';
import Sidebar from './Sidebar.svelte';
import type { LanguageHint } from '$lib/types';

const IRISH_HINTS: LanguageHint[] = [
	{ word: 'sláinte', pronunciation: 'SLAWN-cha', meaning: 'health / cheers' },
	{ word: 'craic', pronunciation: 'crack', meaning: 'fun, entertainment' }
];

const NAME_HINTS: LanguageHint[] = [
	{ word: 'Aoife', pronunciation: 'EE-fa', meaning: 'beauty, radiance' }
];

function resetStores() {
	languageHints.set([]);
	nameHints.set([]);
	uiConfig.set({
		hints_label: 'Focail (Irish Words)',
		default_accent: '#b08531',
		splash_text: '',
		active_tile_source: '',
		tile_sources: [],
		auto_pause_timeout_seconds: 300
	});
}

describe('Sidebar (desktop branch — no onclose prop)', () => {
	beforeEach(resetStores);

	it('renders the sidebar element', () => {
		const { container } = render(Sidebar);
		expect(container.querySelector('[data-testid="sidebar"]')).toBeTruthy();
	});

	it('shows "No words yet." when both hint stores are empty', () => {
		const { getByText } = render(Sidebar);
		expect(getByText('No words yet.')).toBeTruthy();
	});

	it('renders language hints when present', () => {
		languageHints.set(IRISH_HINTS);
		const { getByText } = render(Sidebar);
		expect(getByText('sláinte')).toBeTruthy();
		expect(getByText('craic')).toBeTruthy();
	});

	it('renders name hints alongside language hints', () => {
		nameHints.set(NAME_HINTS);
		languageHints.set(IRISH_HINTS);
		const { getByText } = render(Sidebar);
		expect(getByText('Aoife')).toBeTruthy();
		expect(getByText('sláinte')).toBeTruthy();
	});

	it('renders hint meanings when present', () => {
		languageHints.set(IRISH_HINTS);
		const { container } = render(Sidebar);
		expect(container.textContent).toContain('health / cheers');
	});

	it('renders pronunciations for all hints', () => {
		languageHints.set(IRISH_HINTS);
		const { container } = render(Sidebar);
		expect(container.textContent).toContain('SLAWN-cha');
		expect(container.textContent).toContain('crack');
	});

	it('reflects the hints_label from uiConfig in the summary', () => {
		uiConfig.update((c) => ({ ...c, hints_label: 'Leideanna Teanga' }));
		const { getByText } = render(Sidebar);
		expect(getByText('Leideanna Teanga')).toBeTruthy();
	});
});

describe('Sidebar (mobile overlay branch — onclose prop provided)', () => {
	beforeEach(resetStores);

	it('renders the focail-panel element instead of a sidebar', () => {
		const { container } = render(Sidebar, { props: { onclose: () => {} } });
		expect(container.querySelector('.focail-panel')).toBeTruthy();
		expect(container.querySelector('[data-testid="sidebar"]')).toBeNull();
	});

	it('shows a close button', () => {
		const { container } = render(Sidebar, { props: { onclose: () => {} } });
		expect(container.querySelector('.close-btn')).toBeTruthy();
	});

	it('shows "No words yet." when both hint stores are empty', () => {
		const { getByText } = render(Sidebar, { props: { onclose: () => {} } });
		expect(getByText('No words yet.')).toBeTruthy();
	});

	it('renders hint list in mobile overlay when hints are present', () => {
		languageHints.set(IRISH_HINTS);
		const { getByText } = render(Sidebar, { props: { onclose: () => {} } });
		expect(getByText('sláinte')).toBeTruthy();
		expect(getByText('craic')).toBeTruthy();
	});

	it('renders name hints in mobile overlay', () => {
		nameHints.set(NAME_HINTS);
		const { getByText } = render(Sidebar, { props: { onclose: () => {} } });
		expect(getByText('Aoife')).toBeTruthy();
	});
});
