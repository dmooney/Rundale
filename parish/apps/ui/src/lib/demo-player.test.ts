import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';
import {
	demoEnabled,
	demoPaused,
	demoStatus,
	demoConfig,
} from '../stores/demo';
import { runDemoTurn, stopDemo } from './demo-player';

const testConfig = {
	auto_start: false,
	extra_prompt: null,
	turn_pause_secs: 0.01,
	max_turns: null,
};

vi.mock('../lib/ipc', () => ({
	getDemoContext: vi.fn(async () => ({
		world_description: 'A village',
		recent_log: [],
		nearby_npcs: [],
		recent_events: [],
		extra_prompt: null,
	})),
	getLlmPlayerAction: vi.fn(async () => '"look around"'),
	submitInput: vi.fn(async () => {}),
}));

beforeEach(() => {
	demoEnabled.set(false);
	demoPaused.set(false);
	demoStatus.set('idle');
	demoConfig.set(testConfig);
});

describe('stopDemo', () => {
	it('sets demoEnabled to false', () => {
		demoEnabled.set(true);
		stopDemo();
		expect(get(demoEnabled)).toBe(false);
	});
});

describe('runDemoTurn', () => {
	it('returns immediately when demo is disabled', async () => {
		demoEnabled.set(false);
		await expect(runDemoTurn()).resolves.toBeUndefined();
	});

	it('returns immediately when demo is paused', async () => {
		demoEnabled.set(true);
		demoPaused.set(true);
		await expect(runDemoTurn()).resolves.toBeUndefined();
	});

	it('sets status to waiting when demo is active', async () => {
		demoEnabled.set(true);
		// runDemoTurn will set status to 'waiting', then sleep.
		// We use a timeout to prevent the test from hanging on the sleep.
		const promise = runDemoTurn();
		expect(get(demoStatus)).toBe('waiting');
		// Let the turn complete (sleep timeout)
		await promise;
	});
});
