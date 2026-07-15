import type { ParishHitTarget, ParishRenderCallbacks } from './types';

export function sortParishTargetsForFocus(
	targets: ParishHitTarget[],
): ParishHitTarget[] {
	return [...targets].sort(
		(a, b) => a.order - b.order || a.id.localeCompare(b.id),
	);
}

export function activateParishTarget(
	target: ParishHitTarget | null | undefined,
	callbacks: ParishRenderCallbacks,
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
		case 'open-surface':
			callbacks.onOpenSurface(target.activation.surface);
			return true;
	}
}
