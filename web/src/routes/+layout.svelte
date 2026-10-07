<script lang="ts">
	import '#lib/styles/fonts.js';
	import '#lib/styles/tokens.css';
	import '#lib/styles/base.css';
	import favicon from '#lib/assets/favicon.svg';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import type { LayoutProps } from './$types';
	import { APP_NAME } from '#lib';
	import { api } from '#lib/api/client.js';
	import Banner from '#lib/components/Banner.svelte';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Logo from '#lib/components/Logo.svelte';
	import Header from '#lib/components/shell/Header.svelte';
	import Sidebar from '#lib/components/shell/Sidebar.svelte';
	import Toaster from '#lib/components/Toaster.svelte';
	import { connection } from '#lib/connection.svelte.js';
	import { time } from '#lib/format.js';
	import { live, refetchOnLive } from '#lib/live.svelte.js';
	import { session } from '#lib/session.svelte.js';

	let { children }: LayoutProps = $props();

	const PUBLIC = ['/login', '/setup', '/pair'];
	const isPublic = $derived(PUBLIC.some((p) => page.url.pathname.startsWith(p)));
	let ready = $state(false);
	let searchInput: HTMLInputElement | undefined = $state();

	async function boot() {
		const me = await session.loadMe().catch(() => null);
		if (!me) {
			ready = true;
			if (!isPublic) {
				const setup = await api.GET('/auth/setup').catch(() => null);
				await goto(resolve(setup?.data?.needs_setup ? '/setup' : '/login'));
			}
			return;
		}
		connection.signedOut = false;
		await session.start(me);
		ready = true;
	}

	onMount(() => {
		void boot();
	});

	// Signed out elsewhere (session expired, device revoked): back to sign-in.
	$effect(() => {
		if (connection.signedOut && !isPublic) {
			live.stop();
			session.clear();
			connection.signedOut = false;
			void goto(resolve('/login'));
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
	<link rel="icon" href={favicon} />
	<title>{APP_NAME}</title>
</svelte:head>

<svelte:window {onkeydown} />

<a class="skip" href="#main">Skip to content</a>

{#if isPublic}
	{@render children()}
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
				{@render children()}
			</main>
		</div>
	</div>
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
