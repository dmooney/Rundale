import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import { get } from 'svelte/store';
import {
	demoEnabled,
	demoPaused,
	demoTurnCount,
	demoStatus,
} from '../stores/demo';
import DemoBanner from './DemoBanner.svelte';

vi.mock('../lib/demo-player', () => ({
	stopDemo: vi.fn(),
}));

beforeEach(() => {
	demoEnabled.set(false);
	demoPaused.set(false);
	demoTurnCount.set(0);
	demoStatus.set('idle');
});

describe('DemoBanner', () => {
	it('does not render when demo is not enabled', () => {
		const { container } = render(DemoBanner);
		expect(container.querySelector('.demo-banner')).toBeNull();
	});

	it('renders when demo is enabled', () => {
		demoEnabled.set(true);
		const { container } = render(DemoBanner);
		expect(container.querySelector('.demo-banner')).toBeTruthy();
	});

	it('displays the current turn count', () => {
		demoEnabled.set(true);
		demoTurnCount.set(5);
		const { getByText } = render(DemoBanner);
		expect(getByText('Turn 5')).toBeTruthy();
	});

	it('shows Pause button initially', () => {
		demoEnabled.set(true);
		const { getByText } = render(DemoBanner);
		expect(getByText('Pause')).toBeTruthy();
	});

	it('shows Resume when paused', () => {
		demoEnabled.set(true);
		demoPaused.set(true);
		const { getByText } = render(DemoBanner);
		expect(getByText('Resume')).toBeTruthy();
	});

	it('shows status label for idle state', () => {
		demoEnabled.set(true);
		const { getByText } = render(DemoBanner);
		expect(getByText('idle')).toBeTruthy();
	});

	it('shows status label for acting state', () => {
		demoEnabled.set(true);
		demoStatus.set('acting');
		const { getByText } = render(DemoBanner);
		expect(getByText('acting')).toBeTruthy();
	});

	it('has a Stop button', () => {
		demoEnabled.set(true);
		const { container } = render(DemoBanner);
		expect(container.querySelector('.demo-stop')).toBeTruthy();
	});
});
