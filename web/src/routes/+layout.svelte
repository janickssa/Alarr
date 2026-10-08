<script lang="ts">
	import '#lib/styles/fonts.js';
	import '#lib/styles/tokens.css';
	import '#lib/styles/base.css';
	import { afterNavigate, goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import type { LayoutProps } from './$types';
	import { APP_NAME } from '#lib';
	import { api } from '#lib/api/client.js';
	import Banner from '#lib/components/Banner.svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import Logo from '#lib/components/Logo.svelte';
	import Header from '#lib/components/shell/Header.svelte';
	import Sidebar from '#lib/components/shell/Sidebar.svelte';
	import PullToRefresh from '#lib/components/shell/PullToRefresh.svelte';
	import TabBar from '#lib/components/shell/TabBar.svelte';
	import Toaster from '#lib/components/Toaster.svelte';
	import { connection } from '#lib/connection.svelte.js';
	import { time } from '#lib/format.js';
	import { live, refetchOnLive } from '#lib/live.svelte.js';
	import { nav, parentOf } from '#lib/nav.svelte.js';
	import { ConnectError, session } from '#lib/session.svelte.js';

	let { children }: LayoutProps = $props();

	afterNavigate((n) => {
		if (n.type === 'enter') return;
		if (n.type === 'popstate') nav.depth = Math.max(0, nav.depth - (n.delta ?? 1));
		else nav.depth++;
	});

	// Edge swipe (phones): from the left edge to the right goes back on pushed pages.
	let swipe: { x: number; y: number } | null = null;
	function ontouchstart(e: TouchEvent) {
		const t = e.touches[0];
		swipe =
			t.clientX < 24 && window.innerWidth < 768 && parentOf(page.url.pathname)
				? { x: t.clientX, y: t.clientY }
				: null;
	}
	function ontouchend(e: TouchEvent) {
		if (!swipe) return;
		const t = e.changedTouches[0];
		const dx = t.clientX - swipe.x;
		const dy = Math.abs(t.clientY - swipe.y);
		swipe = null;
		if (dx > 80 && dy < 50) nav.back(page.url.pathname);
	}

	const PUBLIC = ['/login', '/setup', '/pair'];
	const isPublic = $derived(PUBLIC.some((p) => page.url.pathname.startsWith(p)));
	let ready = $state(false);
	let searchInput: HTMLInputElement | undefined = $state();

	/**
	 * Why the app can't show anything right now. Once this browser has been signed in it
	 * never goes to the sign-in page by itself: it shows this with "Try again" and
	 * "Log out" until it gets in again or you log out.
	 */
	type Problem = { title: string; reason: string };
	let problem = $state<Problem | null>(null);
	let retry: ReturnType<typeof setTimeout> | undefined;

	function describe(e: unknown): Problem {
		const host = page.url.host;
		const status = e instanceof ConnectError ? e.status : undefined;
		if (!status)
			return {
				title: "Can't connect",
				reason: `No answer from ${host}. Usually the VPN (WireGuard) isn't connected yet, or the server is down.`
			};
		if (status === 502 || status === 503 || status === 504)
			return {
				title: "Can't connect",
				reason: `${host} answered HTTP ${status}: the proxy can't reach Quorum. Is the Quorum container running?`
			};
		return { title: "Can't connect", reason: `${host} answered HTTP ${status}.` };
	}

	const NOT_ACCEPTED: Problem = {
		title: 'Sign-in not accepted',
		reason:
			"The server didn't accept this device's sign-in. It may have been unpaired or signed out from another device, or the server's database was reset. Log out and sign in again to continue."
	};

	async function boot() {
		clearTimeout(retry);
		let me;
		try {
			me = await session.loadMe();
		} catch (e) {
			// Not the same as signed out: keep the sign-in and try again shortly (the VPN is
			// often still connecting when the app is opened).
			problem = describe(e);
			ready = true;
			retry = setTimeout(() => void boot(), 3000);
			return;
		}
		if (!me) {
			ready = true;
			if (session.remembered && !isPublic) {
				problem = NOT_ACCEPTED;
				retry = setTimeout(() => void boot(), 15000);
				return;
			}
			if (!isPublic) {
				const setup = await api.GET('/auth/setup').catch(() => null);
				await goto(resolve(setup?.data?.needs_setup ? '/setup' : '/login'));
			}
			return;
		}
		problem = null;
		connection.signedOut = false;
		await session.start(me);
		ready = true;
	}

	async function logOut() {
		clearTimeout(retry);
		await session.signOut();
		problem = null;
		await goto(resolve('/login'));
	}

	onMount(() => {
		void boot();
	});

	// A request got 401 while using the app (session expired, device revoked).
	$effect(() => {
		if (connection.signedOut && !isPublic) {
			connection.signedOut = false;
			live.stop();
			if (session.remembered) {
				problem = NOT_ACCEPTED;
				retry = setTimeout(() => void boot(), 15000);
			} else {
				session.clear();
				void goto(resolve('/login'));
			}
		}
	});

	// Keep the sidebar's service list and health dots current.
	refetchOnLive(
		() => [],
		() => (session.me ? session.loadServices() : Promise.resolve()),
		{ health: true, actions: true, minInterval: 1500 }
	);

	// Keyboard shortcuts: "/" search, "g h" Home, "g p" Proxmox, "g s" Services,
	// "g e" Health, "g l" Alerts, "g a" Activity, "g o" Launcher, "g ," Settings.
	let pendingG = false;
	let gTimer: ReturnType<typeof setTimeout> | undefined;
	function onkeydown(e: KeyboardEvent) {
		const t = e.target as HTMLElement;
		if (e.metaKey || e.ctrlKey || e.altKey) return;
		if (t.closest('input, textarea, select, [contenteditable="true"]')) return;
		if (document.querySelector('dialog[open]')) return;
		if (e.key === '/') {
			e.preventDefault();
			searchInput?.focus();
			return;
		}
		if (pendingG) {
			pendingG = false;
			clearTimeout(gTimer);
			const routes = {
				h: '/',
				p: '/proxmox',
				s: '/services',
				e: '/health',
				l: '/alerts',
				o: '/launcher',
				a: '/activity',
				',': '/settings'
			} as const;
			const target = routes[e.key as keyof typeof routes];
			if (target) {
				e.preventDefault();
				void goto(resolve(target));
			}
			return;
		}
		if (e.key === 'g') {
			pendingG = true;
			gTimer = setTimeout(() => (pendingG = false), 1200);
		}
	}
</script>

<svelte:head>
	<title>{APP_NAME}</title>
</svelte:head>

<svelte:window {onkeydown} {ontouchstart} {ontouchend} />

<a class="skip" href="#main">Skip to content</a>

{#if isPublic}
	{@render children()}
{:else if problem}
	<div class="splash" role="alert">
		<Logo size={56} />
		<h1 class="off-title">{problem.title}</h1>
		<p class="off-text">{problem.reason}</p>
		<p class="off-text faint">You stay signed in until you log out. Retrying by itself.</p>
		<div class="off-actions">
			<button
				class="btn btn-primary"
				onclick={() => {
					ready = false;
					problem = null;
					void boot();
				}}><Icon name="ph-arrow-clockwise" />Try again</button
			>
			<button class="btn btn-secondary" onclick={logOut}><Icon name="ph-sign-out" />Log out</button>
		</div>
	</div>
{:else if !ready || !session.me}
	<div class="splash">
		<Logo size={56} />
		<span class="mono faint">Connecting to {page.url.host}…</span>
	</div>
{:else}
	<div class="shell">
		<Sidebar />
		<div class="main-col">
			<Header bind:searchInput />
			{#if !connection.reachable}
				<div class="conn">
					<Banner
						tone="down"
						text="Can't reach the backend.{connection.lastSeen
							? ` Showing data from ${time(connection.lastSeen)}.`
							: ''} Are you on Tailscale?"
					>
						<button class="btn btn-secondary" onclick={() => location.reload()}>Retry</button>
					</Banner>
				</div>
			{/if}
			<main id="main" tabindex="-1">
				<PullToRefresh />
				{@render children()}
			</main>
		</div>
	</div>
	<TabBar />
	<Toaster />
	<ConfirmDialog />
{/if}

<style>
	.shell {
		display: flex;
		min-height: 100vh;
	}
	.main-col {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}
	main {
		flex: 1;
		min-width: 0;
		padding-bottom: calc(var(--tabbar-h) + var(--safe-bottom));
		padding-left: var(--safe-left);
		padding-right: var(--safe-right);
	}
	main:focus {
		outline: none;
	}
	.conn {
		padding: 0 var(--gutter);
	}
	.splash {
		min-height: 100vh;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 18px;
		background: radial-gradient(60% 40% at 50% 44%, #262a60 0%, #161826 70%);
	}
	.off-title {
		font: var(--text-title);
	}
	.off-actions {
		display: flex;
		gap: 10px;
	}
	.off-text {
		max-width: 320px;
		text-align: center;
		color: var(--color-neutral-300);
		font-size: 14px;
		margin: 0 16px;
	}
	.skip {
		position: absolute;
		left: -9999px;
	}
	.skip:focus {
		left: 12px;
		top: 12px;
		z-index: 100;
		background: var(--color-surface);
		padding: 8px 12px;
		border-radius: var(--radius-md);
	}
</style>
