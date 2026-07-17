import { fireEvent, render } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import BugChip from './BugChip.svelte';

const { openNotebookOverlay } = vi.hoisted(() => ({
	openNotebookOverlay: vi.fn(async () => true),
}));

vi.mock('../stores/notebookOverlay', () => ({ openNotebookOverlay }));

describe('BugChip', () => {
	beforeEach(() => openNotebookOverlay.mockClear());

	it('routes record reports through the notebook overlay coordinator', async () => {
		const detail = { requestId: 'req-1630' };
		const { getByRole } = render(BugChip, {
			props: { kind: 'inference', label: 'Failed response', detail },
		});
		const button = getByRole('button', {
			name: 'Report a bug about this record',
		});

		await fireEvent.click(button);

		expect(openNotebookOverlay).toHaveBeenCalledWith('bug', button, {
			kind: 'inference',
			label: 'Failed response',
			detail,
		});
	});
});
