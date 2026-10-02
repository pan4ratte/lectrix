import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	preprocess: vitePreprocess(),
	kit: {
		// Tauri serves a static SPA; SSR is disabled in src/routes/+layout.ts.
		adapter: adapter({ fallback: 'index.html' })
	}
};

export default config;
