import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { tick } from 'svelte';
import type { BugReportResult } from '$lib/types';

/** Flush all pending microtasks + Svelte ticks. */
async function flush() {
	await Promise.resolve();
	await tick();
	await Promise.resolve();
	await tick();
}

const mockSubmitBugReport = vi.fn<(args: unknown) => Promise<BugReportResult>>(
	() =>
		Promise.resolve({
			created: false,
			bundle_path: '/tmp/bug/issue.md',
			message: 'Dry-run: composed bug report',
		}),
);

vi.mock('$lib/ipc', () => ({
	submitBugReport: (args: unknown) => mockSubmitBugReport(args),
}));

import BugReportModal from './BugReportModal.svelte';
import {
	bugReportVisible,
	bugReportContext,
	bugReportScreenshot,
	closeBugReport,
} from '../stores/bugReport';

describe('BugReportModal', () => {
	beforeEach(() => {
		mockSubmitBugReport.mockClear();
		closeBugReport();
		bugReportScreenshot.set('data:image/png;base64,AAAA');
	});

	it('is hidden until visible', () => {
		const { queryByTestId } = render(BugReportModal);
		expect(queryByTestId('bug-report-modal')).toBeNull();
	});

	it('submits title, description, screenshot and context', async () => {
		bugReportContext.set({
			kind: 'inference',
			label: 'call #3',
			detail: { request_id: 3 },
		});
		bugReportVisible.set(true);
		const { getByTestId, getByLabelText } = render(BugReportModal);
		await flush();

		await fireEvent.input(getByLabelText('Title'), {
			target: { value: 'NPC stuck' },
		});
		await fireEvent.input(getByLabelText('Description'), {
			target: { value: 'Never arrives' },
		});
		await fireEvent.click(getByTestId('bug-report-submit'));
		await flush();

		expect(mockSubmitBugReport).toHaveBeenCalledTimes(1);
		expect(mockSubmitBugReport).toHaveBeenCalledWith({
			title: 'NPC stuck',
			description: 'Never arrives',
			screenshotDataUrl: 'data:image/png;base64,AAAA',
			context: {
				kind: 'inference',
				label: 'call #3',
				detail: { request_id: 3 },
			},
		});
		// Result is rendered after submit.
		expect(getByTestId('bug-report-result').textContent).toContain('Dry-run');
	});

	it('does not submit with an empty title', async () => {
		bugReportVisible.set(true);
		const { getByTestId } = render(BugReportModal);
		await flush();
		// Submit button is disabled with an empty title.
		const btn = getByTestId('bug-report-submit') as HTMLButtonElement;
		expect(btn.disabled).toBe(true);
		await fireEvent.click(btn);
		await flush();
		expect(mockSubmitBugReport).not.toHaveBeenCalled();
	});
});
