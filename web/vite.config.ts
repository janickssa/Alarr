import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, type Plugin } from 'vite';

// Phosphor's CSS lists woff2, woff, ttf and svg fonts and Vite would bundle all four
// (~7 MB). Every supported browser uses woff2, so keep only that.
function phosphorWoff2Only(): Plugin {
	return {
		name: 'phosphor-woff2-only',
		enforce: 'pre',
		transform(code, id) {
			if (!/@phosphor-icons[\\/]web[\\/].*\.css/.test(id)) return null;
			return code.replace(
				/src:\s*(url\([^)]+\.woff2[^)]*\)\s*format\(["']woff2["']\))[^;]*;/g,
				'src: $1;'
			);
		}
	};
}

// `npm run dev` proxies the API to a running quorum-server (default: `cargo run -- --demo`).
const backend = process.env.QUORUM_DEV_BACKEND ?? 'http://127.0.0.1:8080';

export default defineConfig({
	plugins: [
		phosphorWoff2Only(),
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			// SPA: the Rust server serves index.html for every non-API route.
			adapter: adapter({ fallback: 'index.html' })
		})
	],
	server: {
		proxy: {
			'/api': { target: backend, changeOrigin: false }
		}
	}
});
