<script lang="ts">
	import type { Node } from '#lib/api/client.js';
	import { celsius, duration, pct, usedOf } from '#lib/format.js';
	import HealthBadge from '../HealthBadge.svelte';
	import ResourceBar from '../ResourceBar.svelte';
	import Sparkline from '../Sparkline.svelte';

	let { node, cpuHistory = [] }: { node: Node; cpuHistory?: (number | null)[] } = $props();
	const frac = (u?: number | null, t?: number | null) => (u != null && t ? u / t : null);
</script>

<div class="card node">
	<div class="row">
		<span class="name mono">{node.name}</span>
		{#if node.ip}<span class="mono faint ip">{node.ip}</span>{/if}
		<span class="spacer"></span>
		<HealthBadge
			health={node.online ? 'healthy' : 'down'}
			label={node.online ? 'Online' : 'Offline'}
			small
		/>
	</div>
	{#if node.online}
		<div class="spark">
			<div class="row caption">
				<span class="muted">CPU · 1 h</span><span class="spacer"></span><span class="mono num"
					>{pct(node.cpu_usage)}</span
				>
			</div>
			<Sparkline values={cpuHistory} width={300} height={30} label="CPU usage, last hour" />
		</div>
		<ResourceBar
			label="CPU · {node.cpu_count ?? '?'} cores"
			fraction={node.cpu_usage}
			value={pct(node.cpu_usage)}
		/>
		<ResourceBar
			label="RAM"
			fraction={frac(node.mem_used, node.mem_total)}
			value="{usedOf(node.mem_used, node.mem_total)} · {pct(frac(node.mem_used, node.mem_total))}"
		/>
		<ResourceBar
			label="Root disk"
			fraction={frac(node.disk_used, node.disk_total)}
			value={usedOf(node.disk_used, node.disk_total)}
		/>
		<div class="facts caption">
			<span><span class="faint">Up</span> {duration(node.uptime_s)}</span>
			{#if node.load_avg?.length}<span
					><span class="faint">Load</span>
					<span class="mono">{node.load_avg.map((l) => l.toFixed(2)).join(' ')}</span></span
				>{/if}
			{#if node.temperature_c != null}<span
					><span class="faint">Temp</span> {celsius(node.temperature_c)}</span
				>{/if}
			{#if node.pve_version}<span
					><span class="faint">PVE</span> <span class="mono">{node.pve_version}</span></span
				>{/if}
		</div>
	{:else}
		<p class="muted caption">This node is not responding. Its guests are not shown as running.</p>
	{/if}
</div>

<style>
	.node {
		gap: 12px;
		min-width: 0;
	}
	.name {
		font-size: 14px;
	}
	.ip {
		font-size: 11.5px;
	}
	.spark {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.facts {
		display: flex;
		flex-wrap: wrap;
		gap: 4px 14px;
		color: var(--color-neutral-300);
	}
</style>
