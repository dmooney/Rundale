import { describe, expect, it, vi } from 'vitest';
import {
	activateParishTarget,
	sortParishTargetsForFocus,
} from './interactions';
import type { ParishHitTarget, ParishRenderCallbacks } from './types';

function callbacks(): ParishRenderCallbacks {
	return {
		onAction: vi.fn(),
		onFocusInput: vi.fn(),
		onOpenSurface: vi.fn(),
		onOpenTab: vi.fn(),
		onSelectNpc: vi.fn(),
		onSend: vi.fn(),
	};
}

function target(
	id: string,
	order: number,
	activation: ParishHitTarget['activation'],
): ParishHitTarget {
	return {
		id,
		order,
		kind: 'card',
		label: id,
		rect: { x: 0, y: 0, width: 44, height: 44 },
		activation,
	};
}

describe('fresh illustrated parish interactions', () => {
	it('keeps keyboard focus order deterministic', () => {
		const sorted = sortParishTargetsForFocus([
			target('later', 20, { type: 'focus-input' }),
			target('alpha', 10, { type: 'focus-input' }),
			target('beta', 10, { type: 'focus-input' }),
		]);
		expect(sorted.map((item) => item.id)).toEqual(['alpha', 'beta', 'later']);
	});

	it('routes a notebook surface without importing the rejected renderer', () => {
		const handlers = callbacks();
		const activated = activateParishTarget(
			target('save', 1, { type: 'open-surface', surface: 'save' }),
			handlers,
		);

		expect(activated).toBe(true);
		expect(handlers.onOpenSurface).toHaveBeenCalledWith('save');
	});

	it('does not activate disabled controls', () => {
		const handlers = callbacks();
		const disabled = target('send', 1, { type: 'send' });
		disabled.disabled = true;

		expect(activateParishTarget(disabled, handlers)).toBe(false);
		expect(handlers.onSend).not.toHaveBeenCalled();
	});
});
