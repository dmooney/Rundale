<script lang="ts">
	import '../app.css';
	import { onMount, onDestroy } from 'svelte';
	import { captureScreen } from '$lib/screenshot';
	import {
		onRequestScreenshot,
		saveScreenshot,
		notifyScreenshotCaptured,
		notifyScreenshotError
	} from '$lib/ipc';

	let { children } = $props();

	// Register the MCP-triggered screenshot listener at layout level so it
	// stays active regardless of which route is currently mounted (gameplay
	// page, editor, etc.). Without this, navigating to /editor before an
	// agent calls parish_take_screenshot causes a guaranteed 15-second timeout.
	let unlistenScreenshot: (() => void) | null = null;

	onMount(async () => {
		unlistenScreenshot = await onRequestScreenshot(async (payload) => {
			try {
				const dataUrl = await captureScreen();
				const info = await saveScreenshot(dataUrl);
				await notifyScreenshotCaptured(payload.request_id, info);
			} catch (e) {
				const msg = e instanceof Error ? e.message : String(e);
				console.warn('Agent screenshot capture failed:', msg);
				await notifyScreenshotError(payload.request_id, msg).catch(() => {});
			}
		});
	});

	onDestroy(() => {
		unlistenScreenshot?.();
	});
</script>

{@render children()}
