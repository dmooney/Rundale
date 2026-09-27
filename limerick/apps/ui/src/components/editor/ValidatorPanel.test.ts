import { beforeEach, describe, it, expect, vi } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';
import ValidatorPanel from './ValidatorPanel.svelte';
import { editorValidation } from '../../stores/editor';
import type { ValidationReport } from '$lib/editor-types';

const mocks = vi.hoisted(() => ({
	editorValidate: vi.fn<() => Promise<ValidationReport>>(),
}));

vi.mock('$lib/editor-ipc', () => ({
	editorValidate: mocks.editorValidate,
}));

describe('ValidatorPanel', () => {
	beforeEach(() => {
		editorValidation.set(null);
		mocks.editorValidate.mockResolvedValue({ errors: [], warnings: [] });
	});

	it('renders the Re-validate button', () => {
		const { getByText } = render(ValidatorPanel);
		expect(getByText('Re-validate')).toBeTruthy();
	});

	it('shows the empty state prompt when report is null', () => {
		const { getByText } = render(ValidatorPanel);
		expect(
			getByText('Click "Re-validate" to check the mod for issues.'),
		).toBeTruthy();
	});

	it('shows "All checks passed." when report has no issues', () => {
		editorValidation.set({ errors: [], warnings: [] });
		const { getByText } = render(ValidatorPanel);
		expect(getByText('All checks passed.')).toBeTruthy();
	});

	it('shows error count when errors are present', () => {
		editorValidation.set({
			errors: [
				{
					category: 'npc',
					severity: 'error',
					doc: '',
					field_path: 'npcs[0].name',
					message: 'Name is required',
					context: null,
				},
				{
					category: 'npc',
					severity: 'error',
					doc: '',
					field_path: 'npcs[1].name',
					message: 'Another error',
					context: null,
				},
			],
			warnings: [],
		});
		const { getByText } = render(ValidatorPanel);
		expect(getByText('2 errors')).toBeTruthy();
	});

	it('uses singular "error" for a single error', () => {
		editorValidation.set({
			errors: [
				{
					category: 'npc',
					severity: 'error',
					doc: '',
					field_path: 'npcs[0].name',
					message: 'Name is required',
					context: null,
				},
			],
			warnings: [],
		});
		const { getByText } = render(ValidatorPanel);
		expect(getByText('1 error')).toBeTruthy();
	});

	it('shows warning count when warnings are present', () => {
		editorValidation.set({
			errors: [],
			warnings: [
				{
					category: 'location',
					severity: 'warning',
					doc: '',
					field_path: 'locations[0].description_template',
					message: 'Template is empty',
					context: null,
				},
			],
		});
		const { getByText } = render(ValidatorPanel);
		expect(getByText('1 warning')).toBeTruthy();
	});

	it('shows both error and warning counts when both are present', () => {
		editorValidation.set({
			errors: [
				{
					category: 'npc',
					severity: 'error',
					doc: '',
					field_path: 'npcs[0].name',
					message: 'Bad',
					context: null,
				},
			],
			warnings: [
				{
					category: 'location',
					severity: 'warning',
					doc: '',
					field_path: 'locations[0]',
					message: 'Warn',
					context: null,
				},
			],
		});
		const { getByText } = render(ValidatorPanel);
		expect(getByText('1 error')).toBeTruthy();
		expect(getByText('1 warning')).toBeTruthy();
	});

	it('error rows have the is-error class', () => {
		editorValidation.set({
			errors: [
				{
					category: 'npc',
					severity: 'error',
					doc: '',
					field_path: 'npcs[0].name',
					message: 'Name required',
					context: null,
				},
			],
			warnings: [],
		});
		const { container } = render(ValidatorPanel);
		const errorRow = container.querySelector('.is-error');
		expect(errorRow).not.toBeNull();
	});

	it('warning rows have the is-warning class', () => {
		editorValidation.set({
			errors: [],
			warnings: [
				{
					category: 'loc',
					severity: 'warning',
					doc: '',
					field_path: 'locations[0]',
					message: 'Warn',
					context: null,
				},
			],
		});
		const { container } = render(ValidatorPanel);
		const warnRow = container.querySelector('.is-warning');
		expect(warnRow).not.toBeNull();
	});

	it('calls editorValidate and updates store when Re-validate is clicked', async () => {
		const newReport: ValidationReport = {
			errors: [],
			warnings: [
				{
					category: 'npc',
					severity: 'warning',
					doc: '',
					field_path: 'npcs[0]',
					message: 'New warning',
					context: null,
				},
			],
		};
		mocks.editorValidate.mockResolvedValue(newReport);
		const { getByText } = render(ValidatorPanel);

		getByText('Re-validate').click();

		await waitFor(() => {
			expect(mocks.editorValidate).toHaveBeenCalledTimes(1);
		});
		await waitFor(() => {
			expect(getByText('1 warning')).toBeTruthy();
		});
	});
});
