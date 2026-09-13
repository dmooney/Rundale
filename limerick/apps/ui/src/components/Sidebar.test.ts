import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import { tick } from 'svelte';
import { languageHints, nameHints, uiConfig, npcsHere } from '../stores/game';
import Sidebar from './Sidebar.svelte';
import type { LanguageHint, NpcInfo } from '$lib/types';

const IRISH_HINTS: LanguageHint[] = [
	{ word: 'sláinte', pronunciation: 'SLAWN-cha', meaning: 'health / cheers' },
	{ word: 'craic', pronunciation: 'crack', meaning: 'fun, entertainment' },
];

const NAME_HINTS: LanguageHint[] = [
	{ word: 'Aoife', pronunciation: 'EE-fa', meaning: 'beauty, radiance' },
];

const NPCS: NpcInfo[] = [
	{
		npc_id: 1,
		name: 'Bridget Kelly',
		real_name: 'Bridget Kelly',
		occupation: 'Weaver',
		mood: 'cheerful',
		introduced: true,
		mood_emoji: '😊',
	},
	{
		npc_id: 2,
		name: 'a stern priest',
		real_name: 'Fr. Tiernan',
		occupation: 'Priest',
		mood: 'stern',
		introduced: false,
		mood_emoji: '😠',
	},
];

function resetStores() {
	languageHints.set([]);
	nameHints.set([]);
	npcsHere.set([]);
	uiConfig.set({
		hints_label: 'Focail (Irish Words)',
		default_accent: '#b08531',
		splash_text: '',
		active_tile_source: '',
		tile_sources: [],
		auto_pause_timeout_seconds: 300,
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

	it('shows a quiet empty state in the Present section when no NPCs are here', () => {
		const { getByText, queryByTestId } = render(Sidebar);
		expect(getByText('Present')).toBeTruthy();
		expect(getByText('No one is about.')).toBeTruthy();
		expect(queryByTestId('npcs-present')).toBeNull();
	});

	it('lists NPCs at the player location with name and occupation', () => {
		npcsHere.set(NPCS);
		const { getByTestId, getByText } = render(Sidebar);
		expect(getByTestId('npcs-present')).toBeTruthy();
		expect(getByText('Bridget Kelly')).toBeTruthy();
		expect(getByText('Weaver')).toBeTruthy();
	});

	it('renders approved portrait art through ordinary DOM images', () => {
		npcsHere.set([
			{
				...NPCS[0],
				name: 'Padraig Darcy',
				real_name: 'Padraig Darcy',
			},
		]);
		const { container } = render(Sidebar);
		const portrait = container.querySelector(
			'img.npc-portrait',
		) as HTMLImageElement;
		expect(portrait.src).toContain(
			'/rundale/notebook-ui/people/portrait-padraig-darcy.png',
		);
		expect(container.querySelector('canvas')).toBeNull();
	});

	it('hides the occupation for unintroduced NPCs', () => {
		npcsHere.set(NPCS);
		const { getByText, queryByText } = render(Sidebar);
		expect(getByText('a stern priest')).toBeTruthy();
		expect(queryByText('Priest')).toBeNull();
	});

	it('reveals canonical identity when the authoritative NPC state changes', async () => {
		const hidden = NPCS[1];
		npcsHere.set([hidden]);
		const { getByText, queryByText } = render(Sidebar);
		expect(getByText('a stern priest')).toBeTruthy();
		expect(queryByText('Fr. Tiernan')).toBeNull();
		expect(queryByText('Priest')).toBeNull();

		npcsHere.set([
			{
				...hidden,
				name: hidden.real_name,
				introduced: true,
			},
		]);
		await tick();

		expect(getByText('Fr. Tiernan')).toBeTruthy();
		expect(getByText('Priest')).toBeTruthy();
		expect(queryByText('a stern priest')).toBeNull();
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

	it('keeps nearby people reachable in the mobile panel', () => {
		npcsHere.set(NPCS);
		const { getByText, getByTestId } = render(Sidebar, {
			props: { onclose: () => {} },
		});
		expect(getByText('Present')).toBeTruthy();
		expect(getByTestId('npcs-present')).toBeTruthy();
		expect(getByText('Bridget Kelly')).toBeTruthy();
	});

	it('shows an authoritative identity reveal in the mobile panel', async () => {
		const hidden = NPCS[1];
		npcsHere.set([hidden]);
		const { getByText, queryByText } = render(Sidebar, {
			props: { onclose: () => {} },
		});
		expect(getByText('a stern priest')).toBeTruthy();
		expect(queryByText('Priest')).toBeNull();

		npcsHere.set([{ ...hidden, name: hidden.real_name, introduced: true }]);
		await tick();

		expect(getByText('Fr. Tiernan')).toBeTruthy();
		expect(getByText('Priest')).toBeTruthy();
	});

	it('renders name hints in mobile overlay', () => {
		nameHints.set(NAME_HINTS);
		const { getByText } = render(Sidebar, { props: { onclose: () => {} } });
		expect(getByText('Aoife')).toBeTruthy();
	});
});
