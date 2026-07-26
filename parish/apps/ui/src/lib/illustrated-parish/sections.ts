import type { TextLogEntry } from '$lib/types';
import type { ParishRenderState, ParishTab } from './types';

export interface NotebookSectionLine {
	label: string;
	text: string;
}

export interface NotebookSectionContent {
	tab: ParishTab;
	title: string;
	lines: NotebookSectionLine[];
}

const EMPTY_NOTE = 'Nothing has been written here yet.';

/**
 * Builds the same concise section model used by the visible Pixi page and its
 * accessibility companion. Tabs are page navigation; they never imply an
 * overlay route.
 */
export function buildNotebookSectionContent(
	state: Pick<
		ParishRenderState,
		'activeTab' | 'world' | 'map' | 'npcs' | 'selectedNpc' | 'journalEntries'
	>,
): NotebookSectionContent {
	switch (state.activeTab) {
		case 'notes':
			return {
				tab: 'notes',
				title: 'Parish Notes',
				lines: [
					{
						label: 'Here',
						text: state.world?.location_name ?? 'Location unknown',
					},
					{
						label: 'Scene',
						text: state.world?.location_description || EMPTY_NOTE,
					},
					{
						label: 'Conditions',
						text:
							[
								state.world?.time_label,
								state.world?.weather,
								state.world?.season,
							]
								.filter(Boolean)
								.join(' · ') || 'Not recorded',
					},
					{
						label: state.world?.active_tasks?.[0] ? 'Work' : 'Next',
						text: state.world?.active_tasks?.[0]
							? `${state.world.active_tasks[0].description} (${state.world.active_tasks[0].status.replace('_', ' ')})`
							: 'Write an intent below, or choose an action stamp.',
					},
				],
			};
		case 'people': {
			const selected = state.selectedNpc;
			return {
				tab: 'people',
				title: selected?.name ?? 'People of the Parish',
				lines: selected
					? [
							{
								label: 'Calling',
								text: selected.occupation || 'Parish resident',
							},
							{ label: 'Mood', text: selected.mood || 'Watchful' },
							{
								label: 'Nearby',
								text: nearbyPeople(state.npcs),
							},
							{
								label: 'Choose',
								text: 'Select a portrait in the Nearby strip.',
							},
						]
					: [{ label: 'Nearby', text: 'No one is nearby.' }],
			};
		}
		case 'places': {
			const current =
				state.map?.locations.find(
					(location) => location.id === state.map?.player_location,
				)?.name ??
				state.world?.location_name ??
				'Location unknown';
			const adjacent =
				state.map?.locations
					.filter((location) => location.adjacent)
					.map((location) => location.name) ?? [];
			return {
				tab: 'places',
				title: 'Places in this Parish',
				lines: [
					{ label: 'Here', text: current },
					{
						label: 'Roads from here',
						text: adjacent.length > 0 ? adjacent.join(' · ') : 'None recorded',
					},
					{
						label: 'Known places',
						text: String(state.map?.locations.length ?? 0),
					},
					{
						label: 'Geography',
						text: 'Open the Map card below for routes and orientation.',
					},
				],
			};
		}
		case 'rumours':
			return {
				tab: 'rumours',
				title: 'Rumours',
				lines: [
					{
						label: 'Pinned',
						text: 'No rumour is pinned to this page yet.',
					},
					{
						label: 'Listen',
						text: 'Stories appear here when somebody trusts you with one.',
					},
				],
			};
		case 'journal':
			return {
				tab: 'journal',
				title: 'Parish Journal',
				lines: journalLines(state.journalEntries, state.npcs),
			};
	}
}

function nearbyPeople(npcs: ParishRenderState['npcs']): string {
	if (npcs.length === 0) return 'No one is nearby.';
	return npcs
		.slice(0, 3)
		.map((npc) => npc.name)
		.join(' · ');
}

function journalLines(
	entries: TextLogEntry[],
	npcs: ParishRenderState['npcs'],
): NotebookSectionLine[] {
	const substantive = entries.filter(
		(entry) => entry.content.trim().length > 0,
	);
	const recent = substantive.slice(-4);
	const latestPlayer = [...substantive]
		.reverse()
		.find((entry) => entry.source.trim().toLowerCase() === 'player');
	if (latestPlayer && !recent.includes(latestPlayer) && recent.length > 0) {
		recent[0] = latestPlayer;
		recent.sort(
			(left, right) => substantive.indexOf(left) - substantive.indexOf(right),
		);
	}
	if (recent.length === 0) {
		return [{ label: 'Latest', text: EMPTY_NOTE }];
	}
	return recent.map((entry) => ({
		label: journalSource(entry, npcs),
		text:
			entry.content +
			(entry.reactions?.length
				? ` · ${entry.reactions.map((reaction) => reaction.emoji).join(' ')}`
				: ''),
	}));
}

function journalSource(
	entry: TextLogEntry,
	npcs: ParishRenderState['npcs'],
): string {
	const source = entry.source.trim();
	const normalized = source.toLowerCase();
	if (normalized === 'player') return 'You';
	if (
		entry.subtype === 'action' ||
		['system', 'action', 'location', 'parish'].includes(normalized)
	) {
		return 'Parish';
	}
	const npc = npcs.find(
		(candidate) => candidate.real_name === source || candidate.name === source,
	);
	if (!npc) return 'Parish';
	return npc.introduced ? npc.name : `a ${npc.occupation?.trim() || 'local'}`;
}
