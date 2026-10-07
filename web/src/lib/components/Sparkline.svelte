<script lang="ts">
	// Small hand-written SVG sparkline (line + soft area).
	let {
		values,
		width = 176,
		height = 34,
		max,
		label
	}: {
		values: (number | null)[];
		width?: number;
		height?: number;
		max?: number;
		label?: string;
	} = $props();

	const paths = $derived.by(() => {
		const pts = values.map((v) => v ?? 0);
		if (pts.length < 2) return { line: '', area: '' };
		const top = max ?? Math.max(...pts, 0.0001) * 1.1;
		const line = pts
			.map(
				(v, i) =>
					`${i ? 'L' : 'M'}${((i / (pts.length - 1)) * width).toFixed(1)} ${(height - (v / top) * height).toFixed(1)}`
			)
			.join(' ');
		return { line, area: `${line} L${width} ${height} L0 ${height} Z` };
	});
</script>

<svg
	{width}
	{height}
	viewBox="0 0 {width} {height}"
	preserveAspectRatio="none"
	role={label ? 'img' : undefined}
	aria-label={label}
	aria-hidden={label ? undefined : 'true'}
>
	<path d={paths.area} fill="var(--chart-area)" opacity=".14" />
	<path
		d={paths.line}
		stroke="var(--chart-line)"
		stroke-width="1.5"
		fill="none"
		stroke-linejoin="round"
		vector-effect="non-scaling-stroke"
	/>
</svg>

<style>
	svg {
		display: block;
		max-width: 100%;
	}
</style>
