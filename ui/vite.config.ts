import { sveltekit } from '@sveltejs/kit/vite';
import { svelteTesting } from '@testing-library/svelte/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit(), svelteTesting()],
	clearScreen: false,
	server: {
		port: 5173,
		strictPort: true
	},
	test: {
		include: ['src/**/*.test.ts'],
		globals: true,
		environment: 'jsdom',
		setupFiles: ['src/test-setup.ts']
	}
});
