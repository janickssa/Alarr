<script lang="ts">
	// "More" on phones: the destinations that don't fit the tab bar, every service by
	// category, and the account.
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { alerts } from '#lib/alerts.svelte.js';
	import Icon from '#lib/components/Icon.svelte';
	import ServiceIcon from '#lib/components/ServiceIcon.svelte';
	import { CATEGORY_LABEL, CATEGORY_ORDER, HEALTH } from '#lib/health.js';
	import { session } from '#lib/session.svelte.js';

	const links = $derived([
		{ href: resolve('/health'), label: 'Health', icon: 'ph-heartbeat' },
		{ href: resolve('/alerts'), label: 'Alerts', icon: 'ph-bell', badge: alerts.active },
		{ href: resolve('/activity'), label: 'Activity', icon: 'ph-list-bullets' },
		{ href: resolve('/launcher'), label: 'Launcher', icon: 'ph-squares-four' },
		{ href: resolve('/settings'), label: 'Settings', icon: 'ph-gear-six' }
	]);
	const groups = $derived(
		CATEGORY_ORDER.map((c) => ({
			id: c,
			label: CATEGORY_LABEL[c],
			services: session.services.filter(
				(s) => s.enabled && s.category === c && s.kind !== 'proxmox'
			)
		})).filter((g) => g.services.length > 0)
	);

	async function signOut() {
		await session.signOut();
		await goto(resolve('/login'));
	}
</script>

<svelte:head><title>More</title></svelte:head>

<div class="page">
	<h1 class="page-title">More</h1>
	<ul class="card list">
		{#each links as l (l.label)}
			<li>
				<a href={l.href}>
					<Icon name={l.icon} size={20} />
					<span class="grow">{l.label}</span>
					{#if l.badge}<span class="badge num">{l.badge}</span>{/if}
					<Icon name="ph-caret-right" color="var(--color-neutral-500)" />
				</a>
			</li>
		{/each}
	</ul>

	{#each groups as g (g.id)}
		<section>
			<h2 class="overline">{g.label}</h2>
			<ul class="card list">
				{#each g.services as s (s.id)}
					<li>
						<a href={resolve('/services/[id]', { id: s.id })}>
							<ServiceIcon icon={s.icon} hue={s.accent_hue} chroma={s.accent_chroma} size={28} />
							<span class="grow">{s.name}</span>
							<span
								class="dot"
								style:background={HEALTH[s.health].fg}
								role="img"
								aria-label={HEALTH[s.health].label}
							></span>
							<Icon name="ph-caret-right" color="var(--color-neutral-500)" />
						</a>
					</li>
				{/each}
			</ul>
		</section>
	{/each}

	<section>
		<h2 class="overline">Account</h2>
		<ul class="card list">
			<li class="who">
				<Icon name="ph-user-circle" size={20} />
				<span class="grow">{session.me?.user.username}</span>
				<span class="faint caption"
					>{session.me?.user.role}{session.me?.auth === 'device' ? ' · paired device' : ''}</span
				>
			</li>
			<li>
				<a href={resolve('/settings/account')}
					><Icon name="ph-key" size={20} /><span class="grow">Change password</span></a
				>
			</li>
			<li>
				<button type="button" onclick={signOut}
					><Icon name="ph-sign-out" size={20} /><span class="grow"
						>{session.me?.auth === 'device' ? 'Sign out and unpair' : 'Sign out'}</span
					></button
				>
			</li>
		</ul>
	</section>
</div>

<style>
	.overline {
		margin: 4px 0 8px;
	}
	.list {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.list li + li {
		border-top: 1px solid var(--color-divider);
	}
	.list a,
	.list button,
	.who {
		display: flex;
		align-items: center;
		gap: 12px;
		min-height: var(--row-height);
		padding: 0 14px;
		width: 100%;
		color: var(--color-text);
		text-decoration: none;
		background: none;
		border: 0;
		font: inherit;
		font-size: 15px;
		text-align: left;
		cursor: pointer;
	}
	.grow {
		flex: 1;
		min-width: 0;
	}
	.dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
	}
	.badge {
		min-width: 20px;
		padding: 1px 6px;
		border-radius: 999px;
		font-size: 11px;
		text-align: center;
		background: var(--health-warning-fill);
		color: var(--health-warning);
	}
</style>
