<script lang="ts">
	// Standard service card (Home grid). Warning/down gets an outline and the badge.
	import type { ServiceView } from '#lib/api/client.js';
	import { guestId } from '#lib/format.js';
	import HealthBadge from './HealthBadge.svelte';
	import HealthIcon from './HealthIcon.svelte';
	import RelativeTime from './RelativeTime.svelte';
	import ServiceIcon from './ServiceIcon.svelte';

	let {
		service,
		headline,
		detail,
		href
	}: { service: ServiceView; headline: string; detail?: string | null; href?: string } = $props();
	const problem = $derived(service.health === 'warning' || service.health === 'down');
	const edge = $derived(
		service.health === 'down' ? 'var(--health-down-edge)' : 'var(--health-warning-edge)'
	);
</script>

<svelte:element
	this={href ? 'a' : 'div'}
	{href}
	class="card svc"
	class:problem
	style:box-shadow={problem ? `0 0 0 1px ${edge}` : undefined}
>
	<div class="head">
		<ServiceIcon icon={service.icon} hue={service.accent_hue} chroma={service.accent_chroma} />
		<span class="name">{service.name}</span>
		{#if problem}<HealthBadge health={service.health} small />{:else}<HealthIcon
				health={service.health}
			/>{/if}
	</div>
	<div class="main">
		<span class="headline num">{headline}</span>
		{#if detail}
			<span class="detail" style:color={problem ? `var(--health-${service.health})` : undefined}
				>{detail}</span
			>
		{/if}
	</div>
	<div class="foot">
		<span class="mono"
			>{service.linked_guest
				? guestId(service.linked_guest.guest_type, service.linked_guest.vmid)
				: ''}</span
		>
		{#if service.updated_at}<span
				><RelativeTime value={service.updated_at} prefix="Updated " /></span
			>{/if}
	</div>
</svelte:element>

<style>
	.svc {
		color: inherit;
		text-decoration: none;
		min-width: 0;
	}
	a.svc:hover {
		color: inherit;
		background: color-mix(in srgb, var(--color-surface) 92%, var(--color-text));
	}
	.head {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.name {
		font-size: 14px;
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.main {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}
	.headline {
		font-size: 22px;
		font-weight: 500;
		letter-spacing: -0.02em;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.detail {
		font-size: 12.5px;
		color: var(--color-neutral-400);
	}
	.foot {
		display: flex;
		justify-content: space-between;
		font-size: 11px;
		color: var(--color-neutral-500);
		gap: 8px;
	}
</style>
