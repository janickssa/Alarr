<script lang="ts">
	import Icon from './Icon.svelte';

	let {
		label,
		value,
		unit = '',
		trend,
		trendIcon = 'ph-minus',
		tone
	}: {
		label: string;
		value: string | number;
		unit?: string;
		trend?: string;
		trendIcon?: string;
		tone?: string;
	} = $props();
</script>

<div class="stat">
	<span class="label">{label}</span>
	<div class="value-row">
		<span class="value num" style:color={tone}>{value}</span>
		{#if unit}<span class="unit">{unit}</span>{/if}
	</div>
	{#if trend}
		<span class="trend"><Icon name={trendIcon} size={12} />{trend}</span>
	{/if}
</div>

<style>
	.stat {
		background: var(--color-surface);
		border-radius: var(--radius-md);
		padding: 12px 14px;
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 0;
	}
	.label {
		font-size: 11px;
		color: var(--color-neutral-400);
	}
	.value-row {
		display: flex;
		align-items: baseline;
		gap: 4px;
	}
	.value {
		font: var(--text-stat);
		letter-spacing: -0.02em;
	}
	.unit {
		font-size: 13px;
		color: var(--color-neutral-400);
	}
	.trend {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 11.5px;
		color: var(--color-neutral-400);
	}
</style>
