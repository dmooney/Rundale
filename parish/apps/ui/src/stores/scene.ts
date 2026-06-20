import { writable } from 'svelte/store';
import type { SceneState } from '$lib/types';

export const sceneState = writable<SceneState | null>(null);

/** True between `travel-start` and the next authoritative world update. */
export const sceneTravelPending = writable<boolean>(false);

export interface SceneNpcFocusRequest {
	request_id: number;
	display_name: string;
	real_name: string;
}

let nextFocusRequestId = 1;

export const sceneNpcFocusRequest = writable<SceneNpcFocusRequest | null>(null);

export function requestSceneNpcFocus(
	displayName: string,
	realName: string,
): void {
	sceneNpcFocusRequest.set({
		request_id: nextFocusRequestId++,
		display_name: displayName,
		real_name: realName,
	});
}

export function clearSceneNpcFocus(requestId: number): void {
	sceneNpcFocusRequest.update((current) =>
		current?.request_id === requestId ? null : current,
	);
}
