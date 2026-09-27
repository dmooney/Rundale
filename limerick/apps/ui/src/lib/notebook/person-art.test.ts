import { describe, expect, it } from 'vitest';
import {
	loadNotebookPersonArt,
	NOTEBOOK_PERSON_ART_BASE,
	normalizeNotebookPersonName,
	resolveNotebookPersonArt,
	type NotebookPersonArtRuntimeManifest,
} from './person-art';

const manifest: NotebookPersonArtRuntimeManifest = {
	name: 'Rundale notebook person art',
	version: 3,
	assets: {
		personArt: {
			fallback: {
				display_name: 'Unknown parish neighbour',
				portrait: 'people/portrait-unknown-neighbour.png',
				marker: 'people/marker-unknown-neighbour.png',
				approval_status: 'approved',
			},
			people: [
				{
					real_name: 'Brigid Ni Fhatharta',
					display_name: 'Brigid Ni Fhatharta',
					npc_id: 19,
					portrait: 'people/portrait-brigid-ni-fhatharta.png',
					marker: 'people/marker-brigid-ni-fhatharta.png',
					approval_status: 'approved',
				},
				{
					real_name: 'Draft Person',
					display_name: 'Draft Person',
					portrait: 'people/portrait-draft-person.png',
					marker: 'people/marker-draft-person.png',
					approval_status: 'draft',
				},
			],
		},
	},
};

describe('notebook person art runtime', () => {
	it('resolves approved art by numeric identity', () => {
		const art = loadNotebookPersonArt(manifest);
		expect(resolveNotebookPersonArt(art, 19, 'A renamed Brigid')).toEqual({
			displayName: 'Brigid Ni Fhatharta',
			portrait: `${NOTEBOOK_PERSON_ART_BASE}people/portrait-brigid-ni-fhatharta.png`,
			marker: `${NOTEBOOK_PERSON_ART_BASE}people/marker-brigid-ni-fhatharta.png`,
			fallback: false,
		});
	});

	it('falls back for unknown, invalid, or unapproved identities', () => {
		const art = loadNotebookPersonArt(manifest);
		for (const [id, name] of [
			[404, 'Brigid Ni Fhatharta'],
			[-1, 'Brigid Ni Fhatharta'],
			[undefined, 'Draft Person'],
		] as const) {
			expect(resolveNotebookPersonArt(art, id, name).fallback).toBe(true);
		}
	});

	it('uses normalized names only when numeric identity is absent', () => {
		const art = loadNotebookPersonArt(manifest);
		expect(
			resolveNotebookPersonArt(art, undefined, '  brígid   ni   fhatharta  ')
				.fallback,
		).toBe(false);
	});

	it('rejects duplicate IDs and ambiguous compatibility names', () => {
		const duplicate = structuredClone(manifest);
		duplicate.assets?.personArt?.people.push({
			real_name: '  Brigid Ni Fhatharta ',
			display_name: 'Impostor',
			npc_id: 19,
			portrait: 'people/portrait-impostor.png',
			marker: 'people/marker-impostor.png',
			approval_status: 'approved',
		});
		const art = loadNotebookPersonArt(duplicate);
		expect(art.byId.has(19)).toBe(false);
		expect(
			art.byName.has(normalizeNotebookPersonName('Brigid Ni Fhatharta')),
		).toBe(false);
		expect(art.assetUrls).not.toContain(
			`${NOTEBOOK_PERSON_ART_BASE}people/portrait-impostor.png`,
		);
	});

	it('uses production fallback art without a usable manifest', () => {
		const art = loadNotebookPersonArt(null);
		expect(resolveNotebookPersonArt(art, 19, 'Brigid').fallback).toBe(true);
		expect(art.assetUrls).toEqual([
			`${NOTEBOOK_PERSON_ART_BASE}people/portrait-unknown-neighbour.png`,
			`${NOTEBOOK_PERSON_ART_BASE}people/marker-unknown-neighbour.png`,
		]);
	});
});
