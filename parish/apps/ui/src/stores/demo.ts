import { writable } from 'svelte/store';

export const demoEnabled = writable(false);
export const demoPaused = writable(false);
export const demoTurnCount = writable(0);
export const demoStatus = writable<'idle' | 'waiting' | 'thinking' | 'acting'>('idle');
export const demoVisible = writable(false);

export interface DemoConfig {
	auto_start: boolean;
	extra_prompt: string | null;
	turn_pause_secs: number;
	max_turns: number | null;
}

export const demoConfig = writable<DemoConfig>({
	auto_start: false,
	extra_prompt: null,
	turn_pause_secs: 2.0,
	max_turns: null
});
