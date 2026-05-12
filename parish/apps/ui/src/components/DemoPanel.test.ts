import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import { get } from 'svelte/store';
import {
	demoEnabled,
	demoPaused,
	demoTurnCount,
	demoStatus,
	demoConfig,
} from '../stores/demo';
import DemoPanel from './DemoPanel.svelte';

vi.mock('../lib/demo-player', () => ({
	startDemoLoop: vi.fn(),
	stopDemo: vi.fn(),
}));

beforeEach(() => {
	demoEnabled.set(false);
	demoPaused.set(false);
	demoTurnCount.set(0);
	demoStatus.set('idle');
});

describe('DemoPanel', () => {
	it('renders demo configuration fields', () => {
		const { getByText } = render(DemoPanel);
		expect(getByText('DEMO MODE')).toBeTruthy();
		expect(getByText('Pause between turns (s)')).toBeTruthy();
		expect(getByText('Max turns (0 = unlimited)')).toBeTruthy();
		expect(getByText('Extra prompt instructions')).toBeTruthy();
	});

	it('shows Apply & Start button when demo not running', () => {
		const { getByText } = render(DemoPanel);
		expect(getByText('Apply & Start')).toBeTruthy();
	});

	it('shows Pause and Stop buttons when demo is running', () => {
		demoEnabled.set(true);
		const { getByText } = render(DemoPanel);
		expect(getByText('Pause')).toBeTruthy();
		expect(getByText('Stop')).toBeTruthy();
	});

	it('shows Resume when demo is running and paused', () => {
		demoEnabled.set(true);
		demoPaused.set(true);
		const { getByText } = render(DemoPanel);
		expect(getByText('Resume')).toBeTruthy();
	});

	it('displays current turn count', () => {
		demoEnabled.set(true);
		demoTurnCount.set(3);
		const { getByText } = render(DemoPanel);
		expect(getByText('3')).toBeTruthy();
	});

	it('displays current status', () => {
		demoEnabled.set(true);
		demoStatus.set('thinking');
		const { getByText } = render(DemoPanel);
		expect(getByText('thinking')).toBeTruthy();
	});

	it('has a close button', () => {
		const { container } = render(DemoPanel);
		expect(container.querySelector('.close-btn')).toBeTruthy();
	});
});
