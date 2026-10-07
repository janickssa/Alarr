<script lang="ts">
	// Health-check card: response time, 7 d uptime and an Open button.
	import type { ServiceView } from '#lib/api/client.js';
	import HealthBadge from './HealthBadge.svelte';
	import HealthIcon from './HealthIcon.svelte';
	import Icon from './Icon.svelte';
	import ServiceIcon from './ServiceIcon.svelte';

	let {
		service,
		uptime7d,
		href
	}: { service: ServiceView; uptime7d?: string | null; href?: string } = $props();
	const problem = $derived(service.health === 'warning' || service.health === 'down');
</script>

<div
	class="card"
	style:box-shadow={problem
		? `0 0 0 1px var(--health-${service.health === 'down' ? 'down' : 'warning'}-edge)`
		: undefined}
>
	<svelte:element this={href ? 'a' : 'div'} {href} class="head">
		<ServiceIcon icon={service.icon} hue={service.accent_hue} chroma={service.accent_chroma} />
		<span class="name">{service.name}</span>
		{#if problem}<HealthBadge health={service.health} small />{:else}<HealthIcon
				health={service.health}
			/>{/if}
	</svelte:element>
	<div class="value">
		{#if service.health === 'down'}
			<span class="reason">{service.health_reason ?? 'Down'}</span>
		{:else}
			<span class="big num">{service.response_ms ?? '–'}</span><span class="unit">ms</span>
		{/if}
		{#if uptime7d}<span class="uptime">{uptime7d}</span>{/if}
	</div>
	{#if service.base_url}
		<a
			class="btn btn-secondary btn-sm open"
			href={service.base_url}
			target="_blank"
			rel="noreferrer noopener"
		>
			<Icon name="ph-arrow-square-out" />Open
		</a>
	{/if}
</div>

<style>
	.head {
		display: flex;
		align-items: center;
		gap: 10px;
		color: inherit;
		text-decoration: none;
	}
	a.head:hover .name {
		color: var(--color-accent-300);
	}
	.name {
		font-size: 14px;
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.value {
		display: flex;
		align-items: baseline;
		gap: 4px;
		min-height: 30px;
	}
	.big {
		font-size: 22px;
		font-weight: 500;
		letter-spacing: -0.02em;
	}
	.unit {
		font-size: 13px;
		color: var(--color-neutral-400);
	}
	.reason {
		font-size: 13px;
		color: var(--health-down);
	}
	.uptime {
		font-size: 12px;
		color: var(--color-neutral-500);
		margin-left: auto;
		white-space: nowrap;
	}
	.open {
		justify-content: center;
	}
</style>
