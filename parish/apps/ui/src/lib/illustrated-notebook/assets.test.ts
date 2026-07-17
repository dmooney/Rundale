import { describe, expect, it } from 'vitest';
import {
	loadNotebookPersonArt,
	NOTEBOOK_ASSET_BASE,
	normalizeNotebookPersonName,
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

		expect(resolveNotebookPersonArt(art, 19, 'A renamed Brigid')).toEqual({
			displayName: 'Brigid Ni Fhatharta',
			portrait: `${NOTEBOOK_ASSET_BASE}people/portrait-brigid-ni-fhatharta.png`,
			marker: `${NOTEBOOK_ASSET_BASE}people/marker-brigid-ni-fhatharta.png`,
			fallback: false,
		});
	});

	it('uses the approved fallback for unknown people', () => {
		const art = loadNotebookPersonArt(manifest);

		expect(resolveNotebookPersonArt(art, 404, 'Brigid Ni Fhatharta')).toEqual({
			displayName: 'Unknown parish neighbour',
			portrait: `${NOTEBOOK_ASSET_BASE}people/portrait-unknown-neighbour.png`,
			marker: `${NOTEBOOK_ASSET_BASE}people/marker-unknown-neighbour.png`,
			fallback: true,
		});
	});

	it('does not preload or resolve unapproved person art', () => {
		const art = loadNotebookPersonArt(manifest);

		expect(
			resolveNotebookPersonArt(art, undefined, 'Draft Person').fallback,
		).toBe(true);
		expect(art.assetUrls).toEqual([
			`${NOTEBOOK_ASSET_BASE}people/portrait-unknown-neighbour.png`,
			`${NOTEBOOK_ASSET_BASE}people/marker-unknown-neighbour.png`,
			`${NOTEBOOK_ASSET_BASE}people/portrait-brigid-ni-fhatharta.png`,
			`${NOTEBOOK_ASSET_BASE}people/marker-brigid-ni-fhatharta.png`,
		]);
	});

	it('uses production unknown art without a usable manifest', () => {
		const art = loadNotebookPersonArt(null);

		expect(resolveNotebookPersonArt(art, 19, 'Brigid Ni Fhatharta')).toEqual({
			displayName: 'Unknown parish neighbour',
			portrait: `${NOTEBOOK_ASSET_BASE}people/portrait-unknown-neighbour.png`,
			marker: `${NOTEBOOK_ASSET_BASE}people/marker-unknown-neighbour.png`,
			fallback: true,
		});
	});

	it('uses normalized names only when numeric identity is absent', () => {
		const art = loadNotebookPersonArt(manifest);

		expect(
			resolveNotebookPersonArt(art, undefined, '  brígid   ni   fhatharta  ')
				.fallback,
		).toBe(false);
		expect(
			resolveNotebookPersonArt(art, -1, 'Brigid Ni Fhatharta').fallback,
		).toBe(true);
	});

	it('rejects every entry sharing a duplicate numeric identity', () => {
		const duplicateManifest = structuredClone(manifest);
		duplicateManifest.assets.personArt?.people.push({
			real_name: 'Impostor',
			display_name: 'Impostor',
			npc_id: 19,
			portrait: 'people/portrait-impostor.png',
			marker: 'people/marker-impostor.png',
			approval_status: 'approved',
		});

		const art = loadNotebookPersonArt(duplicateManifest);

		expect(
			resolveNotebookPersonArt(art, 19, 'Brigid Ni Fhatharta').fallback,
		).toBe(true);
		expect(art.byId.has(19)).toBe(false);
		expect(
			art.byName.has(normalizeNotebookPersonName('Brigid Ni Fhatharta')),
		).toBe(false);
		expect(art.assetUrls).not.toContain(
			`${NOTEBOOK_ASSET_BASE}people/portrait-impostor.png`,
		);
	});

	it('rejects entries with invalid numeric identities or asset paths', () => {
		const invalidManifest = structuredClone(manifest);
		invalidManifest.assets.personArt?.people.push(
			{
				real_name: 'Negative Id',
				display_name: 'Negative Id',
				npc_id: -1,
				portrait: 'people/negative.png',
				marker: 'people/negative-marker.png',
				approval_status: 'approved',
			},
			{
				real_name: 'Missing Portrait',
				display_name: 'Missing Portrait',
				npc_id: 20,
				portrait: '',
				marker: 'people/missing-portrait-marker.png',
				approval_status: 'approved',
			},
		);

		const art = loadNotebookPersonArt(invalidManifest);

		expect(resolveNotebookPersonArt(art, -1, 'Negative Id').fallback).toBe(
			true,
		);
		expect(resolveNotebookPersonArt(art, 20, 'Missing Portrait').fallback).toBe(
			true,
		);
	});

	it('treats malformed manifest shapes as an empty approved roster', () => {
		const malformed = {
			assets: {
				personArt: {
					fallback: { approval_status: 'approved' },
					people: { not: 'an array' },
				},
			},
		} as unknown as NotebookAssetManifest;

		const art = loadNotebookPersonArt(malformed);

		expect(resolveNotebookPersonArt(art, 19, 'Brigid').fallback).toBe(true);
		expect(art.byId.size).toBe(0);
		expect(art.byName.size).toBe(0);
	});

	it('rejects ambiguous compatibility names while preserving unique numeric IDs', () => {
		const duplicateNameManifest = structuredClone(manifest);
		duplicateNameManifest.assets.personArt?.people.push({
			real_name: '  Brigid Ni Fhatharta ',
			display_name: 'A different Brigid',
			npc_id: 20,
			portrait: 'people/portrait-other-brigid.png',
			marker: 'people/marker-other-brigid.png',
			approval_status: 'approved',
		});

		const art = loadNotebookPersonArt(duplicateNameManifest);

		expect(resolveNotebookPersonArt(art, 19, 'wrong name').fallback).toBe(
			false,
		);
		expect(resolveNotebookPersonArt(art, 20, 'wrong name').fallback).toBe(
			false,
		);
		expect(
			resolveNotebookPersonArt(art, undefined, 'Brigid Ni Fhatharta').fallback,
		).toBe(true);
	});
});
