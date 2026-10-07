<script lang="ts">
	// Phone bottom navigation: the 5 destinations (Home, Proxmox, Media, Services, More).
	// Proxmox and Media only appear when configured; everything else lives under More.
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { alerts } from '#lib/alerts.svelte.js';
	import { session } from '#lib/session.svelte.js';
	import Icon from '../Icon.svelte';

	const path = $derived(page.url.pathname);
	const hasMedia = $derived(session.services.some((s) => s.enabled && s.category === 'media'));
	const MORE = ['/more', '/health', '/alerts', '/activity', '/settings', '/launcher'];

	const tabs = $derived(
		[
			{ href: resolve('/'), label: 'Home', icon: 'ph-house', show: true, on: path === '/' },
			{
				href: resolve('/proxmox'),
				label: 'Proxmox',
				icon: 'ph-hard-drives',
				show: !!session.proxmox,
				on: path.startsWith('/proxmox')
			},
			{
				href: resolve('/media'),
				label: 'Media',
				icon: 'ph-film-strip',
				show: hasMedia,
				on: path.startsWith('/media')
			},
			{
				href: resolve('/services'),
				label: 'Services',
				icon: 'ph-squares-four',
				show: true,
				on: path.startsWith('/services')
			},
			{
				href: resolve('/more'),
				label: 'More',
				icon: 'ph-dots-three-outline',
				show: true,
				on: MORE.some((m) => path.startsWith(m)),
				badge: alerts.active
			}
		].filter((t) => t.show)
	);
</script>

<nav class="tabbar" aria-label="Main">
	{#each tabs as t (t.label)}
		<a href={t.href} aria-current={t.on ? 'page' : undefined}>
			<span class="ico">
				<Icon name={t.icon} size={22} fill={t.on} />
				{#if t.badge}<span class="badge num" aria-label="{t.badge} active alerts">{t.badge}</span
					>{/if}
			</span>
			<span class="lbl">{t.label}</span>
		</a>
	{/each}
</nav>

<style>
	.tabbar {
		display: none;
	}
	@media (max-width: 767px) {
		.tabbar {
			display: flex;
			position: fixed;
			left: 0;
			right: 0;
			bottom: 0;
			z-index: 30;
			height: calc(var(--tabbar-h) + var(--safe-bottom));
			padding: 0 var(--safe-right) var(--safe-bottom) var(--safe-left);
			background: color-mix(in srgb, var(--color-bg) 94%, transparent);
			backdrop-filter: blur(10px);
			border-top: 1px solid color-mix(in srgb, var(--color-text) 8%, transparent);
		}
	}
	a {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 2px;
		min-height: 44px;
		color: var(--color-neutral-400);
		text-decoration: none;
		-webkit-tap-highlight-color: transparent;
	}
	a[aria-current='page'] {
		color: var(--color-accent-300);
	}
	.ico {
		position: relative;
		display: grid;
		place-items: center;
	}
	.lbl {
		font-size: 10.5px;
	}
	.badge {
		position: absolute;
		top: -4px;
		left: 14px;
		min-width: 16px;
		padding: 0 4px;
		border-radius: 999px;
		font-size: 10px;
		line-height: 16px;
		text-align: center;
		background: var(--health-warning);
		color: var(--color-bg);
	}
</style>
