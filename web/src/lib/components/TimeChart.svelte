<script lang="ts">
	// Time-series chart (uPlot) with a 1 h / 24 h / 7 d range switch. First series is the
	// accent line with a soft area; a second series is dashed neutral.
	import uPlot from 'uplot';
	import 'uplot/dist/uPlot.min.css';
	import type { RrdRange } from '#lib/api/client.js';
	import SegFilter from './SegFilter.svelte';

	export interface ChartSeries {
		label: string;
		values: (number | null)[];
		format: (v: number) => string;
	}

	let {
		title,
		times,
		series,
		range = $bindable<RrdRange>('hour'),
		ranges = true,
		height = 140,
		loading = false,
		yMax
	}: {
		title: string;
		times: number[];
		series: ChartSeries[];
		range?: RrdRange;
		ranges?: boolean;
		height?: number;
		loading?: boolean;
		yMax?: number;
	} = $props();

	let el: HTMLDivElement | undefined = $state();
	let chart: uPlot | null = null;
	let width = $state(0);

	const latest = $derived(
		series.map((s) => {
			for (let i = s.values.length - 1; i >= 0; i--) {
				const v = s.values[i];
				if (v != null) return s.format(v);
			}
			return '–';
		})
	);

	function hhmm(ts: number) {
		const d = new Date(ts * 1000);
		if (range === 'week') return `${d.getDate()}/${d.getMonth() + 1}`;
		return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
	}

	function options(w: number): uPlot.Options {
		const fmt = series[0]?.format ?? ((v: number) => String(v));
		return {
			width: w,
			height,
			legend: { show: false },
			cursor: { points: { size: 6 } },
			scales: {
				x: { time: false },
				y: { range: (_u, _min, max) => [0, yMax ?? (max > 0 ? max * 1.1 : 1)] }
			},
			axes: [
				{
					stroke: '#9397ab',
					grid: { show: false },
					ticks: { show: false },
					font: '10px "JetBrains Mono", monospace',
					values: (_u, vals) => vals.map(hhmm),
					space: 70,
					size: 22,
					gap: 4
				},
				{
					stroke: '#9397ab',
					grid: { stroke: '#3f424d', width: 1 },
					ticks: { show: false },
					font: '10px "JetBrains Mono", monospace',
					values: (_u, vals) => vals.map((v) => fmt(v)),
					size: 64,
					space: 36
				}
			],
			series: [
				{},
				...series.map((s, i) => ({
					label: s.label,
					stroke: i === 0 ? '#b5abfc' : '#9397ab',
					width: i === 0 ? 1.6 : 1.4,
					dash: i === 0 ? undefined : [4, 3],
					fill: i === 0 ? 'rgba(145,132,217,0.12)' : undefined,
					points: { show: false },
					spanGaps: false,
					value: (_u: uPlot, v: number | null) => (v == null ? '–' : s.format(v))
				}))
			]
		};
	}

	$effect(() => {
		if (!el || width <= 0) return;
		const data = [times, ...series.map((s) => s.values)] as uPlot.AlignedData;
		if (!chart || chart.series.length !== series.length + 1) {
			chart?.destroy();
			chart = new uPlot(options(width), data, el);
		} else {
			chart.setData(data);
		}
	});

	$effect(() => {
		if (chart && width > 0) chart.setSize({ width, height });
	});

	$effect(() => () => {
		chart?.destroy();
		chart = null;
	});
</script>

<div class="chart-card">
	<div class="head">
		<span class="title">{title}</span>
		{#if ranges}
			<SegFilter
				small
				label="Range"
				options={[
					{ value: 'hour', label: '1 h' },
					{ value: 'day', label: '24 h' },
					{ value: 'week', label: '7 d' }
				]}
				bind:value={range}
			/>
		{/if}
	</div>
	<div class="legend">
		{#each series as s, i (s.label)}
			<span class="key">
				<span class="swatch" class:dashed={i > 0}></span>{s.label}
				<span class="num">{latest[i]}</span>
			</span>
		{/each}
	</div>
	<div class="plot" bind:clientWidth={width} style:height="{height}px">
		{#if loading && times.length === 0}
			<div class="skeleton" style:height="{height}px"></div>
		{:else if times.length === 0}
			<div class="empty faint" style:height="{height}px">No data</div>
		{/if}
		<div bind:this={el}></div>
	</div>
</div>

<style>
	.chart-card {
		background: var(--color-surface);
		border-radius: var(--radius-md);
		padding: 14px 16px;
		display: flex;
		flex-direction: column;
		gap: 10px;
		min-width: 0;
	}
	.head {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 8px;
	}
	.title {
		font-size: 13px;
		font-weight: 500;
	}
	.legend {
		display: flex;
		gap: 16px;
		font-size: 11.5px;
		color: var(--color-neutral-400);
		flex-wrap: wrap;
	}
	.key {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}
	.key .num {
		color: var(--color-text);
	}
	.swatch {
		width: 12px;
		height: 2px;
		background: #b5abfc;
	}
	.swatch.dashed {
		height: 0;
		background: none;
		border-top: 2px dashed #9397ab;
	}
	.plot {
		position: relative;
		min-width: 0;
	}
	.plot > .skeleton,
	.plot > .empty {
		position: absolute;
		inset: 0;
		display: grid;
		place-items: center;
		font-size: 12px;
	}
	.plot :global(.u-over),
	.plot :global(.u-under) {
		cursor: crosshair;
	}
</style>
