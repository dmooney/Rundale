import { describe, expect, it, vi } from 'vitest';
import {
	activateNotebookTarget,
	sortNotebookHitTargetsForFocus,
	type NotebookHitTarget,
} from './interactions';
import type { RenderCallbacks } from './types';

function callbacks(): RenderCallbacks {
	return {
		onAction: vi.fn(),
		onFocusInput: vi.fn(),
		onOpenActiveIntents: vi.fn(),
		onOpenMap: vi.fn(),
		onOpenTab: vi.fn(),
		onOpenTime: vi.fn(),
		onSelectNpc: vi.fn(),
		onSend: vi.fn(),
	};
}

const rect = { x: 0, y: 0, width: 10, height: 10 };

describe('illustrated notebook interactions', () => {
	it('routes targets through the shared activation table', () => {
		const cb = callbacks();

		expect(
			activateNotebookTarget(
				{
					id: 'nearby:roisin',
					kind: 'nearby-portrait',
					label: 'Select Roisin',
					rect,
					order: 10,
					activation: { type: 'select-npc', realName: 'Roisin Connolly' },
				},
				cb,
			),
		).toBe(true);
		expect(cb.onSelectNpc).toHaveBeenCalledWith('Roisin Connolly');

		activateNotebookTarget(
			{
				id: 'tab:people',
				kind: 'tab',
				label: 'Open People',
				rect,
				order: 20,
				activation: { type: 'open-tab', tab: 'people' },
			},
			cb,
		);
		expect(cb.onOpenTab).toHaveBeenCalledWith('people');

		activateNotebookTarget(
			{
				id: 'time-card',
				kind: 'time-card',
				label: 'Open time',
				rect,
				order: 30,
				activation: { type: 'open-time' },
			},
			cb,
		);
		expect(cb.onOpenTime).toHaveBeenCalledOnce();
	});

	it('does not activate disabled send targets', () => {
		const cb = callbacks();

		expect(
			activateNotebookTarget(
				{
					id: 'send',
					kind: 'send',
					label: 'Send intent',
					rect,
					order: 90,
					disabled: true,
					activation: { type: 'send' },
				},
				cb,
			),
		).toBe(false);

		expect(cb.onSend).not.toHaveBeenCalled();
	});

	it('sorts focus targets by explicit notebook order and preserves disabled targets', () => {
		const targets: NotebookHitTarget[] = [
			{
				id: 'send',
				kind: 'send',
				label: 'Send',
				rect,
				order: 90,
				disabled: true,
				activation: { type: 'send' },
			},
			{
				id: 'action:ask',
				kind: 'action-stamp',
				label: 'Ask',
				rect,
				order: 40,
				activation: { type: 'action', action: 'ask' },
			},
			{
				id: 'nearby:roisin',
				kind: 'nearby-portrait',
				label: 'Roisin',
				rect,
				order: 10,
				activation: { type: 'select-npc', realName: 'Roisin Connolly' },
			},
		];

		expect(
			sortNotebookHitTargetsForFocus(targets).map((target) => target.id),
		).toEqual(['nearby:roisin', 'action:ask', 'send']);
	});
});
