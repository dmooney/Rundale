import { describe, expect, it } from 'vitest';
import {
	loadNotebookPersonArt,
	NOTEBOOK_ASSET_BASE,
	NOTEBOOK_ASSETS,
	resolveNotebookPersonArt,
	type NotebookAssetManifest,
} from './assets';

const manifest: NotebookAssetManifest = {
	name: 'Rundale notebook assets',
	version: 2,
	source: 'test manifest',
	assets: {
		personArt: {
			source_config:
				'parish/apps/ui/art/notebook-person-art/approved-cast-v1.json',
			portrait_prompt:
				'parish/apps/ui/art/notebook-person-art/prompts/portraits-v1.md',
			marker_prompt:
				'parish/apps/ui/art/notebook-person-art/prompts/markers-v1.md',
			contact_sheet: 'person-art-contact-sheet.png',
			contact_sheet_html: 'person-art-contact-sheet.html',
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

describe('illustrated notebook person art assets', () => {
	it('loads approved named person art from the manifest', () => {
		const art = loadNotebookPersonArt(manifest);

		expect(
			resolveNotebookPersonArt(art, '  brígid   ni   fhatharta  '),
		).toEqual({
			displayName: 'Brigid Ni Fhatharta',
			portrait: `${NOTEBOOK_ASSET_BASE}people/portrait-brigid-ni-fhatharta.png`,
			marker: `${NOTEBOOK_ASSET_BASE}people/marker-brigid-ni-fhatharta.png`,
			fallback: false,
		});
	});

	it('uses the approved fallback for unknown people', () => {
		const art = loadNotebookPersonArt(manifest);

		expect(resolveNotebookPersonArt(art, 'Una Flynn')).toEqual({
			displayName: 'Unknown parish neighbour',
			portrait: `${NOTEBOOK_ASSET_BASE}people/portrait-unknown-neighbour.png`,
			marker: `${NOTEBOOK_ASSET_BASE}people/marker-unknown-neighbour.png`,
			fallback: true,
		});
	});

	it('does not preload or resolve unapproved person art', () => {
		const art = loadNotebookPersonArt(manifest);

		expect(resolveNotebookPersonArt(art, 'Draft Person').fallback).toBe(true);
		expect(art.assetUrls).toEqual([
			`${NOTEBOOK_ASSET_BASE}people/portrait-unknown-neighbour.png`,
			`${NOTEBOOK_ASSET_BASE}people/marker-unknown-neighbour.png`,
			`${NOTEBOOK_ASSET_BASE}people/portrait-brigid-ni-fhatharta.png`,
			`${NOTEBOOK_ASSET_BASE}people/marker-brigid-ni-fhatharta.png`,
		]);
	});

	it('falls back to built-in placeholders without a usable manifest', () => {
		const art = loadNotebookPersonArt(null);

		expect(resolveNotebookPersonArt(art, 'Brigid Ni Fhatharta')).toEqual({
			displayName: 'Unknown parish neighbour',
			portrait: NOTEBOOK_ASSETS.portraits[0],
			marker: NOTEBOOK_ASSETS.npcMarkers[0],
			fallback: true,
		});
	});
});
