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
		// Port priority: PARISH_DEV_PORT (project-specific override) > PORT
		// (set by harnesses such as Claude Code Desktop's `autoPort` and other
		// PaaS-style runners) > the conventional Vite default 5173.
		port:
			parseInt(process.env.PARISH_DEV_PORT || process.env.PORT || '5173', 10) ||
			5173,
		strictPort: true,
		fs: {
			allow: ['.']
		},
		proxy: {
			'/api': {
				target: `http://localhost:${process.env.PARISH_WEB_PORT || '3001'}`,
				ws: true
			}
		}
	},
	test: {
		include: ['src/**/*.test.ts'],
		globals: true,
		environment: 'jsdom',
		setupFiles: ['src/test-setup.ts']
	}
});
