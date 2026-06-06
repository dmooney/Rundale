import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import MapTooltip from './MapTooltip.svelte';
import type { MapTooltipInfo } from '$lib/types';

describe('MapTooltip', () => {
	it('renders nothing when info is null', () => {
		const { container } = render(MapTooltip, {
			props: { info: null },
		});
		expect(container.querySelector('[role="tooltip"]')).toBeNull();
	});

	it('renders the location name', () => {
		const info: MapTooltipInfo = { name: 'The Crossroads' };
		const { getByRole } = render(MapTooltip, { props: { info } });
		expect(getByRole('tooltip').textContent).toContain('The Crossroads');
	});

	it('shows "Unexplored" when visited is false', () => {
		const info: MapTooltipInfo = { name: 'Mystery Place', visited: false };
		const { getByText } = render(MapTooltip, { props: { info } });
		expect(getByText('Unexplored')).toBeTruthy();
	});

	it('does not show "Unexplored" when visited is true', () => {
		const info: MapTooltipInfo = { name: 'Known Place', visited: true };
		const { queryByText } = render(MapTooltip, { props: { info } });
		expect(queryByText('Unexplored')).toBeNull();
	});

	it('shows "Indoor" when indoor is true and visited', () => {
		const info: MapTooltipInfo = { name: 'Barn', indoor: true, visited: true };
		const { getByText } = render(MapTooltip, { props: { info } });
		expect(getByText('Indoor')).toBeTruthy();
	});

	it('shows "Outdoor" when indoor is false and visited', () => {
		const info: MapTooltipInfo = {
			name: 'Field',
			indoor: false,
			visited: true,
		};
		const { getByText } = render(MapTooltip, { props: { info } });
		expect(getByText('Outdoor')).toBeTruthy();
	});

	it('shows travel_minutes when positive and visited', () => {
		const info: MapTooltipInfo = {
			name: 'Mill',
			visited: true,
			travel_minutes: 15,
		};
		const { getByText } = render(MapTooltip, { props: { info } });
		expect(getByText('15 min walk')).toBeTruthy();
	});

	it('does not show travel detail when travel_minutes is 0', () => {
		const info: MapTooltipInfo = {
			name: 'Here',
			visited: true,
			travel_minutes: 0,
		};
		const { queryByText } = render(MapTooltip, { props: { info } });
		expect(queryByText(/min walk/)).toBeNull();
	});

	it('does not show indoor/outdoor detail when indoor is undefined', () => {
		const info: MapTooltipInfo = { name: 'Place', visited: true };
		const { queryByText } = render(MapTooltip, { props: { info } });
		expect(queryByText('Indoor')).toBeNull();
		expect(queryByText('Outdoor')).toBeNull();
	});

	it('adds tooltip--full class when variant="full"', () => {
		const info: MapTooltipInfo = { name: 'Place' };
		const { getByRole } = render(MapTooltip, {
			props: { info, variant: 'full' },
		});
		expect(getByRole('tooltip').classList.contains('tooltip--full')).toBe(true);
	});

	it('does not add tooltip--full class for default variant', () => {
		const info: MapTooltipInfo = { name: 'Place' };
		const { getByRole } = render(MapTooltip, { props: { info } });
		expect(getByRole('tooltip').classList.contains('tooltip--full')).toBe(
			false,
		);
	});
});
