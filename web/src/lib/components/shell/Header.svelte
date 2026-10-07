<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { connection } from '#lib/connection.svelte.js';
	import { page } from '$app/state';
	import { nav, parentOf } from '#lib/nav.svelte.js';
	import { session } from '#lib/session.svelte.js';
	import Icon from '../Icon.svelte';
	import GlobalSearch from './GlobalSearch.svelte';

	let {
		searchInput = $bindable<HTMLInputElement | undefined>()
	}: { searchInput?: HTMLInputElement } = $props();

	let menuOpen = $state(false);
	const pushed = $derived(parentOf(page.url.pathname) != null);

	async function logout() {
		menuOpen = false;
		await session.signOut();
		await goto(resolve('/login'));
	}
</script>

<svelte:window
	onclick={(e) => {
		if (menuOpen && !(e.target as HTMLElement).closest('.user')) menuOpen = false;
	}}
/>

<header class="header">
	{#if pushed}
		<button class="back" type="button" aria-label="Back" onclick={() => nav.back(page.url.pathname)}
			><Icon name="ph-caret-left" size={22} /></button
		>
	{/if}
	<div class="search"><GlobalSearch bind:input={searchInput} /></div>
	<div class="spacer"></div>
	<span
		class="live"
		class:off={connection.live !== 'open'}
		title={connection.live === 'open' ? 'Live updates connected' : 'Live updates reconnecting'}
	>
		<span class="pulse"></span><span class="live-text"
			>{connection.live === 'open' ? 'Live' : 'Reconnecting'}</span
		>
	</span>
	<div class="user">
		<button
			class="btn btn-secondary btn-sm"
			aria-haspopup="menu"
			aria-expanded={menuOpen}
			onclick={() => (menuOpen = !menuOpen)}
		>
			<Icon name="ph-user-circle" size={16} />{session.me?.user.username}
			<span class="faint">{session.me?.user.role}</span>
		</button>
		{#if menuOpen}
			<div class="menu" role="menu">
				<a role="menuitem" href={resolve('/settings/account')} onclick={() => (menuOpen = false)}
					><Icon name="ph-key" />Change password</a
				>
				<button role="menuitem" onclick={logout}><Icon name="ph-sign-out" />Sign out</button>
			</div>
		{/if}
	</div>
</header>

<style>
	.header {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 0 var(--gutter);
		height: var(--header-h);
		position: sticky;
		top: 0;
		z-index: 20;
		background: color-mix(in srgb, var(--color-bg) 92%, transparent);
		backdrop-filter: blur(6px);
	}
	.search {
		min-width: 0;
	}
	.back {
		display: none;
	}
	@media (max-width: 767px) {
		.header {
			gap: 8px;
			padding: var(--safe-top) calc(var(--gutter) + var(--safe-right)) 0
				calc(var(--gutter) + var(--safe-left));
			height: calc(var(--header-h) + var(--safe-top));
		}
		.search {
			flex: 1;
		}
		/* Account lives under More on phones; keep only the live dot. */
		.user,
		.spacer,
		.live-text {
			display: none;
		}
		.back {
			display: grid;
			place-items: center;
			width: 44px;
			height: 44px;
			margin-left: -12px;
			border: 0;
			background: none;
			color: var(--color-accent-300);
			cursor: pointer;
		}
	}
	.live {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--color-neutral-400);
	}
	.pulse {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--health-healthy);
	}
	.off .pulse {
		background: var(--health-warning);
	}
	.user {
		position: relative;
	}
	.menu {
		position: absolute;
		right: 0;
		top: calc(100% + 4px);
		z-index: 30;
		min-width: 190px;
		background: var(--color-surface);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-md);
		padding: 4px;
		display: flex;
		flex-direction: column;
	}
	.menu a,
	.menu button {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 10px;
		border-radius: 6px;
		font-size: 13.5px;
		background: none;
		border: 0;
		color: var(--color-text);
		text-decoration: none;
		cursor: pointer;
		text-align: left;
	}
	.menu a:hover,
	.menu button:hover {
		background: color-mix(in srgb, var(--color-text) 7%, transparent);
	}
</style>
