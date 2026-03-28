<script lang="ts">
	import { onMount } from 'svelte';
	import { get } from 'svelte/store';
	import StatusBar from '../components/StatusBar.svelte';
	import ChatPanel from '../components/ChatPanel.svelte';
	import MapPanel from '../components/MapPanel.svelte';
	import Sidebar from '../components/Sidebar.svelte';
	import InputField from '../components/InputField.svelte';

	import { worldState, mapData, npcsHere, textLog, streamingActive, loadingSpinner, loadingPhrase, loadingColor, irishHints } from '../stores/game';
	import { palette } from '../stores/theme';
	import {
		getWorldSnapshot,
		getMap,
		getNpcsHere,
		onWorldUpdate,
		onStreamToken,
		onStreamEnd,
		onTextLog,
		onLoading,
		onThemeUpdate
	} from '$lib/ipc';

	onMount(async () => {
		// Initial data fetch
		try {
			const [snap, map, npcs] = await Promise.all([
				getWorldSnapshot(),
				getMap(),
				getNpcsHere()
			]);
			worldState.set(snap);
			mapData.set(map);
			npcsHere.set(npcs);
			// Show initial location description in the chat panel
			if (snap.location_description) {
				textLog.update((log) => [
					...log,
					{ source: 'system', content: snap.location_description }
				]);
			}
		} catch (e) {
			console.warn('Initial fetch failed (expected in browser dev):', e);
		}

		// Subscribe to events
		const unlisten = await Promise.all([
			onWorldUpdate(async (snap) => {
				worldState.set(snap);
				try {
					const [map, npcs] = await Promise.all([getMap(), getNpcsHere()]);
					mapData.set(map);
					npcsHere.set(npcs);
				} catch (_) {}
			}),

			onTextLog((payload) => {
				textLog.update((log) => [
					...log,
					{ source: payload.source, content: payload.content }
				]);
			}),

			onStreamToken((payload) => {
				textLog.update((log) => {
					if (log.length > 0 && log[log.length - 1].streaming) {
						const last = log[log.length - 1];
						return [
							...log.slice(0, -1),
							{ ...last, content: last.content + payload.token }
						];
					}
					// Start new streaming entry
					return [...log, { source: 'NPC', content: payload.token, streaming: true }];
				});
			}),

			onStreamEnd((payload) => {
				// Finalize the streaming entry
				textLog.update((log) => {
					if (log.length > 0 && log[log.length - 1].streaming) {
						const last = log[log.length - 1];
						return [...log.slice(0, -1), { ...last, streaming: false }];
					}
					return log;
				});
				irishHints.set(payload.hints);
				streamingActive.set(false);
			}),

			onLoading((payload) => {
				const wasActive = get(streamingActive);
				streamingActive.set(payload.active);
				if (payload.active) {
					// Update animated loading phrase and spinner
					if (payload.spinner) loadingSpinner.set(payload.spinner);
					if (payload.phrase) loadingPhrase.set(payload.phrase);
					if (payload.color) loadingColor.set(payload.color);
					// Only clean up stale streaming entries on the *first*
					// loading event (transition from inactive → active), not
					// on every animation tick, to avoid erasing in-progress text.
					if (!wasActive) {
						textLog.update((log) => {
							if (log.length > 0 && log[log.length - 1].streaming) {
								return log.slice(0, -1);
							}
							return log;
						});
					}
				}
			}),

			onThemeUpdate((p) => {
				palette.apply(p);
			})
		]);

		return () => {
			unlisten.forEach((fn) => fn());
		};
	});
</script>

<div class="app-shell">
	<StatusBar />
	<div class="main-area">
		<div class="chat-col">
			<ChatPanel />
			<InputField />
		</div>
		<div class="right-col">
			<MapPanel />
			<Sidebar />
		</div>
	</div>
</div>

<style>
	.app-shell {
		display: flex;
		flex-direction: column;
		height: 100vh;
		overflow: hidden;
	}

	.main-area {
		flex: 1;
		display: grid;
		grid-template-columns: 1fr 220px;
		overflow: hidden;
	}

	.chat-col {
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.right-col {
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}
</style>
