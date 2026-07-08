import type { NotebookAction } from '$lib/notebook/actions';
import type { NotebookRect, NotebookTab, RenderCallbacks } from './types';

export type NotebookHitTargetKind =
	| 'npc-marker'
	| 'nearby-portrait'
	| 'tab'
	| 'action-stamp'
	| 'intent-strip'
	| 'send'
	| 'map-card'
	| 'time-card'
	| 'active-intents-card';

export type NotebookTargetActivation =
	| { type: 'select-npc'; realName: string }
	| { type: 'open-tab'; tab: NotebookTab }
	| { type: 'action'; action: NotebookAction }
	| { type: 'focus-input' }
	| { type: 'send' }
	| { type: 'open-map' }
	| { type: 'open-time' }
	| { type: 'open-active-intents' };

export interface NotebookHitTarget {
	id: string;
	kind: NotebookHitTargetKind;
	label: string;
	rect: NotebookRect;
	activation: NotebookTargetActivation;
	order: number;
	disabled?: boolean;
}

export interface NotebookInteractionState {
	hoveredTargetId: string | null;
	focusedTargetId: string | null;
}

export function notebookHitTarget(
	target: NotebookHitTarget,
): NotebookHitTarget {
	return target;
}

export function sortNotebookHitTargetsForFocus(
	targets: NotebookHitTarget[],
): NotebookHitTarget[] {
	return [...targets]
		.filter((target) => !target.disabled)
		.sort((a, b) => a.order - b.order || a.id.localeCompare(b.id));
}

export function activateNotebookTarget(
	target: NotebookHitTarget | null | undefined,
	callbacks: RenderCallbacks,
): boolean {
	if (!target || target.disabled) return false;
	switch (target.activation.type) {
		case 'select-npc':
			callbacks.onSelectNpc(target.activation.realName);
			return true;
		case 'open-tab':
			callbacks.onOpenTab(target.activation.tab);
			return true;
		case 'action':
			callbacks.onAction(target.activation.action);
			return true;
		case 'focus-input':
			callbacks.onFocusInput();
			return true;
		case 'send':
			callbacks.onSend();
			return true;
		case 'open-map':
			callbacks.onOpenMap();
			return true;
		case 'open-time':
			callbacks.onOpenTime();
			return true;
		case 'open-active-intents':
			callbacks.onOpenActiveIntents();
			return true;
	}
}
