import { tiles, currentTileSource } from '../../stores/tiles';
import type { TileSource } from '$lib/types';

/**
 * Subscribes to the shared `tiles` store and pushes the active tile
 * source into `controller` whenever the selection changes.
 *
 * Returns an unsubscribe function. Call in `onMount` and invoke from
 * the returned cleanup so the subscription is torn down with the
 * component.
 */
export function subscribeTileSource(
	getController: () => {
		setTileSource: (source: TileSource | undefined) => void;
	} | null,
): () => void {
	return tiles.subscribe((state) => {
		const controller = getController();
		if (controller) {
			controller.setTileSource(currentTileSource(state));
		}
	});
}
