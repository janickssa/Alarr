<script lang="ts">
	// Resource bar: neutral below 80 %, amber + warning icon 80–89 %, red + error icon ≥ 90 %.
	import { usageLevel } from '#lib/health.js';
	import Icon from './Icon.svelte';

	let {
		label,
		fraction,
		value,
		compact = false
	}: {
		label?: string;
		fraction: number | null | undefined;
		value?: string;
		compact?: boolean;
	} = $props();
	const level = $derived(usageLevel(fraction));
	const color = $derived(
		level === 'down'
			? 'var(--health-down)'
			: level === 'warning'
				? 'var(--health-warning)'
				: 'var(--color-neutral-300)'
	);
	const width = $derived(`${Math.min(100, Math.max(0, (fraction ?? 0) * 100))}%`);
</script>

<div class="bar-wrap" class:compact>
	{#if label || value}
		<div class="top">
			<span class="lbl">
				{label}
				{#if level !== 'ok'}
					<Icon
						name={level === 'down' ? 'ph-x-circle' : 'ph-warning'}
						fill
						size={13}
						{color}
						label={level === 'down' ? 'Critical' : 'High'}
					/>
				{/if}
			</span>
			{#if value}<span class="val mono num">{value}</span>{/if}
		</div>
	{/if}
	<div
		class="track"
		role="meter"
		aria-label={label ?? 'Usage'}
		aria-valuemin={0}
		aria-valuemax={100}
		aria-valuenow={fraction == null ? undefined : Math.round(fraction * 100)}
	>
		<div class="fill" style:width style:background={color}></div>
		{#if !compact}
			<div class="tick" style:left="80%"></div>
			<div class="tick" style:left="90%"></div>
		{/if}
	</div>
</div>

<style>
	.bar-wrap {
		display: flex;
		flex-direction: column;
		gap: 5px;
		min-width: 0;
	}
	.top {
		display: flex;
		justify-content: space-between;
		gap: 8px;
		font-size: 12.5px;
	}
	.lbl {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
	.val {
		font-size: 12px;
		color: var(--color-neutral-300);
		white-space: nowrap;
	}
	.track {
		position: relative;
		height: 6px;
		border-radius: 3px;
		background: var(--color-neutral-800);
	}
	.compact .track {
		height: 4px;
		border-radius: 2px;
	}
	.fill {
		position: absolute;
		inset: 0 auto 0 0;
		border-radius: inherit;
	}
	.tick {
		position: absolute;
		top: -2px;
		bottom: -2px;
		width: 1px;
		background: var(--color-neutral-600);
	}
</style>
