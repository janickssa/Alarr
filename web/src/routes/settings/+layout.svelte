<script lang="ts">
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import type { LayoutProps } from './$types';
	import { session } from '#lib/session.svelte.js';

	let { children }: LayoutProps = $props();
	const tabs = $derived(
		[
			{ href: resolve('/settings/services'), label: 'Services', owner: true },
			{ href: resolve('/settings/users'), label: 'Users and devices', owner: true },
			{ href: resolve('/settings/appearance'), label: 'Appearance', owner: true },
			{ href: resolve('/settings/account'), label: 'Account', owner: false },
			{ href: resolve('/settings/backend'), label: 'Backend', owner: false }
		].filter((t) => !t.owner || session.isOwner)
	);
</script>

<div class="page">
	<div class="page-header"><h1 class="page-title">Settings</h1></div>
	<nav class="tabs" aria-label="Settings sections">
		{#each tabs as t (t.href)}
			<a href={t.href} aria-current={page.url.pathname.startsWith(t.href) ? 'page' : undefined}
				>{t.label}</a
			>
		{/each}
	</nav>
	{@render children()}
</div>

<style>
	.tabs {
		display: flex;
		gap: 4px;
		flex-wrap: wrap;
		border-bottom: 1px solid var(--color-divider);
	}
	.tabs a {
		padding: 8px 12px;
		color: var(--color-neutral-300);
		text-decoration: none;
		font-size: 14px;
		border-bottom: 2px solid transparent;
		margin-bottom: -1px;
	}
	.tabs a:hover {
		color: var(--color-text);
	}
	.tabs a[aria-current='page'] {
		color: var(--color-accent-300);
		border-bottom-color: var(--color-accent);
	}
</style>
