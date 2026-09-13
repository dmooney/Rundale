import { fireEvent, render } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import BugChip from './BugChip.svelte';

const { openSurface } = vi.hoisted(() => ({
	openSurface: vi.fn(async () => true),
}));

vi.mock('../stores/surfaceCoordinator', () => ({ openSurface }));

describe('BugChip', () => {
	beforeEach(() => openSurface.mockClear());

	it('routes record reports through the surface coordinator', async () => {
		const detail = { requestId: 'req-1630' };
		const { getByRole } = render(BugChip, {
			props: { kind: 'inference', label: 'Failed response', detail },
		});
		const button = getByRole('button', {
			name: 'Report a bug about this record',
		});

		await fireEvent.click(button);

		expect(openSurface).toHaveBeenCalledWith('bug', button, {
			kind: 'inference',
			label: 'Failed response',
			detail,
		});
	});
});
