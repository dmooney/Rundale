import { derived } from 'svelte/store';
import { mapData, npcsHere } from './game';

export interface KnownNoun {
	text: string;
	category: 'location' | 'npc';
	priority: number; // lower = higher priority
}

/**
 * All known nouns derived from current game state.
 * Updates automatically when mapData or npcsHere change.
 */
export const knownNouns = derived(
	[mapData, npcsHere],
	([$mapData, $npcsHere]) => {
		const nouns: KnownNoun[] = [];

		if ($mapData) {
			for (const loc of $mapData.locations) {
				nouns.push({
					text: loc.name,
					category: 'location',
					priority: loc.visited ? (loc.adjacent ? 0 : 2) : 3
				});
			}
		}

		for (const npc of $npcsHere) {
			nouns.push({
				text: npc.name,
				category: 'npc',
				priority: 1
			});
		}

		nouns.sort((a, b) => a.priority - b.priority || a.text.localeCompare(b.text));
		return nouns;
	}
);

/**
 * Find nouns matching a prefix. Matches against the start of any
 * whitespace/apostrophe-delimited word in the noun text.
 *
 * Examples: "pub" matches "Darcy's Pub", "cross" matches "The Crossroads"
 */
export function findMatches(prefix: string, nouns: KnownNoun[]): KnownNoun[] {
	if (prefix.length === 0) return [];
	const lower = prefix.toLowerCase();

	return nouns.filter((noun) => {
		const nounLower = noun.text.toLowerCase();
		return (
			nounLower.startsWith(lower) ||
			nounLower.split(/[\s']+/).some((word) => word.startsWith(lower))
		);
	});
}
