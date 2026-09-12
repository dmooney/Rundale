import { render } from '@testing-library/svelte';
import { beforeEach, describe, expect, it } from 'vitest';
import { worldState } from '../stores/game';
import SceneHeader from './SceneHeader.svelte';

describe('SceneHeader', () => {
	beforeEach(() => worldState.set(null));

	it('renders responsive DOM art with readable location context', () => {
		worldState.set({
			location_name: 'Kilteevan',
			location_description: 'Rain shines on the parish crossroads.',
		} as never);
		const { getByTestId, getByText, container } = render(SceneHeader);
		expect(getByTestId('scene-header')).toBeTruthy();
		expect(getByText('Kilteevan')).toBeTruthy();
		expect(container.querySelector('picture source')).toHaveAttribute(
			'media',
			'(max-width: 768px)',
		);
		expect(container.querySelector('canvas')).toBeNull();
	});
});
