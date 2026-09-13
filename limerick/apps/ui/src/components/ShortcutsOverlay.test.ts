import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import ShortcutsOverlay from './ShortcutsOverlay.svelte';

describe('ShortcutsOverlay', () => {
	it('renders the shortcut list with the main bindings', () => {
		const { getByTestId, getByText, queryByText } = render(ShortcutsOverlay, {
			props: { onclose: () => {} },
		});
		expect(getByTestId('shortcuts-overlay')).toBeTruthy();
		expect(getByText('Toggle the full parish map')).toBeTruthy();
		expect(getByText('Open the Ledger (save / load)')).toBeTruthy();
		expect(getByText('Toggle the debug panel')).toBeTruthy();
		expect(getByText('Save a screenshot')).toBeTruthy();
		expect(getByText('Move through chat and surface controls')).toBeTruthy();
		expect(
			getByText('Activate a control or send the current message'),
		).toBeTruthy();
		expect(queryByText('Browse input history')).toBeNull();
		expect(queryByText('New line (Enter sends)')).toBeNull();
	});

	it('calls onclose when the close button is clicked', async () => {
		const onclose = vi.fn();
		const { getByLabelText } = render(ShortcutsOverlay, { props: { onclose } });
		await fireEvent.click(getByLabelText('Close shortcuts'));
		expect(onclose).toHaveBeenCalledOnce();
	});

	it('calls onclose on Escape', async () => {
		const onclose = vi.fn();
		render(ShortcutsOverlay, { props: { onclose } });
		await fireEvent.keyDown(window, { key: 'Escape' });
		expect(onclose).toHaveBeenCalledOnce();
	});

	it('calls onclose when the backdrop is clicked', async () => {
		const onclose = vi.fn();
		const { getByLabelText } = render(ShortcutsOverlay, { props: { onclose } });
		await fireEvent.click(getByLabelText('Dismiss shortcuts overlay'));
		expect(onclose).toHaveBeenCalledOnce();
	});

	it('calls onclose on a second ?', async () => {
		const onclose = vi.fn();
		render(ShortcutsOverlay, { props: { onclose } });
		await fireEvent.keyDown(window, { key: '?' });
		expect(onclose).toHaveBeenCalledOnce();
	});
});
