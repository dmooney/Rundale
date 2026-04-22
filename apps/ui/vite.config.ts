import { sveltekit } from '@sveltejs/kit/vite';
import { svelteTesting } from '@testing-library/svelte/vite';
import { defineConfig } from 'vitest/config';

// Minimal local declaration so TypeScript accepts `process.env` in this Node-only
// config file without pulling in `@types/node` as a project-wide dependency.
declare const process: { env: Record<string, string | undefined> };

export default defineConfig({
	plugins: [sveltekit(), svelteTesting()],
	clearScreen: false,
	server: {
		port: parseInt(process.env.PARISH_DEV_PORT || '5173', 10) || 5173,
		strictPort: true,
		fs: {
			allow: ['.']
		}
	},
	test: {
		include: ['src/**/*.test.ts'],
		globals: true,
		environment: 'jsdom',
		setupFiles: ['src/test-setup.ts']
	}
});
