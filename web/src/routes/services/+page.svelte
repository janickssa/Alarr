<script lang="ts">
	// All services grouped by category, as compact rows.
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import EmptyState from '#lib/components/EmptyState.svelte';
	import Icon from '#lib/components/Icon.svelte';
	import SearchField from '#lib/components/SearchField.svelte';
	import SegFilter from '#lib/components/SegFilter.svelte';
	import ServiceRow from '#lib/components/ServiceRow.svelte';
	import { guestId, ms } from '#lib/format.js';
	import { CATEGORY_LABEL, CATEGORY_ORDER } from '#lib/health.js';
	import { session } from '#lib/session.svelte.js';

	let query = $state('');
	let filter = $state<'all' | 'problems' | 'disabled'>('all');
	const category = $derived(page.url.searchParams.get('category'));

	const services = $derived(
		session.services.filter(
			(s) =>
				(!category || s.category === category) &&
				(filter === 'all' ||
					(filter === 'problems' ? s.health === 'down' || s.health === 'warning' : !s.enabled)) &&
				(!query || `${s.name} ${s.id}`.toLowerCase().includes(query.toLowerCase()))
		)
	);
	const groups = $derived(
		CATEGORY_ORDER.map((c) => ({
			id: c,
			label: CATEGORY_LABEL[c],
			items: services.filter((s) => s.category === c)
		})).filter((g) => g.items.length > 0)
	);

	function sub(s: (typeof session.services)[number]) {
		const parts: string[] = [];
		if (!s.enabled) parts.push('Turned off in settings');
		else if (s.health === 'down' || s.health === 'warning') parts.push(s.health_reason ?? '');
		else if (s.response_ms != null) parts.push(ms(s.response_ms));
		if (s.linked_guest) parts.push(guestId(s.linked_guest.guest_type, s.linked_guest.vmid));
		return parts.filter(Boolean).join(' · ');
	}
</script>

<svelte:head><title>Services</title></svelte:head>

<div class="page">
	<div class="page-header">
		<h1 class="page-title">{category ? CATEGORY_LABEL[category] : 'Services'}</h1>
		<span class="spacer"></span>
		{#if session.isOwner}
			<a class="btn btn-secondary" href={resolve('/settings/services')}
				><Icon name="ph-sliders" />Manage</a
			>
			<a class="btn btn-primary" href={resolve('/settings/services/new')}
				><Icon name="ph-plus" />Add service</a
			>
		{/if}
	</div>
	<div class="row wrap">
		<SearchField bind:value={query} placeholder="Filter services" label="Filter services" />
		<SegFilter
			label="Show"
			bind:value={filter}
			options={[
				{ value: 'all', label: 'All', count: session.services.length },
				{
					value: 'problems',
					label: 'Problems',
					count: session.services.filter((s) => s.health === 'down' || s.health === 'warning')
						.length
				},
				{
					value: 'disabled',
					label: 'Disabled',
					count: session.services.filter((s) => !s.enabled).length
				}
			]}
		/>
	</div>

	{#if groups.length === 0}
		<EmptyState
			title={session.services.length === 0 ? 'No services yet' : 'No services match'}
			detail={session.services.length === 0
				? 'Add Proxmox and health checks in Settings.'
				: undefined}
		/>
	{/if}
	{#each groups as g (g.id)}
		<section aria-labelledby="cat-{g.id}">
			<h2 id="cat-{g.id}" class="overline cat">{g.label}</h2>
			<div class="card list">
				{#each g.items as s (s.id)}
					<ServiceRow
						name={s.name}
						sub={sub(s)}
						icon={s.icon}
						hue={s.accent_hue}
						chroma={s.accent_chroma}
						health={s.health}
						href={s.kind === 'proxmox'
							? resolve('/proxmox')
							: resolve('/services/[id]', { id: s.id })}
					/>
				{/each}
			</div>
		</section>
	{/each}
</div>

<style>
	.wrap {
		flex-wrap: wrap;
		gap: 12px;
	}
	.cat {
		margin: 4px 0 8px;
	}
	.list {
		padding: 4px 0;
		gap: 0;
	}
</style>
