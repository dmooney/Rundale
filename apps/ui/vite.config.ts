import { sveltekit } from '@sveltejs/kit/vite';
import { svelteTesting } from '@testing-library/svelte/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
	plugins: [sveltekit(), svelteTesting()],
	clearScreen: false,
	server: {
		port: parseInt(process.env.PARISH_DEV_PORT || '5173'),
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
