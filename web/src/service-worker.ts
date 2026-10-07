/// <reference lib="webworker" />
// Service worker: caches the app shell (the build and static files) so the PWA starts
// instantly and can show "backend unreachable" offline. API responses are never cached:
// requests under /api/ always go to the network, and only same-origin GETs for files from
// this build are served from the cache.
import { version } from '$app/env';
import { assets, immutable } from '$app/manifest';
import { self } from '$app/service-worker';

const sw = self as unknown as ServiceWorkerGlobalScope;
const CACHE = `quorum-${version}`;
const abs = (p: string) => (p.startsWith('/') ? p : `/${p}`);
// Splash screens are only fetched by iOS at install; leave them out of the shell.
const SHELL = [
	'/',
	// Every supported browser uses woff2; skip the legacy font formats.
	...immutable.map((f) => abs(f.path)).filter((p) => !/\.(woff|ttf)$/.test(p)),
	...assets.map((f) => abs(f.path)).filter((p) => !p.startsWith('/splash/'))
];
const SHELL_SET = new Set(SHELL);

sw.addEventListener('install', (event) => {
	event.waitUntil(
		caches
			.open(CACHE)
			.then((cache) => cache.addAll(SHELL))
			.then(() => sw.skipWaiting())
	);
});

sw.addEventListener('activate', (event) => {
	event.waitUntil(
		caches
			.keys()
			.then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
			.then(() => sw.clients.claim())
	);
});

sw.addEventListener('fetch', (event) => {
	const req = event.request;
	if (req.method !== 'GET') return;
	const url = new URL(req.url);
	if (url.origin !== sw.location.origin) return;
	// Never touch the API (live data, secrets, the SSE stream).
	if (url.pathname.startsWith('/api/')) return;

	// Navigations: network first (fresh index), the cached shell when offline. The app then
	// shows its own "can't reach the backend" screen.
	if (req.mode === 'navigate') {
		event.respondWith(fetch(req).catch(async () => (await caches.match('/')) ?? Response.error()));
		return;
	}

	// Build and static files: cache first (they are versioned with the build).
	if (SHELL_SET.has(url.pathname)) {
		event.respondWith(caches.match(url.pathname).then((hit) => hit ?? fetch(req)));
	}
});
