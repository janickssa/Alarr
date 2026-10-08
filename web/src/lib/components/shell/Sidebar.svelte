<script lang="ts">
	// Left sidebar: the destinations, then enabled services grouped by category.
	// A category only appears when it has at least one enabled service.
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { CATEGORY_LABEL, CATEGORY_ORDER, HEALTH } from '#lib/health.js';
	import { alerts } from '#lib/alerts.svelte.js';
	import { session } from '#lib/session.svelte.js';
	import Icon from '../Icon.svelte';
	import Logo from '../Logo.svelte';
	import ServiceIcon from '../ServiceIcon.svelte';

	const path = $derived(page.url.pathname);
	const enabled = $derived(session.services.filter((s) => s.enabled));
	const hasMedia = $derived(enabled.some((s) => s.category === 'media'));

	const nav = $derived(
		[
			{
				href: resolve('/'),
				label: 'Home',
				icon: 'ph-house',
				show: true,
				match: (p: string) => p === '/'
			},
			{
				href: resolve('/proxmox'),
				label: 'Proxmox',
				icon: 'ph-hard-drives',
				show: !!session.proxmox,
				match: (p: string) => p.startsWith('/proxmox')
			},
			{
				href: resolve('/media'),
				label: 'Media',
				icon: 'ph-film-strip',
				show: hasMedia,
				match: (p: string) => p.startsWith('/media')
			},
			{
				href: resolve('/services'),
				label: 'Services',
				icon: 'ph-squares-four',
				show: true,
				match: (p: string) => p.startsWith('/services')
			},
			{
				href: resolve('/health'),
				label: 'Health',
				icon: 'ph-heartbeat',
				show: true,
				match: (p: string) => p.startsWith('/health')
			},
			{
				href: resolve('/alerts'),
				label: 'Alerts',
				icon: 'ph-bell',
				show: true,
				badge: alerts.active,
				match: (p: string) => p.startsWith('/alerts')
			},
			{
				href: resolve('/activity'),
				label: 'Activity',
				icon: 'ph-list-bullets',
				show: true,
				match: (p: string) => p.startsWith('/activity')
			},
			{
				href: resolve('/settings'),
				label: 'Settings',
				icon: 'ph-gear-six',
				show: true,
				match: (p: string) => p.startsWith('/settings')
			}
		].filter((n) => n.show)
	);

	const groups = $derived(
		CATEGORY_ORDER.map((c) => ({
			id: c,
			label: CATEGORY_LABEL[c],
			services: enabled.filter((s) => s.category === c && s.kind !== 'proxmox')
		})).filter((g) => g.services.length > 0)
	);
</script>

<nav class="sidebar" aria-label="Main sidebar">
	<a class="brand" href={resolve('/')}><Logo /></a>
	<ul class="nav">
		{#each nav as n (n.label)}
			<li>
				<a href={n.href} aria-current={n.match(path) ? 'page' : undefined}>
					<Icon name={n.icon} size={18} /><span class="lbl">{n.label}</span>
					{#if 'badge' in n && n.badge}<span class="badge num" aria-label="{n.badge} active"
							>{n.badge}</span
						>{/if}
				</a>
			</li>
		{/each}
	</ul>
	{#each groups as g (g.id)}
		<div class="group">
			<div class="overline">{g.label}</div>
			<ul>
				{#each g.services as s (s.id)}
					<li>
						<a
							href={resolve('/services/[id]', { id: s.id })}
							aria-current={path === `/services/${s.id}` ? 'page' : undefined}
						>
							<ServiceIcon icon={s.icon} hue={s.accent_hue} chroma={s.accent_chroma} size={22} />
							<span class="svc-name">{s.name}</span>
							<span
								class="dot"
								style:background={HEALTH[s.health].fg}
								title={HEALTH[s.health].label}
								aria-label={HEALTH[s.health].label}
								role="img"
							></span>
						</a>
					</li>
				{/each}
			</ul>
		</div>
	{/each}
</nav>

<style>
	.badge {
		margin-left: auto;
		min-width: 20px;
		padding: 1px 6px;
		border-radius: 999px;
		font-size: 11px;
		text-align: center;
		background: var(--health-warning-fill);
		color: var(--health-warning);
	}
	.sidebar {
		width: 232px;
		flex: none;
		height: 100vh;
		position: sticky;
		top: 0;
		overflow-y: auto;
		padding: 18px 12px 24px;
		display: flex;
		flex-direction: column;
		gap: 18px;
		border-right: 1px solid color-mix(in srgb, var(--color-text) 6%, transparent);
	}
	.brand {
		color: inherit;
		text-decoration: none;
		padding: 0 8px;
	}
	ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	a {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 0 8px;
		min-height: 34px;
		border-radius: var(--radius-md);
		color: var(--color-neutral-300);
		text-decoration: none;
		font-size: 14px;
	}
	a:hover {
		color: var(--color-text);
		background: color-mix(in srgb, var(--color-text) 5%, transparent);
	}
	a[aria-current='page'] {
		color: var(--color-accent-300);
		background: color-mix(in srgb, var(--color-accent) 12%, transparent);
	}
	.group {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.group .overline {
		padding: 0 8px;
		font-size: 10.5px;
	}
	.group a {
		min-height: 32px;
		font-size: 13px;
	}
	.svc-name {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	/* Phones use the tab bar; tablets get an icon rail. */
	@media (max-width: 767px) {
		.sidebar {
			display: none;
		}
	}
	@media (min-width: 768px) and (max-width: 1023px) {
		.sidebar {
			width: 68px;
			padding: 18px 10px 24px;
			align-items: center;
		}
		.sidebar :global(.name),
		.lbl,
		.group {
			display: none;
		}
		.nav a {
			justify-content: center;
			width: 44px;
			min-height: 44px;
			padding: 0;
			position: relative;
		}
		.badge {
			position: absolute;
			top: 2px;
			right: 0;
			min-width: 16px;
			padding: 0 4px;
			font-size: 10px;
		}
	}
	.dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		flex: none;
	}
</style>
