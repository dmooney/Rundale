/**
 * First-run setup overlay snapshot + progress events (#1200 TD-054).
 */

import { command, onEvent } from './transport';

export interface SetupStatusPayload {
	message: string;
}
export interface SetupProgressPayload {
	/** Bytes downloaded so far across discovered Ollama pull artifacts. */
	completed: number;
	/** Total bytes expected across discovered Ollama pull artifacts. */
	total: number;
}
export interface SetupDonePayload {
	success: boolean;
	error: string;
}
export interface SetupSnapshot {
	current_message: string;
	messages: string[];
	completed: number;
	total: number;
	done: boolean;
	success: boolean | null;
	error: string;
	needs_onboarding: boolean;
}

export const getSetupSnapshot = () =>
	command<SetupSnapshot>('get_setup_snapshot');

export const onSetupStatus = (cb: (payload: SetupStatusPayload) => void) =>
	onEvent<SetupStatusPayload>('setup-status', cb);

export const onSetupProgress = (cb: (payload: SetupProgressPayload) => void) =>
	onEvent<SetupProgressPayload>('setup-progress', cb);

export const onSetupDone = (cb: (payload: SetupDonePayload) => void) =>
	onEvent<SetupDonePayload>('setup-done', cb);

export const onSetupNeedsOnboarding = (
	cb: (payload: SetupStatusPayload) => void,
) => onEvent<SetupStatusPayload>('setup-needs-onboarding', cb);
