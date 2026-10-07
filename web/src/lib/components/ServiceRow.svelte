<script lang="ts">
	// Compact list row: icon tile, name + sub line, health badge, caret.
	import type { Health } from '#lib/api/client.js';
	import HealthBadge from './HealthBadge.svelte';
	import Icon from './Icon.svelte';
	import ServiceIcon from './ServiceIcon.svelte';

	let {
		name,
		sub,
		icon,
		hue,
		chroma,
		health,
		href,
		suffix
	}: {
		name: string;
		sub?: string;
		icon: string;
		hue: number;
		chroma?: number;
		health: Health;
		href?: string;
		suffix?: string;
	} = $props();
</script>

<svelte:element this={href ? 'a' : 'div'} {href} class="row-item">
	<ServiceIcon {icon} {hue} {chroma} />
	<div class="txt">
		<span class="name"
			>{name}{#if suffix}<span class="tag tag-neutral suffix">{suffix}</span>{/if}</span
		>
		{#if sub}<span class="sub">{sub}</span>{/if}
	</div>
	<HealthBadge {health} small />
	{#if href}<Icon name="ph-caret-right" color="var(--color-neutral-500)" />{/if}
</svelte:element>

<style>
	.row-item {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 0 14px;
		min-height: 52px;
		color: inherit;
		text-decoration: none;
	}
	a.row-item:hover {
		color: inherit;
		background: color-mix(in srgb, var(--color-text) 4%, transparent);
	}
	.txt {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}
	.name {
		font-size: 14px;
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.suffix {
		font-size: 10px;
		padding: 0 6px;
	}
	.sub {
		font-size: 12px;
		color: var(--color-neutral-400);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
</style>
