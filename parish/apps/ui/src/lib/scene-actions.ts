import type {
	MapData,
	NpcInfo,
	SceneHotspotAction,
	SceneNpcView,
	TextLogEntry,
} from '$lib/types';

export interface SceneHotspotActionContext {
	mapData: MapData | null;
	submitInput: (text: string) => Promise<void>;
	appendSystemLog: (content: string) => void;
	onError: (content: string) => void;
	sceneNpcs?: SceneNpcView[];
	npcsHere?: NpcInfo[];
	requestNpcFocus?: (displayName: string, realName: string) => void;
}

export interface SceneNpcActionContext {
	npcsHere: NpcInfo[];
	requestNpcFocus: (displayName: string, realName: string) => void;
}

export function sceneTravelCommand(
	action: SceneHotspotAction,
	mapData: MapData | null,
): string | null {
	if (!('travel_to' in action)) return null;
	const targetId = String(action.travel_to);
	const target = mapData?.locations.find((loc) => loc.id === targetId);
	return target ? `go to ${target.name}` : null;
}

export async function activateSceneHotspot(
	action: SceneHotspotAction,
	ctx: SceneHotspotActionContext,
): Promise<void> {
	if ('travel_to' in action) {
		const command = sceneTravelCommand(action, ctx.mapData);
		if (!command) return;
		try {
			await ctx.submitInput(command);
		} catch (err) {
			ctx.onError(`Could not travel: ${formatSceneActionError(err)}`);
		}
		return;
	}

	if ('inspect' in action) {
		const text = action.inspect.trim();
		if (text) ctx.appendSystemLog(text);
		return;
	}

	if ('talk_to' in action) {
		const npc = ctx.sceneNpcs?.find(
			(candidate) => candidate.npc_id === action.talk_to,
		);
		if (!npc || !ctx.npcsHere || !ctx.requestNpcFocus) return;
		activateSceneNpc(npc, {
			npcsHere: ctx.npcsHere,
			requestNpcFocus: ctx.requestNpcFocus,
		});
	}
}

export function sceneNpcRecipient(
	npc: SceneNpcView,
	npcsHere: NpcInfo[],
): { displayName: string; realName: string } {
	const match =
		npcsHere.find((candidate) => candidate.name === npc.display_name) ??
		(npc.real_name
			? npcsHere.find((candidate) => candidate.real_name === npc.real_name)
			: undefined);
	return {
		displayName: npc.display_name,
		realName: match?.real_name ?? npc.real_name ?? npc.display_name,
	};
}

export function activateSceneNpc(
	npc: SceneNpcView,
	ctx: SceneNpcActionContext,
): void {
	const recipient = sceneNpcRecipient(npc, ctx.npcsHere);
	ctx.requestNpcFocus(recipient.displayName, recipient.realName);
}

export function appendSceneInspectLog(
	log: TextLogEntry[],
	content: string,
): TextLogEntry[] {
	return [...log, { source: 'system', subtype: 'scene-inspect', content }];
}

function formatSceneActionError(err: unknown): string {
	if (err instanceof Error) return err.message;
	if (typeof err === 'string') return err;
	return 'unknown error';
}
