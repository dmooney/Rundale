/**
 * IPC transport layer — works in both Tauri (desktop) and browser (web server).
 *
 * In Tauri mode, uses `@tauri-apps/api` invoke/listen.
 * In browser mode, uses fetch for commands and WebSocket for events.
 * All exported function signatures are identical regardless of transport.
 *
 * This module is a stable barrel (#1200 TD-054): the single transport adapter
 * lives in `ipc/transport.ts` and the command families in `ipc/{game,save,
 * setup,provider,screenshot}.ts`. Everything is re-exported here so existing
 * `import { … } from '$lib/ipc'` paths are unchanged.
 */

export {
	command,
	onEvent,
	onReconnect,
	disposeTransport,
	isTauri,
	IS_TAURI,
	COMMAND_TIMEOUT_MS,
	type UnlistenFn,
} from './ipc/transport';

export * from './ipc/game';
export * from './ipc/save';
export * from './ipc/scene';
export * from './ipc/screenshot';
export * from './ipc/setup';
export * from './ipc/provider';
