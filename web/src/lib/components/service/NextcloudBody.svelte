<script lang="ts">
	// Nextcloud (read-only): storage, active users, files, updates, PHP and database.
	import type { NextcloudData } from '#lib/api/client.js';
	import { bytes, count, usedOf } from '#lib/format.js';
	import Banner from '../Banner.svelte';
	import ResourceBar from '../ResourceBar.svelte';
	import StatTile from '../StatTile.svelte';

	let { data }: { data: NextcloudData } = $props();
	const memUsed = $derived(
		data.mem_total != null && data.mem_free != null ? data.mem_total - data.mem_free : null
	);
	const rows = $derived(
		[
			['Web server', data.webserver],
			['PHP', data.php_version],
			['PHP memory limit', data.php_memory_limit != null ? bytes(data.php_memory_limit) : null],
			['Database', data.database],
			['Database size', data.database_size != null ? bytes(data.database_size) : null],
			['Apps installed', data.apps_installed != null ? String(data.apps_installed) : null],
			[
				'Load (1, 5, 15 min)',
				data.cpu_load.length ? data.cpu_load.map((l) => l.toFixed(2)).join('  ') : null
			]
		].filter((r): r is [string, string] => r[1] != null)
	);
</script>

{#if data.update_available}
	<Banner
		tone="progress"
		icon="ph-arrow-circle-up"
		text="Nextcloud {data.update_available} is available"
	/>
{/if}
{#if data.app_updates > 0}
	<Banner
		tone="progress"
		icon="ph-puzzle-piece"
		text="{data.app_updates} app update{data.app_updates === 1 ? '' : 's'} available"
	/>
{/if}

<div class="stats">
	<StatTile
		label="Active users · 24 h"
		value={data.active_24h}
		trend="{data.active_1h} in the last hour · {data.active_5m} now"
	/>
	<StatTile label="Users" value={data.users} />
	<StatTile label="Files" value={count(data.files)} trend="{count(data.shares)} shares" />
	<StatTile
		label="Free space"
		value={data.free_space != null ? bytes(data.free_space).split(' ')[0] : '–'}
		unit={data.free_space != null ? bytes(data.free_space).split(' ')[1] : ''}
	/>
</div>

<div class="two">
	<section class="card pad">
		<h2 class="section-title">Server</h2>
		<dl>
			{#each rows as [k, v] (k)}
				<dt>{k}</dt>
				<dd class="mono">{v}</dd>
			{/each}
		</dl>
	</section>
	{#if data.mem_total}
		<section class="card pad">
			<h2 class="section-title">Memory</h2>
			<ResourceBar
				label="RAM"
				fraction={memUsed != null ? memUsed / data.mem_total : null}
				value={usedOf(memUsed, data.mem_total)}
			/>
		</section>
	{/if}
</div>

<style>
	.stats {
		display: grid;
		gap: 10px;
		grid-template-columns: repeat(4, minmax(0, 1fr));
	}
	.two {
		display: grid;
		gap: 16px;
		grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
		align-items: start;
	}
	.pad {
		padding: 16px;
	}
	.section-title {
		margin-bottom: 10px;
	}
	dl {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 8px 16px;
		margin: 0;
		font-size: 13px;
	}
	dt {
		color: var(--color-neutral-400);
	}
	dd {
		margin: 0;
	}
	@media (max-width: 900px) {
		.stats {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
		.two {
			grid-template-columns: 1fr;
		}
	}
</style>
