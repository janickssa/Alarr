<script lang="ts">
	// Navidrome: now playing, library counts, last scan, scan library.
	import type { NavidromeData, ServiceView } from '#lib/api/client.js';
	import { runAction } from '#lib/actions.svelte.js';
	import { clock, dateTime, relative, count } from '#lib/format.js';
	import { session } from '#lib/session.svelte.js';
	import EmptyState from '../EmptyState.svelte';
	import Icon from '../Icon.svelte';
	import StatTile from '../StatTile.svelte';

	let { service, data }: { service: ServiceView; data: NavidromeData } = $props();
	const canAct = $derived(session.canAct(service.id));
</script>

<div class="stats">
	<StatTile label="Playing now" value={data.now_playing.length} />
	<StatTile label="Songs" value={count(data.songs)} />
	<StatTile label="Folders" value={count(data.folders)} />
	<StatTile
		label="Last scan"
		value={data.scanning ? 'Scanning…' : data.last_scan ? relative(data.last_scan) : '–'}
	/>
</div>

<div class="row">
	<button
		class="btn btn-secondary btn-sm"
		disabled={!canAct || data.scanning}
		onclick={() => void runAction(service.id, 'scan', { label: 'Scanning music library' })}
	>
		<Icon name="ph-arrows-clockwise" spin={data.scanning} />Scan library
	</button>
	<button
		class="btn btn-ghost btn-sm"
		disabled={!canAct || data.scanning}
		onclick={() =>
			void runAction(service.id, 'full_scan', { label: 'Full rescan of music library' })}
	>
		Full rescan
	</button>
	{#if data.last_scan}<span class="caption faint">Last scan {dateTime(data.last_scan)}</span>{/if}
</div>

<section>
	<h2 class="section-title">Now playing</h2>
	{#if data.now_playing.length === 0}
		<EmptyState icon="ph-music-notes" title="Nothing playing" />
	{:else}
		<ul class="list">
			{#each data.now_playing as n, i (i)}
				<li class="item">
					<div class="cover">
						<Icon name="ph-music-notes" size={20} color="oklch(0.84 0.07 10)" />
					</div>
					<div class="txt">
						<span class="title">{n.title}</span>
						<span class="caption muted">{[n.artist, n.album].filter(Boolean).join(' · ')}</span>
					</div>
					<span class="caption muted">{n.user}{n.player ? ` · ${n.player}` : ''}</span>
					{#if n.duration_s}<span class="mono caption faint">{clock(n.duration_s)}</span>{/if}
				</li>
			{/each}
		</ul>
	{/if}
</section>

<style>
	.stats {
		display: grid;
		gap: 10px;
		grid-template-columns: repeat(4, minmax(0, 1fr));
	}
	.row {
		gap: 8px;
	}
	section {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.item {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 12px;
		border-radius: var(--radius-md);
		background: var(--color-surface);
	}
	.cover {
		width: 40px;
		height: 40px;
		border-radius: 6px;
		background: oklch(0.33 0.045 10);
		display: grid;
		place-items: center;
		flex: none;
	}
	.txt {
		flex: 1;
		display: flex;
		flex-direction: column;
	}
	.title {
		font-size: 14px;
	}
	@media (max-width: 900px) {
		.stats {
			grid-template-columns: repeat(2, minmax(0, 1fr));
		}
	}
</style>
