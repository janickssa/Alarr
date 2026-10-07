<script lang="ts">
	// 24 h uptime strip: one segment per bucket, coloured by the worst state seen.
	import type { Health } from '#lib/api/client.js';
	import { HEALTH } from '#lib/health.js';
	import { time } from '#lib/format.js';

	let {
		buckets,
		uptime,
		height = 20,
		labels = true
	}: {
		buckets: { start: string; health: Health; uptime?: number | null }[];
		uptime?: number | null;
		height?: number;
		labels?: boolean;
	} = $props();

	function color(h: Health) {
		if (h === 'healthy') return 'var(--health-healthy-strip)';
		if (h === 'warning') return 'var(--health-warning)';
		if (h === 'down') return 'var(--health-down)';
		return 'var(--color-neutral-800)';
	}
</script>

<div class="strip-wrap">
	<div class="strip" style:height="{height}px" role="img" aria-label="Uptime, last 24 hours">
		{#each buckets as b (b.start)}
			<div
				class="seg"
				style:background={color(b.health)}
				title="{time(b.start)} · {HEALTH[b.health].label}{b.uptime != null
					? ` · ${b.uptime} %`
					: ''}"
			></div>
		{/each}
	</div>
	{#if labels}
		<div class="legend mono">
			<span>24 h ago</span>
			{#if uptime != null}<span>{uptime} %</span>{/if}
			<span>now</span>
		</div>
	{/if}
</div>

<style>
	.strip-wrap {
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 0;
	}
	.strip {
		display: flex;
		gap: 2px;
	}
	.seg {
		flex: 1;
		border-radius: 1.5px;
		min-width: 2px;
	}
	.legend {
		display: flex;
		justify-content: space-between;
		font-size: 10px;
		color: var(--color-neutral-500);
	}
</style>
