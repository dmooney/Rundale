// @ts-check
import { defineConfig } from 'astro/config';
import svelte from '@astrojs/svelte';

// Static output published to GitHub Pages at https://dmooney.github.io/Rundale.
// `base` must match the repo name so absolute asset URLs resolve under /Rundale.
export default defineConfig({
  site: 'https://dmooney.github.io',
  base: '/Rundale',
  integrations: [svelte()],
});
