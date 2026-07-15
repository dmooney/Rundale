import type { NpcInfo } from '$lib/types';
import {
	notebookActionDraft,
	type NotebookAction,
} from '$lib/notebook/actions';

export type SubmitInput = (text: string) => Promise<void>;

export interface SubmitNotebookCommandOptions {
	text: string;
	busy: boolean;
	paused: boolean;
	submitInput: SubmitInput;
	onLocalSubmit: () => void;
}

export function draftForNotebookAction(
	action: NotebookAction,
	selectedNpc: NpcInfo | null,
): string {
	return notebookActionDraft(action, selectedNpc);
}

export async function submitNotebookCommand({
	text,
	busy,
	paused,
	submitInput,
	onLocalSubmit,
}: SubmitNotebookCommandOptions): Promise<boolean> {
	const trimmed = text.trim();
	if (!trimmed || busy) return false;

	if (paused && !trimmed.startsWith('/')) {
		await submitInput('/resume');
	}
	onLocalSubmit();
	await submitInput(trimmed);
	return true;
}
