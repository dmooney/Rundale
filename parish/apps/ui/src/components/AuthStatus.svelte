<script lang="ts">
	import { onMount } from 'svelte';

	interface AuthStatus {
		oauth_enabled: boolean;
		logged_in: boolean;
		provider?: string;
		display_name?: string;
	}

	let status = $state<AuthStatus | null>(null);

	onMount(async () => {
		// Skip fetch if in Tauri (no /api server running)
		if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) return;

		try {
			const resp = await fetch('/api/auth/status');
			if (resp.ok) status = await resp.json();
		} catch {
			// Not critical — auth UI is optional
		}
	});
</script>

{#if status?.oauth_enabled}
	<span class="sep">·</span>
	{#if status.logged_in}
		<span class="auth-indicator" title="Signed in with Google — your saves are synced">
			✓ {status.display_name ?? 'Google'}
		</span>
		<span class="sep">·</span>
		<a href="/auth/logout" class="auth-link">Sign out</a>
	{:else}
		<a href="/auth/login/google" class="auth-link">Login with Google</a>
	{/if}
{/if}

<style>
	.auth-indicator {
		color: var(--color-muted);
		font-size: 0.6rem;
		letter-spacing: 0.07em;
		white-space: nowrap;
	}

	.auth-link {
		color: var(--color-muted);
		font-size: 0.6rem;
		letter-spacing: 0.07em;
		text-decoration: none;
		border-bottom: 1px solid var(--color-border);
		transition: color 0.2s, border-color 0.2s;
		white-space: nowrap;
	}

	.auth-link:hover {
		color: var(--color-accent);
		border-color: var(--color-accent);
	}

	.sep {
		color: var(--color-border);
		font-size: 0.7rem;
		letter-spacing: 0;
		opacity: 0.8;
	}
</style>
