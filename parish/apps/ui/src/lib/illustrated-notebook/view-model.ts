import { isNotebookLogEntry } from '$lib/notebook/log';
import { notebookPersonLabel } from '$lib/notebook/people';
import type { NpcInfo, Reaction, TextLogEntry } from '$lib/types';
import type {
	NotebookLiveLine,
	NotebookLiveLineKind,
	NotebookTaskView,
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
	const liveLines = selectVisibleNotebookLines(
		classified.map(({ line }) => line),
		MAX_NOTEBOOK_LIVE_LINES,
	);
	const selectedNpc = input.selectedNpc;
	const activeTasks = selectActiveTasks(input.world?.active_tasks ?? []);
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
		currentTask: activeTasks[0] ?? null,
		activeTasks,
	};
}

function selectActiveTasks(
	tasks: NonNullable<NotebookViewModelInput['world']>['active_tasks'],
): NotebookTaskView[] {
	return tasks
		.filter(
			(
				task,
			): task is typeof task & {
				status: 'assigned' | 'in_progress';
			} =>
				(task.status === 'in_progress' || task.status === 'assigned') &&
				Boolean(cleanText(task.description)),
		)
		.map<NotebookTaskView>((task) => ({
			id: task.id,
			description: cleanText(task.description),
			status: task.status,
			statusLabel: task.status === 'in_progress' ? 'In progress' : 'Assigned',
		}))
		.sort((left, right) => {
			const leftPriority = left.status === 'in_progress' ? 0 : 1;
			const rightPriority = right.status === 'in_progress' ? 0 : 1;
			return leftPriority - rightPriority;
		});
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
	const actionNarration = entry.subtype === 'action';
	const npc = actionNarration ? null : findNpcForSource(entry.source, npcs);
	let kind: NotebookLiveLineKind;
	let speaker: string;

	if (actionNarration) {
		kind = 'narration';
		speaker = 'Parish';
	} else if (normalizedSource === 'player' || normalizedSource === 'you') {
		kind = entry.subtype === 'command' ? 'command' : 'player';
		speaker = kind === 'command' ? 'Command' : 'You';
	} else if (normalizedSource === 'system' || normalizedSource === 'action') {
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
				`${index}:${cleanText(speaker).toLocaleLowerCase() || 'unknown'}:${content.slice(0, 32)}`,
			kind,
			speaker,
			content,
			streaming: Boolean(entry.streaming),
			messageId: cleanText(entry.id) || null,
			reactions: (entry.reactions ?? []).map((reaction) => ({
				...reaction,
				source:
					reaction.source === 'player'
						? 'player'
						: sanitizeUnintroducedNames(reaction.source, npcs),
			})),
		},
	};
}

/** Compact player-visible reaction text shared by Pixi and the DOM drawer. */
export function notebookReactionSummary(reactions: Reaction[]): string {
	return reactions
		.map((reaction) => {
			const source = cleanText(reaction.source);
			return source && source !== 'player'
				? `${reaction.emoji} ${source}`
				: reaction.emoji;
		})
		.join(' · ');
}

/**
 * Budgets the visible chronicle without losing the latest player input.
 *
 * The view model uses this for its five-line cap and the Pixi renderer uses
 * it again for its smaller responsive panels. A plain tail slice at either
 * layer would hide the command that caused a long multi-line response.
 */
export function selectVisibleNotebookLines(
	lines: NotebookLiveLine[],
	limit: number,
): NotebookLiveLine[] {
	if (limit <= 0) return [];
	if (lines.length <= limit) return lines;

	let latestPlayerIndex = -1;
	for (let i = lines.length - 1; i >= 0; i -= 1) {
		if (lines[i].kind === 'player' || lines[i].kind === 'command') {
			latestPlayerIndex = i;
			break;
		}
	}

	if (latestPlayerIndex < 0) {
		return selectPrioritizedOutputs(lines, limit);
	}

	const currentTurn = lines.slice(latestPlayerIndex);
	if (currentTurn.length <= limit) return currentTurn;
	if (limit === 1) return [lines[latestPlayerIndex]];
	return [
		currentTurn[0],
		...selectPrioritizedOutputs(currentTurn.slice(1), limit - 1),
	];
}

/**
 * Select newest authoritative output while retaining an actively streamed NPC
 * line even if a later status/event arrives during the same player turn.
 */
function selectPrioritizedOutputs(
	lines: NotebookLiveLine[],
	limit: number,
): NotebookLiveLine[] {
	if (limit <= 0 || lines.length === 0) return [];
	if (lines.length <= limit) return lines;

	let activeStreamIndex = -1;
	for (let index = lines.length - 1; index >= 0; index -= 1) {
		if (lines[index].kind === 'npc' && lines[index].streaming) {
			activeStreamIndex = index;
			break;
		}
	}
	if (activeStreamIndex < 0) return lines.slice(-limit);
	if (limit === 1) return [lines[activeStreamIndex]];

	const selectedIndexes = new Set<number>([activeStreamIndex]);
	for (
		let index = lines.length - 1;
		index >= 0 && selectedIndexes.size < limit;
		index -= 1
	) {
		selectedIndexes.add(index);
	}
	return [...selectedIndexes]
		.sort((left, right) => left - right)
		.map((index) => lines[index]);
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
