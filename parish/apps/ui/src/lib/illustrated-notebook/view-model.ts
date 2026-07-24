import { isNotebookLogEntry } from '$lib/notebook/log';
import { notebookPersonLabel } from '$lib/notebook/people';
import type { NpcInfo, TextLogEntry } from '$lib/types';
import type {
	NotebookLiveLine,
	NotebookLiveLineKind,
	NotebookViewModel,
	NotebookViewModelInput,
} from './types';

export const MAX_NOTEBOOK_LIVE_LINES = 5;
const MAX_NOTEBOOK_LINE_LENGTH = 180;
const MAX_PERSON_LINES = 2;

interface ClassifiedLine {
	line: NotebookLiveLine;
	npc: NpcInfo | null;
}

export function notebookNpcLabel(npc: NpcInfo): string {
	const displayName = npc.name.trim();
	const realName = npc.real_name.trim();

	if (npc.introduced) {
		return displayName || realName || 'Unknown person';
	}

	if (
		!displayName ||
		(realName &&
			displayName.toLocaleLowerCase().includes(realName.toLocaleLowerCase()))
	) {
		return 'Unintroduced person';
	}

	return notebookPersonLabel({ ...npc, real_name: '' });
}

export function buildNotebookViewModel(
	input: NotebookViewModelInput,
): NotebookViewModel {
	const classified = input.textLog
		.map((entry, index) => classifyLine(entry, index, input.npcs))
		.filter((entry): entry is ClassifiedLine => entry !== null);
	const liveLines = selectLiveLines(classified.map(({ line }) => line));
	const selectedNpc = input.selectedNpc;
	const locationName =
		cleanText(input.world?.location_name) || 'Location not yet known';
	const locationDescription =
		cleanText(input.world?.location_description) ||
		'No description of this place has arrived yet.';
	const person = selectedNpc
		? {
				label: notebookNpcLabel(selectedNpc),
				mood: cleanText(selectedNpc.mood) || 'Mood not yet observed',
				detail:
					selectedNpc.introduced && cleanText(selectedNpc.occupation)
						? cleanText(selectedNpc.occupation)
						: selectedNpc.introduced
							? 'Occupation not recorded'
							: 'Not yet introduced',
				recentLines: classified
					.filter(
						({ npc }) =>
							npc !== null && npc.real_name === selectedNpc.real_name,
					)
					.map(({ line }) => line)
					.slice(-MAX_PERSON_LINES),
				emptyNote: 'No spoken exchange with this person is recorded yet.',
			}
		: null;

	return {
		locationName,
		locationDescription,
		weather: cleanText(input.world?.weather) || 'Weather not yet known',
		time: input.world
			? `${String(input.world.hour).padStart(2, '0')}:${String(
					input.world.minute,
				).padStart(2, '0')}`
			: 'Time not yet known',
		person,
		liveTitle: input.busy ? 'Live chronicle · listening' : 'Live chronicle',
		liveEmpty: input.busy
			? 'Waiting for the next words from the parish…'
			: 'Your commands, actions, and parish replies will appear here.',
		liveLines,
		intentPlaceholder: input.busy
			? 'waiting on the parish…'
			: person
				? `Write what you say to ${person.label}…`
				: input.world
					? `What do you do at ${locationName}?`
					: 'Write what you do next…',
		draftSummary: cleanText(input.intentText) || 'No draft written',
	};
}

function classifyLine(
	entry: TextLogEntry,
	index: number,
	npcs: NpcInfo[],
): ClassifiedLine | null {
	if (!isNotebookLogEntry(entry)) return null;
	const content = truncate(sanitizeUnintroducedNames(entry.content, npcs));
	if (!content) return null;

	const normalizedSource = cleanText(entry.source).toLocaleLowerCase();
	const npc = findNpcForSource(entry.source, npcs);
	let kind: NotebookLiveLineKind;
	let speaker: string;

	if (normalizedSource === 'player' || normalizedSource === 'you') {
		kind = 'player';
		speaker = 'You';
	} else if (normalizedSource === 'system') {
		kind =
			entry.subtype === 'location'
				? 'location'
				: entry.subtype === 'error'
					? 'error'
					: 'narration';
		speaker =
			kind === 'location' ? 'Place' : kind === 'error' ? 'Notice' : 'Parish';
	} else {
		kind = 'npc';
		speaker = npc ? notebookNpcLabel(npc) : 'Someone';
	}

	return {
		npc,
		line: {
			key:
				entry.id ||
				`${index}:${normalizedSource || 'unknown'}:${content.slice(0, 32)}`,
			kind,
			speaker,
			content,
			streaming: Boolean(entry.streaming),
		},
	};
}

function selectLiveLines(lines: NotebookLiveLine[]): NotebookLiveLine[] {
	if (lines.length <= MAX_NOTEBOOK_LIVE_LINES) return lines;

	let latestPlayerIndex = -1;
	for (let i = lines.length - 1; i >= 0; i -= 1) {
		if (lines[i].kind === 'player') {
			latestPlayerIndex = i;
			break;
		}
	}

	if (latestPlayerIndex < 0) return lines.slice(-MAX_NOTEBOOK_LIVE_LINES);
	const currentTurn = lines.slice(latestPlayerIndex);
	if (currentTurn.length <= MAX_NOTEBOOK_LIVE_LINES) return currentTurn;
	return [currentTurn[0], ...currentTurn.slice(-(MAX_NOTEBOOK_LIVE_LINES - 1))];
}

function findNpcForSource(source: string, npcs: NpcInfo[]): NpcInfo | null {
	const normalized = cleanText(source).toLocaleLowerCase();
	if (!normalized) return null;

	const exact =
		npcs.find(
			(npc) =>
				cleanText(npc.real_name).toLocaleLowerCase() === normalized ||
				cleanText(npc.name).toLocaleLowerCase() === normalized,
		) ?? null;
	if (exact) return exact;

	const firstNameMatches = npcs.filter((npc) => {
		const firstName = cleanText(npc.real_name).split(/\s+/)[0];
		return firstName && firstName.toLocaleLowerCase() === normalized;
	});
	return firstNameMatches.length === 1 ? firstNameMatches[0] : null;
}

function sanitizeUnintroducedNames(content: string, npcs: NpcInfo[]): string {
	let sanitized = cleanText(content);
	for (const npc of npcs) {
		const realName = npc.real_name.trim();
		if (npc.introduced || !realName) continue;
		sanitized = sanitized.replace(
			new RegExp(escapeRegExp(realName), 'giu'),
			notebookNpcLabel(npc),
		);
	}
	return sanitized;
}

function cleanText(value: string | null | undefined): string {
	return (value ?? '').replace(/\s+/g, ' ').trim();
}

function truncate(value: string): string {
	if (value.length <= MAX_NOTEBOOK_LINE_LENGTH) return value;
	return `${value.slice(0, MAX_NOTEBOOK_LINE_LENGTH - 1).trimEnd()}…`;
}

function escapeRegExp(value: string): string {
	return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
