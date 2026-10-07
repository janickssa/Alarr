<script lang="ts">
	// Large card: header, a row of stats and a mini chart.
	import type { Snippet } from 'svelte';
	import type { ServiceView } from '#lib/api/client.js';
	import HealthBadge from './HealthBadge.svelte';
	import ServiceIcon from './ServiceIcon.svelte';
	import Sparkline from './Sparkline.svelte';

	let {
		service,
		stats,
		chart,
		chartLabel,
		href,
		badge
	}: {
		service: ServiceView;
		stats: { label: string; value: string; unit?: string }[];
		chart?: number[];
		chartLabel?: string;
		href?: string;
		badge?: Snippet;
	} = $props();
</script>

<svelte:element this={href ? 'a' : 'div'} {href} class="card large">
	<div class="left">
		<div class="head">
			<ServiceIcon icon={service.icon} hue={service.accent_hue} chroma={service.accent_chroma} />
			<span class="name">{service.name}</span>
			{#if badge}{@render badge()}{:else}<HealthBadge health={service.health} small />{/if}
		</div>
		<div class="stats">
			{#each stats as s (s.label)}
				<div class="stat">
					<span class="lbl">{s.label}</span>
					<span class="v num"
						>{s.value}
						{#if s.unit}<span class="u">{s.unit}</span>{/if}</span
					>
				</div>
			{/each}
		</div>
	</div>
	{#if chart && chart.length > 1}
		<div class="chart">
			<Sparkline values={chart} width={260} height={64} label={chartLabel} />
			{#if chartLabel}<span class="mono faint cl">{chartLabel}</span>{/if}
		</div>
	{/if}
</svelte:element>

<style>
	.large {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		gap: 20px;
		align-items: center;
		padding: 16px;
		color: inherit;
		text-decoration: none;
		grid-column: span 2;
	}
	a.large:hover {
		color: inherit;
		background: color-mix(in srgb, var(--color-surface) 92%, var(--color-text));
	}
	.left {
		display: flex;
		flex-direction: column;
		gap: 12px;
		min-width: 0;
	}
	.head {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.name {
		font-size: 14px;
	}
	.stats {
		display: flex;
		gap: 24px;
		flex-wrap: wrap;
	}
	.stat {
		display: flex;
		flex-direction: column;
	}
	.lbl {
		font-size: 11px;
		color: var(--color-neutral-400);
	}
	.v {
		font-size: 22px;
		font-weight: 500;
		white-space: nowrap;
	}
	.u {
		font-size: 13px;
		color: var(--color-neutral-400);
	}
	.chart {
		display: flex;
		flex-direction: column;
		gap: 4px;
		width: 260px;
	}
	.cl {
		font-size: 10px;
	}
	@media (max-width: 900px) {
		.large {
			grid-column: auto;
			grid-template-columns: 1fr;
		}
		.chart {
			width: 100%;
		}
	}
</style>
