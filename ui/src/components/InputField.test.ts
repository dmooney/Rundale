import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { streamingActive } from '../stores/game';
import InputField from './InputField.svelte';

// Mock ipc submitInput
vi.mock('$lib/ipc', () => ({
	submitInput: vi.fn(async (_text: string) => {})
}));

describe('InputField', () => {
	beforeEach(() => {
		streamingActive.set(false);
	});

	it('renders an input field', () => {
		const { getByRole } = render(InputField);
		expect(getByRole('textbox')).toBeTruthy();
	});

	it('has correct placeholder when idle', () => {
		const { getByPlaceholderText } = render(InputField);
		expect(getByPlaceholderText('What do you do?')).toBeTruthy();
	});

	it('is disabled when streaming', () => {
		streamingActive.set(true);
		const { getByRole } = render(InputField);
		expect((getByRole('textbox') as HTMLInputElement).disabled).toBe(true);
	});

	it('clears input after submit', async () => {
		const { getByRole } = render(InputField);
		const input = getByRole('textbox') as HTMLInputElement;
		await fireEvent.input(input, { target: { value: 'hello' } });
		await fireEvent.keyDown(input, { key: 'Enter' });
		// Input should be cleared
		expect(input.value).toBe('');
	});
});
